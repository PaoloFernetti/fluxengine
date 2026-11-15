//! Командный буфер Vulkan: запись, барьеры, submit с ожиданием fence.

#![allow(unsafe_code)]

use std::sync::Arc;

use ash::vk;

use super::super::cmd::{BarrierBatch, CommandBuffer};
use super::super::error::RhiError;
use super::super::types::{
    BufferId, DescriptorSetId, ImageId, PipelineId, PipelineKind, QueryPoolId, RenderPassDesc,
};
use super::{convert, DeviceContext};

/// Командный буфер Vulkan.
pub struct VulkanCommandBuffer {
    context: Arc<DeviceContext>,
    command: vk::CommandBuffer,
    recording: bool,
    in_pass: bool,
    pipeline: Option<PipelineId>,
    pipeline_kind: Option<PipelineKind>,
    pipeline_layout: Option<vk::PipelineLayout>,
}

impl VulkanCommandBuffer {
    /// Выделяет primary-буфер из пула устройства.
    pub(crate) fn new(context: Arc<DeviceContext>) -> Result<Self, RhiError> {
        let info = vk::CommandBufferAllocateInfo::default()
            .command_pool(context.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: pool and allocation info are valid.
        let command =
            unsafe { context.device.allocate_command_buffers(&info) }.map_err(convert::map_vk)?[0];
        Ok(Self {
            context,
            command,
            recording: false,
            in_pass: false,
            pipeline: None,
            pipeline_kind: None,
            pipeline_layout: None,
        })
    }

    fn require_recording(&self) -> Result<(), RhiError> {
        if self.recording {
            Ok(())
        } else {
            Err(RhiError::validation("command buffer is not recording"))
        }
    }

    fn submit(&self) -> Result<(), RhiError> {
        let device = &self.context.device;
        let fence_info = vk::FenceCreateInfo::default();
        // SAFETY: fence create info is valid.
        let fence = unsafe { device.create_fence(&fence_info, None) }.map_err(convert::map_vk)?;
        let command_buffers = [self.command];
        let submit_info = vk::SubmitInfo::default().command_buffers(&command_buffers);
        // SAFETY: command buffer is recorded and the queue belongs to the device.
        let submitted = unsafe { device.queue_submit(self.context.queue, &[submit_info], fence) };
        let result = match submitted {
            Ok(()) => {
                // SAFETY: fence was submitted; waiting is valid.
                let waited = unsafe { device.wait_for_fences(&[fence], true, u64::MAX) };
                waited.map_err(convert::map_vk)
            }
            Err(error) => Err(convert::map_vk(error)),
        };
        // SAFETY: fence is no longer used.
        unsafe { device.destroy_fence(fence, None) };
        result
    }
}

impl CommandBuffer for VulkanCommandBuffer {
    fn begin(&mut self) -> Result<(), RhiError> {
        if self.recording {
            return Err(RhiError::validation("command buffer already recording"));
        }
        let info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        // SAFETY: command buffer is allocated and not recording.
        unsafe {
            self.context
                .device
                .begin_command_buffer(self.command, &info)
        }
        .map_err(convert::map_vk)?;
        self.recording = true;
        Ok(())
    }

    fn end(&mut self) -> Result<(), RhiError> {
        self.require_recording()?;
        if self.in_pass {
            return Err(RhiError::validation("render pass is still open"));
        }
        // SAFETY: command buffer is recording.
        unsafe { self.context.device.end_command_buffer(self.command) }.map_err(convert::map_vk)?;
        self.recording = false;
        Ok(())
    }

    fn begin_render_pass(&mut self, desc: &RenderPassDesc) -> Result<(), RhiError> {
        self.require_recording()?;
        if self.in_pass {
            return Err(RhiError::validation("render pass already open"));
        }
        let (render_pass, framebuffer, width, height) = {
            let mut store = self.context.store.lock();
            store.render_pass_and_framebuffer(&self.context.device, desc)?
        };
        let color = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: desc.clear_color,
            },
        };
        let mut clear_values = Vec::with_capacity(desc.color_attachments.len() + 1);
        for _ in &desc.color_attachments {
            clear_values.push(color);
        }
        if desc.depth_attachment.is_some() {
            clear_values.push(vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    // Reversed-Z: дальняя плоскость — 0.0 (ADR-0008).
                    depth: 0.0,
                    stencil: 0,
                },
            });
        }
        let info = vk::RenderPassBeginInfo::default()
            .render_pass(render_pass)
            .framebuffer(framebuffer)
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: vk::Extent2D { width, height },
            })
            .clear_values(&clear_values);
        // SAFETY: render pass and framebuffer were created for these attachments.
        unsafe {
            self.context.device.cmd_begin_render_pass(
                self.command,
                &info,
                vk::SubpassContents::INLINE,
            );
        }
        // Pipeline использует dynamic viewport/scissor (ADR-0035): без явной
        // установки draw не растеризует ни одного фрагмента.
        let viewport = vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: width as f32,
            height: height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        };
        let scissor = vk::Rect2D {
            offset: vk::Offset2D { x: 0, y: 0 },
            extent: vk::Extent2D { width, height },
        };
        // SAFETY: command buffer is inside the render pass; extent matches.
        unsafe {
            self.context
                .device
                .cmd_set_viewport(self.command, 0, std::slice::from_ref(&viewport));
            self.context
                .device
                .cmd_set_scissor(self.command, 0, std::slice::from_ref(&scissor));
        }
        self.in_pass = true;
        Ok(())
    }

    fn end_render_pass(&mut self) -> Result<(), RhiError> {
        if !self.in_pass {
            return Err(RhiError::validation("no render pass is open"));
        }
        // SAFETY: a render pass is open on this command buffer.
        unsafe { self.context.device.cmd_end_render_pass(self.command) };
        self.in_pass = false;
        Ok(())
    }

    fn reset_query_pool(
        &mut self,
        pool: QueryPoolId,
        first: u32,
        count: u32,
    ) -> Result<(), RhiError> {
        self.require_recording()?;
        let (handle, capacity) = self.context.store.lock().query_pool(pool)?;
        let in_bounds = match first.checked_add(count) {
            Some(end) => count > 0 && end <= capacity,
            None => false,
        };
        if !in_bounds {
            return Err(RhiError::validation("query range is out of bounds"));
        }
        // SAFETY: handle belongs to the pool and the range is validated.
        unsafe {
            self.context
                .device
                .cmd_reset_query_pool(self.command, handle, first, count);
        }
        Ok(())
    }

    fn write_timestamp(&mut self, pool: QueryPoolId, index: u32) -> Result<(), RhiError> {
        self.require_recording()?;
        let (handle, capacity) = self.context.store.lock().query_pool(pool)?;
        if index >= capacity {
            return Err(RhiError::validation("timestamp index is out of bounds"));
        }
        // SAFETY: handle belongs to the pool and the index is validated.
        unsafe {
            self.context.device.cmd_write_timestamp(
                self.command,
                vk::PipelineStageFlags::ALL_COMMANDS,
                handle,
                index,
            );
        }
        Ok(())
    }

    fn set_pipeline(&mut self, pipeline: PipelineId) -> Result<(), RhiError> {
        self.require_recording()?;
        let (handle, kind, layout) = {
            let store = self.context.store.lock();
            let entry = store.pipeline_entry(pipeline)?;
            (entry.pipeline, entry.kind, entry.layout)
        };
        let bind_point = match kind {
            PipelineKind::Graphics => vk::PipelineBindPoint::GRAPHICS,
            PipelineKind::Compute => vk::PipelineBindPoint::COMPUTE,
        };
        // SAFETY: pipeline belongs to the device and is bound on this buffer.
        unsafe {
            self.context
                .device
                .cmd_bind_pipeline(self.command, bind_point, handle);
        }
        self.pipeline = Some(pipeline);
        self.pipeline_kind = Some(kind);
        self.pipeline_layout = Some(layout);
        Ok(())
    }

    fn push_constants(&mut self, data: &[u8]) -> Result<(), RhiError> {
        self.require_recording()?;
        if data.is_empty() || data.len() % 4 != 0 {
            return Err(RhiError::validation(
                "push constants must be non-empty and 4-byte aligned",
            ));
        }
        let (Some(layout), Some(kind), Some(pipeline)) =
            (self.pipeline_layout, self.pipeline_kind, self.pipeline)
        else {
            return Err(RhiError::validation("push constants require a pipeline"));
        };
        let size = {
            let store = self.context.store.lock();
            store.pipeline_entry(pipeline)?.push_constant_size
        };
        if data.len() as u32 > size {
            return Err(RhiError::validation(
                "push constants exceed the pipeline range",
            ));
        }
        let stages = match kind {
            PipelineKind::Graphics => vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            PipelineKind::Compute => vk::ShaderStageFlags::COMPUTE,
        };
        // SAFETY: layout belongs to the bound pipeline; data is 4-byte aligned.
        unsafe {
            self.context
                .device
                .cmd_push_constants(self.command, layout, stages, 0, data);
        }
        Ok(())
    }

    fn bind_descriptor_set(&mut self, set: DescriptorSetId) -> Result<(), RhiError> {
        self.require_recording()?;
        let (set, kind) = {
            let store = self.context.store.lock();
            let entry = store.descriptor_set(set)?;
            (entry.set, self.pipeline_kind)
        };
        let Some(layout) = self.pipeline_layout else {
            return Err(RhiError::validation(
                "descriptor set bound without a pipeline",
            ));
        };
        let bind_point = match kind {
            Some(PipelineKind::Graphics) => vk::PipelineBindPoint::GRAPHICS,
            Some(PipelineKind::Compute) => vk::PipelineBindPoint::COMPUTE,
            None => {
                return Err(RhiError::validation(
                    "descriptor set bound without a pipeline",
                ))
            }
        };
        // SAFETY: set is compatible with the pipeline layout (same bindings).
        unsafe {
            self.context.device.cmd_bind_descriptor_sets(
                self.command,
                bind_point,
                layout,
                0,
                std::slice::from_ref(&set),
                &[],
            );
        }
        Ok(())
    }

    fn copy_buffer_to_image(&mut self, buffer: BufferId, image: ImageId) -> Result<(), RhiError> {
        self.require_recording()?;
        if self.in_pass {
            return Err(RhiError::validation("copy inside a render pass"));
        }
        let (buffer, image, width, height, aspect) = {
            let store = self.context.store.lock();
            let source = store
                .buffers
                .get(&buffer.0)
                .ok_or_else(|| RhiError::validation("unknown source buffer"))?;
            // Контракт ADR-0039 — по usage-флагам: на lavapipe вся память
            // host-visible, и проверка по allocation пропускала бы storage.
            if !source.usage.is_staging() || !source.allocation.host_visible {
                return Err(RhiError::validation("copy source must be a staging buffer"));
            }
            let target = store.image_entry(image)?;
            (
                source.buffer,
                target.image,
                target.width,
                target.height,
                convert::aspect(target.format),
            )
        };
        let region = vk::BufferImageCopy::default()
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(aspect)
                    .mip_level(0)
                    .base_array_layer(0)
                    .layer_count(1),
            )
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            });
        // SAFETY: buffer/image belong to the device; caller set up the barriers.
        unsafe {
            self.context.device.cmd_copy_buffer_to_image(
                self.command,
                buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                std::slice::from_ref(&region),
            );
        }
        Ok(())
    }

    fn copy_image_to_buffer(&mut self, image: ImageId, buffer: BufferId) -> Result<(), RhiError> {
        self.require_recording()?;
        if self.in_pass {
            return Err(RhiError::validation("copy inside a render pass"));
        }
        let (image, buffer, width, height, aspect) = {
            let store = self.context.store.lock();
            let source = store.image_entry(image)?;
            let target = store
                .buffers
                .get(&buffer.0)
                .ok_or_else(|| RhiError::validation("unknown destination buffer"))?;
            if !target.usage.is_staging() || !target.allocation.host_visible {
                return Err(RhiError::validation(
                    "copy destination must be a staging buffer",
                ));
            }
            (
                source.image,
                target.buffer,
                source.width,
                source.height,
                convert::aspect(source.format),
            )
        };
        let region = vk::BufferImageCopy::default()
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(aspect)
                    .mip_level(0)
                    .base_array_layer(0)
                    .layer_count(1),
            )
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            });
        // SAFETY: buffer/image belong to the device; caller set up the barriers.
        unsafe {
            self.context.device.cmd_copy_image_to_buffer(
                self.command,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                buffer,
                std::slice::from_ref(&region),
            );
        }
        Ok(())
    }

    fn copy_buffer_to_buffer(
        &mut self,
        src: BufferId,
        dst: BufferId,
        size: u64,
    ) -> Result<(), RhiError> {
        self.require_recording()?;
        if self.in_pass {
            return Err(RhiError::validation("copy inside a render pass"));
        }
        if size == 0 {
            return Err(RhiError::validation("copy size must be non-zero"));
        }
        let (source, target) = {
            let store = self.context.store.lock();
            let source = store
                .buffers
                .get(&src.0)
                .ok_or_else(|| RhiError::validation("unknown source buffer"))?;
            if !source.usage.transfer_src {
                return Err(RhiError::validation(
                    "copy source must be a transfer source",
                ));
            }
            let target = store
                .buffers
                .get(&dst.0)
                .ok_or_else(|| RhiError::validation("unknown destination buffer"))?;
            if !target.usage.transfer_dst {
                return Err(RhiError::validation(
                    "copy destination must be a transfer destination",
                ));
            }
            if size > source.size || size > target.size {
                return Err(RhiError::validation("copy size exceeds buffer size"));
            }
            (source.buffer, target.buffer)
        };
        let region = vk::BufferCopy::default().size(size);
        // SAFETY: buffers belong to the device and the copy is in bounds.
        unsafe {
            self.context.device.cmd_copy_buffer(
                self.command,
                source,
                target,
                std::slice::from_ref(&region),
            );
            // Делаем запись transfer видимой шейдерным стадиям: следующий
            // submit читает storage-буфер в vertex pulling.
            let barrier = vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ);
            self.context.device.cmd_pipeline_barrier(
                self.command,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::VERTEX_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::DependencyFlags::empty(),
                std::slice::from_ref(&barrier),
                &[],
                &[],
            );
        }
        Ok(())
    }

    fn draw(&mut self, vertices: u32, instances: u32) -> Result<(), RhiError> {
        self.require_recording()?;
        if !self.in_pass {
            return Err(RhiError::validation("draw outside a render pass"));
        }
        if self.pipeline_kind != Some(PipelineKind::Graphics) {
            return Err(RhiError::validation(
                "draw without a bound graphics pipeline",
            ));
        }
        // SAFETY: draw is recorded inside an open render pass with a pipeline.
        unsafe {
            self.context
                .device
                .cmd_draw(self.command, vertices, instances, 0, 0);
        }
        Ok(())
    }

    fn dispatch(&mut self, groups: [u32; 3]) -> Result<(), RhiError> {
        self.require_recording()?;
        if self.in_pass {
            return Err(RhiError::validation("dispatch inside a render pass"));
        }
        if self.pipeline_kind != Some(PipelineKind::Compute) {
            return Err(RhiError::validation(
                "dispatch without a bound compute pipeline",
            ));
        }
        if groups.contains(&0) {
            return Err(RhiError::validation("dispatch groups must be non-zero"));
        }
        // SAFETY: command buffer is recording outside a render pass.
        unsafe {
            self.context
                .device
                .cmd_dispatch(self.command, groups[0], groups[1], groups[2]);
        }
        Ok(())
    }

    fn barriers(&mut self, batch: &BarrierBatch) -> Result<(), RhiError> {
        self.require_recording()?;
        for barrier in batch.barriers() {
            let (image, aspect) = {
                let store = self.context.store.lock();
                let entry = store.image_entry(barrier.image)?;
                (entry.image, convert::aspect(entry.format))
            };
            let from = convert::image_state(barrier.from);
            let to = convert::image_state(barrier.to);
            let image_barrier = vk::ImageMemoryBarrier::default()
                .src_access_mask(from.access)
                .dst_access_mask(to.access)
                .old_layout(from.layout)
                .new_layout(to.layout)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(aspect)
                        .base_mip_level(0)
                        .level_count(vk::REMAINING_MIP_LEVELS)
                        .base_array_layer(0)
                        .layer_count(vk::REMAINING_ARRAY_LAYERS),
                );
            // SAFETY: image belongs to the device and the transition is explicit.
            unsafe {
                self.context.device.cmd_pipeline_barrier(
                    self.command,
                    from.stage,
                    to.stage,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    std::slice::from_ref(&image_barrier),
                );
            }
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<(), RhiError> {
        if self.recording {
            return Err(RhiError::validation(
                "command buffer must be ended before finish",
            ));
        }
        self.submit()
    }
}

impl Drop for VulkanCommandBuffer {
    fn drop(&mut self) {
        let device = &self.context.device;
        if self.recording {
            // SAFETY: recording buffer can be ended before it is freed.
            unsafe {
                let _ = device.end_command_buffer(self.command);
            }
        }
        // SAFETY: command buffer was allocated from this pool and is freed once.
        unsafe {
            device.free_command_buffers(self.context.command_pool, &[self.command]);
        }
    }
}
