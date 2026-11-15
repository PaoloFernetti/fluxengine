//! Vulkan-бэкенд (ADR-0034): instance/device/queues, память, offscreen swapchain.
//!
//! Бэкенд включается feature-гейтом `vulkan` и загружает Vulkan loader в
//! рантайме (`ash` с feature `loaded`), поэтому сборка не требует Vulkan SDK.
//! Если loader или GPU недоступны, создание устройства возвращает
//! [`RhiError::Unsupported`], а не паникует: headless-null остаётся дефолтом.
//!
//! `unsafe` изолирован в этом модуле: RHI-контракты выше остаются безопасными.

#![allow(unsafe_code)]

mod command;
mod convert;
mod debug;
mod memory;
mod swapchain;

use std::collections::BTreeMap;
use std::ffi::CStr;
use std::sync::Arc;

use ash::vk;
use flux_core::memory::MemoryTag;
use parking_lot::Mutex;

use super::cmd::CommandBuffer;
use super::device::RhiDevice;
use super::error::RhiError;
use super::swapchain::Swapchain;
use super::types::{
    BackendKind, BindingKind, BufferId, BufferUsage, Capabilities, DescriptorBinding,
    DescriptorResource, DescriptorSetDesc, DescriptorSetId, DescriptorWrite, DeviceSignature,
    GpuBufferDesc, GpuImageDesc, ImageId, PipelineDesc, PipelineId, PipelineKind, PresentMode,
    PrimitiveTopology, QueryPoolDesc, QueryPoolId, RenderPassDesc, SamplerDesc, SamplerId,
    SurfaceId, SurfaceTarget, SwapchainDesc, TextureFormat, WindowSurface,
};
use super::vram::{BudgetSnapshot, VramAllocator, VramLease};
use memory::{Allocation, MemoryState};

pub use command::VulkanCommandBuffer;
pub use debug::error_count as validation_error_count;
pub use swapchain::{VulkanPresentSwapchain, VulkanSwapchain};

/// Контекст устройства, разделяемый командными буферами и swapchain.
pub(crate) struct DeviceContext {
    /// Держит библиотеку loader-а живой до уничтожения instance.
    #[allow(dead_code)]
    pub(crate) entry: ash::Entry,
    pub(crate) instance: ash::Instance,
    /// Физическое устройство, выбранное при создании.
    pub(crate) physical: vk::PhysicalDevice,
    pub(crate) device: ash::Device,
    pub(crate) queue: vk::Queue,
    /// Семейство graphics-очереди.
    pub(crate) queue_family: u32,
    pub(crate) command_pool: vk::CommandPool,
    pub(crate) caps: Capabilities,
    pub(crate) validation: Option<debug::DebugMessenger>,
    pub(crate) store: Mutex<ResourceStore>,
    /// Кэш PSO (ADR-0034): общий для всех pipeline устройства.
    pub(crate) pipeline_cache: Mutex<Option<vk::PipelineCache>>,
    /// `VK_KHR_surface`: destroy_surface и запросы возможностей.
    pub(crate) surface_loader: ash::khr::surface::Instance,
    pub(crate) headless_surface: Option<ash::ext::headless_surface::Instance>,
    #[cfg(target_os = "windows")]
    pub(crate) win32_surface: Option<ash::khr::win32_surface::Instance>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub(crate) xlib_surface: Option<ash::khr::xlib_surface::Instance>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub(crate) wayland_surface: Option<ash::khr::wayland_surface::Instance>,
    #[cfg(all(unix, not(target_os = "macos")))]
    pub(crate) xcb_surface: Option<ash::khr::xcb_surface::Instance>,
    /// `VK_KHR_swapchain`; `None` — устройство не умеет present.
    pub(crate) swapchain_loader: Option<ash::khr::swapchain::Device>,
    /// Доступно ли `VK_KHR_surface`.
    pub(crate) surface_supported: bool,
    api_version: u32,
    device_name: String,
}

impl DeviceContext {
    /// Освобождает все ресурсы, созданные через устройство.
    pub(crate) fn destroy_resources(&self) {
        let mut store = self.store.lock();
        let device = &self.device;
        for (_, entry) in std::mem::take(&mut store.buffers) {
            // SAFETY: resource is owned by the store and destroyed exactly once.
            unsafe { device.destroy_buffer(entry.buffer, None) };
            store.memory.free(device, entry.allocation);
        }
        for (_, entry) in std::mem::take(&mut store.images) {
            // Образы swapchain освобождает сама цепочка (ADR-0045).
            if entry.owned {
                // SAFETY: resource is owned by the store and destroyed exactly once.
                unsafe {
                    device.destroy_image_view(entry.view, None);
                    device.destroy_image(entry.image, None);
                }
            }
            if let Some(allocation) = entry.allocation {
                store.memory.free(device, allocation);
            }
        }
        for (_, entry) in std::mem::take(&mut store.pipelines) {
            // SAFETY: pipeline, layout and modules are owned by the store.
            unsafe {
                device.destroy_pipeline(entry.pipeline, None);
                device.destroy_pipeline_layout(entry.layout, None);
                for module in entry.modules {
                    device.destroy_shader_module(module, None);
                }
            }
        }
        for (_, entry) in std::mem::take(&mut store.query_pools) {
            // SAFETY: query pool is owned by the store and destroyed exactly once.
            unsafe { device.destroy_query_pool(entry.pool, None) };
        }
        for (_, entry) in std::mem::take(&mut store.samplers) {
            // SAFETY: sampler is owned by the store and destroyed exactly once.
            unsafe { device.destroy_sampler(entry.sampler, None) };
        }
        if let Some(pool) = store.descriptor_pool.take() {
            let sets: Vec<vk::DescriptorSet> = std::mem::take(&mut store.descriptor_sets)
                .into_values()
                .map(|entry| entry.set)
                .collect();
            if !sets.is_empty() {
                // SAFETY: sets were allocated from this pool.
                unsafe {
                    let _ = device.free_descriptor_sets(pool, &sets);
                }
            }
            // SAFETY: pool is owned by the store and destroyed exactly once.
            unsafe { device.destroy_descriptor_pool(pool, None) };
        }
        if let Some(cache) = self.pipeline_cache.lock().take() {
            // SAFETY: cache is owned by the context and destroyed exactly once.
            unsafe { device.destroy_pipeline_cache(cache, None) };
        }
        for (_, layout) in std::mem::take(&mut store.descriptor_layouts) {
            // SAFETY: layout is owned by the store and destroyed exactly once.
            unsafe { device.destroy_descriptor_set_layout(layout, None) };
        }
        for (_, framebuffer) in std::mem::take(&mut store.framebuffers) {
            // SAFETY: framebuffer is owned by the store and destroyed exactly once.
            unsafe { device.destroy_framebuffer(framebuffer, None) };
        }
        for (_, render_pass) in std::mem::take(&mut store.render_passes) {
            // SAFETY: render pass is owned by the store and destroyed exactly once.
            unsafe { device.destroy_render_pass(render_pass, None) };
        }
        for (_, surface) in std::mem::take(&mut store.surfaces) {
            // SAFETY: surface is owned by the store and destroyed exactly once;
            // swapchains that used it were dropped before the context (Arc).
            unsafe { self.surface_loader.destroy_surface(surface, None) };
        }
    }
}

impl Drop for DeviceContext {
    fn drop(&mut self) {
        // SAFETY: device is idle before resources and the device itself are destroyed.
        unsafe {
            let _ = self.device.device_wait_idle();
        }
        self.destroy_resources();
        // SAFETY: pool/device/messenger/instance are owned by this context.
        unsafe {
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
        }
        if let Some(messenger) = self.validation.take() {
            messenger.destroy(&self.instance);
        }
        // SAFETY: instance is owned by this context and destroyed last.
        unsafe {
            self.instance.destroy_instance(None);
        }
    }
}

/// Запись буфера в реестре ресурсов.
pub(crate) struct BufferEntry {
    pub(crate) buffer: vk::Buffer,
    pub(crate) allocation: Allocation,
    pub(crate) size: u64,
    /// Флаги использования: staging-контракт проверяется по ним, а не по
    /// свойствам памяти (на lavapipe вся память host-visible).
    pub(crate) usage: BufferUsage,
}

/// Запись изображения в реестре ресурсов.
pub(crate) struct ImageEntry {
    pub(crate) image: vk::Image,
    pub(crate) view: vk::ImageView,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: TextureFormat,
    /// Аллокация устройства; `None` — образ принадлежит swapchain (ADR-0045).
    pub(crate) allocation: Option<Allocation>,
    /// `true` — образ создан `create_image`; `false` — образ swapchain.
    pub(crate) owned: bool,
}

/// Запись pipeline в реестре ресурсов.
pub(crate) struct PipelineEntry {
    pub(crate) pipeline: vk::Pipeline,
    pub(crate) layout: vk::PipelineLayout,
    pub(crate) push_constant_size: u32,
    pub(crate) modules: Vec<vk::ShaderModule>,
    pub(crate) kind: PipelineKind,
}

/// Запись пула запросов в реестре ресурсов.
pub(crate) struct QueryPoolEntry {
    pub(crate) pool: vk::QueryPool,
    pub(crate) count: u32,
}

/// Запись сэмплера в реестре ресурсов (ADR-0041).
pub(crate) struct SamplerEntry {
    pub(crate) sampler: vk::Sampler,
}

/// Запись дескрипторного набора в реестре ресурсов.
pub(crate) struct DescriptorSetEntry {
    pub(crate) set: vk::DescriptorSet,
    pub(crate) bindings: Vec<DescriptorBinding>,
}

/// Vulkan-тип дескриптора для вида биндинга.
const fn descriptor_type(kind: BindingKind) -> vk::DescriptorType {
    match kind {
        BindingKind::UniformBuffer => vk::DescriptorType::UNIFORM_BUFFER,
        BindingKind::StorageBuffer => vk::DescriptorType::STORAGE_BUFFER,
        BindingKind::SampledImage => vk::DescriptorType::SAMPLED_IMAGE,
        BindingKind::StorageImage => vk::DescriptorType::STORAGE_IMAGE,
        BindingKind::Sampler => vk::DescriptorType::SAMPLER,
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct RenderPassKey {
    colors: Vec<i32>,
    depth: Option<i32>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct FramebufferKey {
    colors: Vec<i32>,
    depth: Option<i32>,
    images: Vec<u64>,
    width: u32,
    height: u32,
}

/// Реестр ресурсов устройства: детерминированные id, кэш render pass/framebuffer.
pub(crate) struct ResourceStore {
    buffers: BTreeMap<u64, BufferEntry>,
    images: BTreeMap<u64, ImageEntry>,
    pipelines: BTreeMap<u64, PipelineEntry>,
    query_pools: BTreeMap<u64, QueryPoolEntry>,
    samplers: BTreeMap<u64, SamplerEntry>,
    descriptor_sets: BTreeMap<u64, DescriptorSetEntry>,
    descriptor_layouts: BTreeMap<Vec<DescriptorBinding>, vk::DescriptorSetLayout>,
    descriptor_pool: Option<vk::DescriptorPool>,
    render_passes: BTreeMap<RenderPassKey, vk::RenderPass>,
    framebuffers: BTreeMap<FramebufferKey, vk::Framebuffer>,
    /// Поверхности представления (`VkSurfaceKHR`).
    surfaces: BTreeMap<u64, vk::SurfaceKHR>,
    pub(crate) memory: MemoryState,
    next_id: u64,
}

impl ResourceStore {
    fn new(memory: MemoryState) -> Self {
        Self {
            buffers: BTreeMap::new(),
            images: BTreeMap::new(),
            pipelines: BTreeMap::new(),
            query_pools: BTreeMap::new(),
            samplers: BTreeMap::new(),
            descriptor_sets: BTreeMap::new(),
            descriptor_layouts: BTreeMap::new(),
            descriptor_pool: None,
            render_passes: BTreeMap::new(),
            framebuffers: BTreeMap::new(),
            surfaces: BTreeMap::new(),
            memory,
            next_id: 0,
        }
    }

    /// Следующий детерминированный id ресурса.
    pub(crate) fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        id
    }

    /// Число живых буферов и изображений.
    pub(crate) fn live_resources(&self) -> usize {
        self.buffers.len() + self.images.len()
    }

    /// Число живых pipeline.
    pub(crate) fn pipeline_count(&self) -> usize {
        self.pipelines.len()
    }

    /// Изображение по хэндлу.
    pub(crate) fn image_entry(&self, id: ImageId) -> Result<&ImageEntry, RhiError> {
        self.images.get(&id.0).ok_or(RhiError::InvalidHandle {
            kind: "image",
            id: id.0,
        })
    }

    /// Удаляет регистрацию образов swapchain (сами `VkImage` им не принадлежат).
    pub(crate) fn remove_swapchain_images(&mut self, ids: &[ImageId]) {
        for id in ids {
            self.images.remove(&id.0);
        }
    }

    /// Забирает кэш framebuffer-ов: после пересоздания образов swapchain
    /// старые `VkFramebuffer` ссылаются на уничтоженные view.
    pub(crate) fn take_framebuffers(&mut self) -> Vec<vk::Framebuffer> {
        std::mem::take(&mut self.framebuffers)
            .into_values()
            .collect()
    }

    /// Pipeline по хэндлу.
    pub(crate) fn pipeline_entry(&self, id: PipelineId) -> Result<&PipelineEntry, RhiError> {
        self.pipelines.get(&id.0).ok_or(RhiError::InvalidHandle {
            kind: "pipeline",
            id: id.0,
        })
    }

    /// Пул запросов и его ёмкость.
    pub(crate) fn query_pool(&self, id: QueryPoolId) -> Result<(vk::QueryPool, u32), RhiError> {
        let entry = self.query_pools.get(&id.0).ok_or(RhiError::InvalidHandle {
            kind: "query_pool",
            id: id.0,
        })?;
        Ok((entry.pool, entry.count))
    }

    /// Дескрипторный набор по хэндлу.
    pub(crate) fn descriptor_set(
        &self,
        id: DescriptorSetId,
    ) -> Result<&DescriptorSetEntry, RhiError> {
        self.descriptor_sets
            .get(&id.0)
            .ok_or(RhiError::InvalidHandle {
                kind: "descriptor_set",
                id: id.0,
            })
    }

    /// Layout набора дескрипторов (кэшируется по составу биндингов).
    pub(crate) fn ensure_descriptor_layout(
        &mut self,
        device: &ash::Device,
        bindings: &[DescriptorBinding],
    ) -> Result<vk::DescriptorSetLayout, RhiError> {
        let key = bindings.to_vec();
        if let Some(layout) = self.descriptor_layouts.get(&key) {
            return Ok(*layout);
        }
        let entries: Vec<vk::DescriptorSetLayoutBinding> = bindings
            .iter()
            .map(|binding| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(binding.binding)
                    .descriptor_type(descriptor_type(binding.kind))
                    .descriptor_count(binding.count)
                    .stage_flags(vk::ShaderStageFlags::ALL)
            })
            .collect();
        let info = vk::DescriptorSetLayoutCreateInfo::default().bindings(&entries);
        // SAFETY: bindings are validated by the RHI layer.
        let layout =
            unsafe { device.create_descriptor_set_layout(&info, None) }.map_err(convert::map_vk)?;
        self.descriptor_layouts.insert(key, layout);
        Ok(layout)
    }

    /// Пул дескрипторов устройства (создаётся лениво).
    pub(crate) fn ensure_descriptor_pool(
        &mut self,
        device: &ash::Device,
    ) -> Result<vk::DescriptorPool, RhiError> {
        if let Some(pool) = self.descriptor_pool {
            return Ok(pool);
        }
        let sizes = [
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(256),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(256),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .descriptor_count(256),
            vk::DescriptorPoolSize::default()
                .ty(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(256),
        ];
        let info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(256)
            .pool_sizes(&sizes)
            .flags(vk::DescriptorPoolCreateFlags::FREE_DESCRIPTOR_SET);
        // SAFETY: pool sizes are static and within driver limits.
        let pool =
            unsafe { device.create_descriptor_pool(&info, None) }.map_err(convert::map_vk)?;
        self.descriptor_pool = Some(pool);
        Ok(pool)
    }

    /// Возвращает render pass для набора форматов, создавая его при необходимости.
    pub(crate) fn ensure_render_pass(
        &mut self,
        device: &ash::Device,
        colors: &[i32],
        depth: Option<i32>,
    ) -> Result<vk::RenderPass, RhiError> {
        let key = RenderPassKey {
            colors: colors.to_vec(),
            depth,
        };
        if let Some(pass) = self.render_passes.get(&key) {
            return Ok(*pass);
        }
        let pass = create_render_pass(device, colors, depth)?;
        self.render_passes.insert(key, pass);
        Ok(pass)
    }

    /// Возвращает (render pass, framebuffer, width, height), создавая их при необходимости.
    pub(crate) fn render_pass_and_framebuffer(
        &mut self,
        device: &ash::Device,
        desc: &RenderPassDesc,
    ) -> Result<(vk::RenderPass, vk::Framebuffer, u32, u32), RhiError> {
        if !desc.has_attachments() {
            return Err(RhiError::validation(
                "render pass needs at least one attachment",
            ));
        }
        let mut colors = Vec::with_capacity(desc.color_attachments.len());
        let mut color_views = Vec::with_capacity(desc.color_attachments.len());
        let mut depth = None;
        let mut depth_view = None;
        let mut width = 0;
        let mut height = 0;
        for &id in &desc.color_attachments {
            let entry = self.image_entry(id)?;
            if entry.format.is_depth() {
                return Err(RhiError::validation(
                    "depth image cannot be a color attachment",
                ));
            }
            if width == 0 {
                width = entry.width;
                height = entry.height;
            } else if width != entry.width || height != entry.height {
                return Err(RhiError::validation(
                    "render pass attachments have different sizes",
                ));
            }
            colors.push(convert::format(entry.format).as_raw());
            color_views.push(entry.view);
        }
        if let Some(id) = desc.depth_attachment {
            let entry = self.image_entry(id)?;
            if !entry.format.is_depth() {
                return Err(RhiError::validation(
                    "depth attachment must use a depth format",
                ));
            }
            if width == 0 {
                width = entry.width;
                height = entry.height;
            } else if width != entry.width || height != entry.height {
                return Err(RhiError::validation(
                    "render pass attachments have different sizes",
                ));
            }
            depth = Some(convert::format(entry.format).as_raw());
            depth_view = Some(entry.view);
        }
        if width == 0 {
            return Err(RhiError::validation("render pass has no sized attachment"));
        }

        let render_pass = self.ensure_render_pass(device, &colors, depth)?;

        let mut image_ids: Vec<u64> = desc.color_attachments.iter().map(|id| id.0).collect();
        if let Some(id) = desc.depth_attachment {
            image_ids.push(id.0);
        }
        let framebuffer_key = FramebufferKey {
            colors,
            depth,
            images: image_ids,
            width,
            height,
        };
        let framebuffer = if let Some(framebuffer) = self.framebuffers.get(&framebuffer_key) {
            *framebuffer
        } else {
            let mut views = color_views;
            if let Some(view) = depth_view {
                views.push(view);
            }
            let info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(&views)
                .width(width)
                .height(height)
                .layers(1);
            // SAFETY: render pass and views are valid; framebuffer is owned by the store.
            let framebuffer =
                unsafe { device.create_framebuffer(&info, None) }.map_err(convert::map_vk)?;
            self.framebuffers.insert(framebuffer_key, framebuffer);
            framebuffer
        };

        Ok((render_pass, framebuffer, width, height))
    }
}

fn create_render_pass(
    device: &ash::Device,
    colors: &[i32],
    depth: Option<i32>,
) -> Result<vk::RenderPass, RhiError> {
    let mut attachments = Vec::with_capacity(colors.len() + usize::from(depth.is_some()));
    for &color in colors {
        attachments.push(
            vk::AttachmentDescription::default()
                .format(vk::Format::from_raw(color))
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL),
        );
    }
    if let Some(depth) = depth {
        attachments.push(
            vk::AttachmentDescription::default()
                .format(vk::Format::from_raw(depth))
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL),
        );
    }
    let color_refs: Vec<vk::AttachmentReference> = (0..colors.len())
        .map(|index| {
            vk::AttachmentReference::default()
                .attachment(index as u32)
                .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        })
        .collect();
    let depth_ref = depth.map(|_| {
        vk::AttachmentReference::default()
            .attachment(colors.len() as u32)
            .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
    });
    let mut subpass = vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_refs);
    if let Some(reference) = depth_ref.as_ref() {
        subpass = subpass.depth_stencil_attachment(reference);
    }
    let dependency = vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
        )
        .dst_stage_mask(
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
        )
        .src_access_mask(vk::AccessFlags::empty())
        .dst_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
        );
    let info = vk::RenderPassCreateInfo::default()
        .attachments(&attachments)
        .subpasses(std::slice::from_ref(&subpass))
        .dependencies(std::slice::from_ref(&dependency));
    // SAFETY: all slices outlive the call and describe a valid render pass.
    unsafe { device.create_render_pass(&info, None) }.map_err(convert::map_vk)
}

fn cstring(value: &str) -> Result<std::ffi::CString, RhiError> {
    std::ffi::CString::new(value)
        .map_err(|_| RhiError::validation("shader entry point contains NUL"))
}

fn create_shader_module(
    device: &ash::Device,
    bytes: &[u8],
    label: &'static str,
) -> Result<vk::ShaderModule, RhiError> {
    if bytes.is_empty() {
        return Err(RhiError::validation(format!(
            "pipeline '{label}' has an empty shader"
        )));
    }
    let mut cursor = std::io::Cursor::new(bytes);
    // SAFETY: reading SPIR-V only validates the byte stream.
    let code = ash::util::read_spv(&mut cursor)
        .map_err(|_| RhiError::validation(format!("pipeline '{label}' has invalid SPIR-V")))?;
    let info = vk::ShaderModuleCreateInfo::default().code(&code);
    // SAFETY: code is a valid SPIR-V word stream.
    unsafe { device.create_shader_module(&info, None) }.map_err(convert::map_vk)
}

const fn topology(topology: PrimitiveTopology) -> vk::PrimitiveTopology {
    match topology {
        PrimitiveTopology::TriangleList => vk::PrimitiveTopology::TRIANGLE_LIST,
        PrimitiveTopology::LineList => vk::PrimitiveTopology::LINE_LIST,
        PrimitiveTopology::PointList => vk::PrimitiveTopology::POINT_LIST,
    }
}

fn build_pipeline(
    device: &ash::Device,
    store: &Mutex<ResourceStore>,
    cache: vk::PipelineCache,
    desc: &PipelineDesc,
    modules: &mut Vec<vk::ShaderModule>,
    layout_slot: &mut Option<vk::PipelineLayout>,
) -> Result<vk::Pipeline, RhiError> {
    // Set 0 описывается биндингами pipeline; пустой layout — без дескрипторов.
    let set_layout = if desc.bindings.is_empty() {
        None
    } else {
        Some(
            store
                .lock()
                .ensure_descriptor_layout(device, desc.bindings)?,
        )
    };
    let set_layouts: Vec<vk::DescriptorSetLayout> = set_layout.into_iter().collect();
    // Push constants (ADR-0042): диапазон на все graphics/compute стадии.
    let push_range = (desc.push_constant_size > 0).then(|| {
        let stages = match desc.kind {
            PipelineKind::Graphics => vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            PipelineKind::Compute => vk::ShaderStageFlags::COMPUTE,
        };
        vk::PushConstantRange::default()
            .stage_flags(stages)
            .offset(0)
            .size(desc.push_constant_size)
    });
    let push_ranges: Vec<vk::PushConstantRange> = push_range.into_iter().collect();
    let layout_info = vk::PipelineLayoutCreateInfo::default()
        .set_layouts(&set_layouts)
        .push_constant_ranges(&push_ranges);
    // SAFETY: layout description references layouts owned by the store.
    let layout =
        unsafe { device.create_pipeline_layout(&layout_info, None) }.map_err(convert::map_vk)?;
    *layout_slot = Some(layout);
    match desc.kind {
        PipelineKind::Graphics => {
            if desc.vertex_shader.is_empty() || desc.fragment_shader.is_empty() {
                return Err(RhiError::validation(
                    "graphics pipeline requires vertex and fragment SPIR-V",
                ));
            }
            if desc.color_formats.is_empty() {
                return Err(RhiError::validation(
                    "graphics pipeline requires color formats",
                ));
            }
            let colors: Vec<i32> = desc
                .color_formats
                .iter()
                .map(|format| convert::format(*format).as_raw())
                .collect();
            let depth = desc
                .depth_format
                .map(|format| convert::format(format).as_raw());
            let render_pass = store.lock().ensure_render_pass(device, &colors, depth)?;
            let vertex = create_shader_module(device, desc.vertex_shader, desc.label)?;
            modules.push(vertex);
            let fragment = create_shader_module(device, desc.fragment_shader, desc.label)?;
            modules.push(fragment);
            let vertex_entry = cstring(desc.vertex_entry)?;
            let fragment_entry = cstring(desc.fragment_entry)?;
            let stages = [
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(vertex)
                    .name(&vertex_entry),
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(fragment)
                    .name(&fragment_entry),
            ];
            let binding = vk::VertexInputBindingDescription::default()
                .binding(0)
                .stride(desc.vertex_stride.max(12))
                .input_rate(vk::VertexInputRate::VERTEX);
            let attribute = vk::VertexInputAttributeDescription::default()
                .location(0)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(0);
            // `vertex_stride == 0` — pipeline без vertex buffer (например,
            // полноэкранный треугольник из SV_VertexID, ADR-0035).
            let vertex_input = if desc.vertex_stride == 0 {
                vk::PipelineVertexInputStateCreateInfo::default()
            } else {
                vk::PipelineVertexInputStateCreateInfo::default()
                    .vertex_binding_descriptions(std::slice::from_ref(&binding))
                    .vertex_attribute_descriptions(std::slice::from_ref(&attribute))
            };
            let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
                .topology(topology(desc.topology))
                .primitive_restart_enable(false);
            let viewport_state = vk::PipelineViewportStateCreateInfo::default()
                .viewport_count(1)
                .scissor_count(1);
            let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
                .polygon_mode(vk::PolygonMode::FILL)
                .cull_mode(vk::CullModeFlags::NONE)
                .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                .line_width(1.0);
            let multisample = vk::PipelineMultisampleStateCreateInfo::default()
                .rasterization_samples(vk::SampleCountFlags::TYPE_1);
            let blend_attachments: Vec<vk::PipelineColorBlendAttachmentState> = colors
                .iter()
                .map(|_| {
                    vk::PipelineColorBlendAttachmentState::default()
                        .blend_enable(false)
                        .color_write_mask(vk::ColorComponentFlags::RGBA)
                })
                .collect();
            let color_blend =
                vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);
            let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
            let dynamic =
                vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
            let depth_stencil = if depth.is_some() {
                vk::PipelineDepthStencilStateCreateInfo::default()
                    .depth_test_enable(true)
                    .depth_write_enable(true)
                    // Reversed-Z: ближе — больше (ADR-0008).
                    .depth_compare_op(vk::CompareOp::GREATER_OR_EQUAL)
            } else {
                vk::PipelineDepthStencilStateCreateInfo::default()
            };
            let info = vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vertex_input)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterization)
                .multisample_state(&multisample)
                .color_blend_state(&color_blend)
                .dynamic_state(&dynamic)
                .depth_stencil_state(&depth_stencil)
                .layout(layout)
                .render_pass(render_pass)
                .subpass(0);
            // SAFETY: all descriptions outlive the call; cache is owned by the context.
            let pipelines = unsafe { device.create_graphics_pipelines(cache, &[info], None) }
                .map_err(|(_, error)| convert::map_vk(error))?;
            Ok(pipelines[0])
        }
        PipelineKind::Compute => {
            if desc.compute_shader.is_empty() {
                return Err(RhiError::validation(
                    "compute pipeline requires compute SPIR-V",
                ));
            }
            let module = create_shader_module(device, desc.compute_shader, desc.label)?;
            modules.push(module);
            let entry = cstring(desc.compute_entry)?;
            let stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::COMPUTE)
                .module(module)
                .name(&entry);
            let info = vk::ComputePipelineCreateInfo::default()
                .stage(stage)
                .layout(layout);
            // SAFETY: all descriptions outlive the call; cache is owned by the context.
            let pipelines = unsafe { device.create_compute_pipelines(cache, &[info], None) }
                .map_err(|(_, error)| convert::map_vk(error))?;
            Ok(pipelines[0])
        }
    }
}

/// Vulkan-устройство.
pub struct VulkanDevice {
    context: Arc<DeviceContext>,
}

impl VulkanDevice {
    /// Создаёт устройство через runtime-загруженный Vulkan loader.
    ///
    /// # Errors
    ///
    /// [`RhiError::Unsupported`] — нет loader-а или GPU с graphics-очередью;
    /// [`RhiError::Validation`] — драйвер отверг instance/device;
    /// [`RhiError::OutOfMemory`] — не хватает памяти под внутренние объекты.
    pub fn new() -> Result<Self, RhiError> {
        // SAFETY: runtime loading is the only side effect; failure is handled.
        let entry = unsafe { ash::Entry::load() }.map_err(|_| RhiError::Unsupported {
            feature: "vulkan loader not available",
        })?;
        // SAFETY: entry is valid; the query has no other preconditions.
        let instance_version = unsafe { entry.try_enumerate_instance_version() }
            .map_err(convert::map_vk)?
            .unwrap_or(vk::API_VERSION_1_0);
        let surface_extensions = SurfaceExtensions::detect(&entry)?;
        let (instance, debug_enabled) = create_instance(
            &entry,
            instance_version,
            cfg!(debug_assertions),
            &surface_extensions,
        )?;
        match create_context(
            entry,
            instance,
            instance_version,
            debug_enabled,
            surface_extensions,
        ) {
            Ok(context) => Ok(Self {
                context: Arc::new(context),
            }),
            Err(error) => Err(error),
        }
    }

    /// Имя физического устройства.
    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.context.device_name
    }

    /// Версия Vulkan API, с которой создан instance.
    #[must_use]
    pub fn api_version(&self) -> u32 {
        self.context.api_version
    }

    /// Включены ли validation layers.
    #[must_use]
    pub fn validation_enabled(&self) -> bool {
        self.context.validation.is_some()
    }

    /// Число validation-ошибок с момента старта процесса.
    #[must_use]
    pub fn validation_errors(&self) -> u64 {
        debug::error_count()
    }

    /// Число живых буферов и изображений.
    #[must_use]
    pub fn live_resources(&self) -> usize {
        self.context.store.lock().live_resources()
    }

    /// Число живых pipeline.
    #[must_use]
    pub fn pipeline_count(&self) -> usize {
        self.context.store.lock().pipeline_count()
    }

    /// Занято памяти ресурсами, байт.
    #[must_use]
    pub fn allocated_bytes(&self) -> u64 {
        self.context.store.lock().memory.allocated()
    }

    /// Снимок бюджета VRAM.
    #[must_use]
    pub fn budget_snapshot(&self) -> BudgetSnapshot {
        self.context.store.lock().memory.budget()
    }
}

impl Drop for VulkanDevice {
    fn drop(&mut self) {
        // SAFETY: waiting for idle is valid while the device is alive.
        unsafe {
            let _ = self.context.device.device_wait_idle();
        }
        self.context.destroy_resources();
    }
}

impl RhiDevice for VulkanDevice {
    fn backend(&self) -> BackendKind {
        BackendKind::Vulkan
    }

    fn capabilities(&self) -> &Capabilities {
        &self.context.caps
    }

    fn device_signature(&self) -> DeviceSignature {
        DeviceSignature::new(
            BackendKind::Vulkan,
            self.context.device_name.clone(),
            self.context.api_version,
        )
    }

    fn pipeline_cache(&self) -> Result<Vec<u8>, RhiError> {
        let cache = self
            .context
            .pipeline_cache
            .lock()
            .ok_or_else(|| RhiError::validation("pipeline cache is unavailable"))?;
        // SAFETY: cache is owned by the context and alive while locked.
        unsafe { self.context.device.get_pipeline_cache_data(cache) }.map_err(convert::map_vk)
    }

    fn load_pipeline_cache(&mut self, data: &[u8]) -> Result<(), RhiError> {
        if data.is_empty() {
            return Ok(());
        }
        let info = vk::PipelineCacheCreateInfo::default().initial_data(data);
        // SAFETY: `data` outlives the call; the driver copies the blob.
        let cache = match unsafe { self.context.device.create_pipeline_cache(&info, None) } {
            Ok(cache) => cache,
            Err(error) => return Err(convert::map_vk(error)),
        };
        let previous = self.context.pipeline_cache.lock().replace(cache);
        if let Some(previous) = previous {
            // SAFETY: the old cache is no longer used by any pipeline creation.
            unsafe { self.context.device.destroy_pipeline_cache(previous, None) };
        }
        Ok(())
    }

    fn create_buffer(&mut self, desc: &GpuBufferDesc) -> Result<BufferId, RhiError> {
        if desc.size == 0 {
            return Err(RhiError::validation("buffer size must be non-zero"));
        }
        if !desc.usage.any() {
            return Err(RhiError::validation("buffer usage must not be empty"));
        }
        let device = &self.context.device;
        let usage = convert::buffer_usage(desc.usage);
        let info = vk::BufferCreateInfo::default()
            .size(desc.size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        // SAFETY: descriptor is validated above.
        let buffer = unsafe { device.create_buffer(&info, None) }.map_err(convert::map_vk)?;
        // SAFETY: buffer was just created.
        let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let host_visible = desc.usage.transfer_src
            && desc.usage.transfer_dst
            && !(desc.usage.vertex
                || desc.usage.index
                || desc.usage.uniform
                || desc.usage.storage
                || desc.usage.indirect);
        let (required, preferred) = if host_visible {
            (
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )
        } else {
            (
                vk::MemoryPropertyFlags::empty(),
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )
        };
        let mut store = self.context.store.lock();
        let allocation =
            match store
                .memory
                .allocate(device, requirements, required, preferred, desc.size)
            {
                Ok(allocation) => allocation,
                Err(error) => {
                    // SAFETY: buffer is not bound and is destroyed on the error path.
                    unsafe { device.destroy_buffer(buffer, None) };
                    return Err(error);
                }
            };
        // SAFETY: allocation satisfies the buffer requirements.
        if let Err(error) = unsafe { device.bind_buffer_memory(buffer, allocation.memory, 0) } {
            store.memory.free(device, allocation);
            // SAFETY: buffer is not bound and is destroyed on the error path.
            unsafe { device.destroy_buffer(buffer, None) };
            return Err(convert::map_vk(error));
        }
        let id = store.next_id();
        store.buffers.insert(
            id,
            BufferEntry {
                buffer,
                allocation,
                size: desc.size,
                usage: desc.usage,
            },
        );
        Ok(BufferId(id))
    }

    fn destroy_buffer(&mut self, id: BufferId) -> bool {
        let Some(entry) = self.context.store.lock().buffers.remove(&id.0) else {
            return false;
        };
        // SAFETY: entry is owned by the store and removed above.
        unsafe { self.context.device.destroy_buffer(entry.buffer, None) };
        self.context
            .store
            .lock()
            .memory
            .free(&self.context.device, entry.allocation);
        true
    }

    fn create_image(&mut self, desc: &GpuImageDesc) -> Result<ImageId, RhiError> {
        if desc.width == 0 || desc.height == 0 {
            return Err(RhiError::validation("image dimensions must be non-zero"));
        }
        if desc.mip_levels == 0 {
            return Err(RhiError::validation("mip levels must be non-zero"));
        }
        if desc.width > self.context.caps.max_texture_dimension_2d
            || desc.height > self.context.caps.max_texture_dimension_2d
        {
            return Err(RhiError::validation("image exceeds max texture dimension"));
        }
        if !desc.usage.any() {
            return Err(RhiError::validation("image usage must not be empty"));
        }
        if desc.format.is_depth() {
            if !desc.usage.depth_stencil {
                return Err(RhiError::validation(
                    "depth format requires depth_stencil usage",
                ));
            }
            if desc.usage.render_target {
                return Err(RhiError::validation(
                    "depth format cannot be a color attachment",
                ));
            }
        } else if desc.usage.depth_stencil {
            return Err(RhiError::validation(
                "depth_stencil usage requires a depth format",
            ));
        }
        let device = &self.context.device;
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(convert::format(desc.format))
            .extent(vk::Extent3D {
                width: desc.width,
                height: desc.height,
                depth: 1,
            })
            .mip_levels(desc.mip_levels)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(convert::image_usage(desc.usage))
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        // SAFETY: descriptor is validated above.
        let image = unsafe { device.create_image(&info, None) }.map_err(convert::map_vk)?;
        // SAFETY: image was just created.
        let requirements = unsafe { device.get_image_memory_requirements(image) };
        let requested = u64::from(desc.width)
            .saturating_mul(u64::from(desc.height))
            .saturating_mul(u64::from(desc.format.bytes_per_pixel()))
            .saturating_mul(u64::from(desc.mip_levels));
        let mut store = self.context.store.lock();
        let allocation = match store.memory.allocate(
            device,
            requirements,
            vk::MemoryPropertyFlags::empty(),
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            requested,
        ) {
            Ok(allocation) => allocation,
            Err(error) => {
                // SAFETY: image is not bound and is destroyed on the error path.
                unsafe { device.destroy_image(image, None) };
                return Err(error);
            }
        };
        // SAFETY: allocation satisfies the image requirements.
        if let Err(error) = unsafe { device.bind_image_memory(image, allocation.memory, 0) } {
            store.memory.free(device, allocation);
            // SAFETY: image is not bound and is destroyed on the error path.
            unsafe { device.destroy_image(image, None) };
            return Err(convert::map_vk(error));
        }
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(convert::aspect(desc.format))
            .base_mip_level(0)
            .level_count(desc.mip_levels)
            .base_array_layer(0)
            .layer_count(1);
        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(convert::format(desc.format))
            .subresource_range(range);
        // SAFETY: image is valid and the view description is complete.
        let view = match unsafe { device.create_image_view(&view_info, None) } {
            Ok(view) => view,
            Err(error) => {
                store.memory.free(device, allocation);
                // SAFETY: image is unbound and destroyed on the error path.
                unsafe { device.destroy_image(image, None) };
                return Err(convert::map_vk(error));
            }
        };
        let id = store.next_id();
        store.images.insert(
            id,
            ImageEntry {
                image,
                view,
                width: desc.width,
                height: desc.height,
                format: desc.format,
                allocation: Some(allocation),
                owned: true,
            },
        );
        Ok(ImageId(id))
    }

    fn destroy_image(&mut self, id: ImageId) -> bool {
        // Образы swapchain не освобождаются через устройство (ADR-0045).
        let removed = {
            let mut store = self.context.store.lock();
            if store.images.get(&id.0).is_some_and(|entry| entry.owned) {
                store.images.remove(&id.0)
            } else {
                None
            }
        };
        let Some(entry) = removed else {
            return false;
        };
        // SAFETY: entry is owned by the store and removed above.
        unsafe {
            self.context.device.destroy_image_view(entry.view, None);
            self.context.device.destroy_image(entry.image, None);
        }
        if let Some(allocation) = entry.allocation {
            self.context
                .store
                .lock()
                .memory
                .free(&self.context.device, allocation);
        }
        true
    }

    fn create_query_pool(&mut self, desc: &QueryPoolDesc) -> Result<QueryPoolId, RhiError> {
        if desc.count == 0 || desc.count > 4_096 {
            return Err(RhiError::validation("query pool count must be 1..=4096"));
        }
        let info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::TIMESTAMP)
            .query_count(desc.count);
        // SAFETY: create info is valid for the lifetime of the call.
        let pool = unsafe { self.context.device.create_query_pool(&info, None) }
            .map_err(convert::map_vk)?;
        let id = self.context.store.lock().next_id();
        self.context.store.lock().query_pools.insert(
            id,
            QueryPoolEntry {
                pool,
                count: desc.count,
            },
        );
        Ok(QueryPoolId(id))
    }

    fn destroy_query_pool(&mut self, id: QueryPoolId) -> bool {
        let Some(entry) = self.context.store.lock().query_pools.remove(&id.0) else {
            return false;
        };
        // SAFETY: entry is owned by the store and removed above.
        unsafe { self.context.device.destroy_query_pool(entry.pool, None) };
        true
    }

    fn read_query_pool(&mut self, id: QueryPoolId) -> Result<Vec<u64>, RhiError> {
        let (pool, count) = self.context.store.lock().query_pool(id)?;
        // SAFETY: waiting makes timestamp results available for reading.
        unsafe { self.context.device.device_wait_idle() }.map_err(convert::map_vk)?;
        let mut data = vec![0_u64; count as usize];
        // SAFETY: pool/count come from the store entry; data matches query count.
        unsafe {
            self.context.device.get_query_pool_results(
                pool,
                0,
                &mut data,
                vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
            )
        }
        .map_err(convert::map_vk)?;
        Ok(data)
    }

    fn create_pipeline(&mut self, desc: &PipelineDesc) -> Result<PipelineId, RhiError> {
        let device = &self.context.device;
        let cache = self
            .context
            .pipeline_cache
            .lock()
            .unwrap_or(vk::PipelineCache::null());
        let mut modules = Vec::new();
        let mut layout_slot: Option<vk::PipelineLayout> = None;
        let result = build_pipeline(
            device,
            &self.context.store,
            cache,
            desc,
            &mut modules,
            &mut layout_slot,
        );
        match result {
            Ok(pipeline) => {
                let Some(layout) = layout_slot.take() else {
                    // SAFETY: pipeline was created; destroy it on the error path.
                    unsafe { device.destroy_pipeline(pipeline, None) };
                    for module in modules {
                        // SAFETY: modules are owned by this call.
                        unsafe { device.destroy_shader_module(module, None) };
                    }
                    return Err(RhiError::validation("internal: pipeline layout missing"));
                };
                let id = self.context.store.lock().next_id();
                self.context.store.lock().pipelines.insert(
                    id,
                    PipelineEntry {
                        pipeline,
                        layout,
                        push_constant_size: desc.push_constant_size,
                        modules,
                        kind: desc.kind,
                    },
                );
                Ok(PipelineId(id))
            }
            Err(error) => {
                if let Some(layout) = layout_slot.take() {
                    // SAFETY: layout was created by this call.
                    unsafe { device.destroy_pipeline_layout(layout, None) };
                }
                for module in modules {
                    // SAFETY: modules are owned by this call.
                    unsafe { device.destroy_shader_module(module, None) };
                }
                Err(error)
            }
        }
    }

    fn destroy_pipeline(&mut self, id: PipelineId) -> bool {
        let Some(entry) = self.context.store.lock().pipelines.remove(&id.0) else {
            return false;
        };
        // SAFETY: entry is owned by the store and removed above.
        unsafe {
            self.context.device.destroy_pipeline(entry.pipeline, None);
            self.context
                .device
                .destroy_pipeline_layout(entry.layout, None);
            for module in entry.modules {
                self.context.device.destroy_shader_module(module, None);
            }
        }
        true
    }

    fn create_sampler(&mut self, desc: &SamplerDesc) -> Result<SamplerId, RhiError> {
        if !desc.is_well_formed() {
            return Err(RhiError::validation("sampler description is out of range"));
        }
        if desc.max_anisotropy > 1 && !self.context.caps.supports_sampler_anisotropy {
            return Err(RhiError::validation(
                "sampler anisotropy is not supported by the device",
            ));
        }
        let info = vk::SamplerCreateInfo::default()
            .mag_filter(convert::filter(desc.mag_filter))
            .min_filter(convert::filter(desc.min_filter))
            .mipmap_mode(convert::mipmap(desc.mipmap_mode))
            .address_mode_u(convert::address(desc.address_u))
            .address_mode_v(convert::address(desc.address_v))
            .address_mode_w(convert::address(desc.address_w))
            .anisotropy_enable(desc.max_anisotropy > 1)
            .max_anisotropy(f32::from(desc.max_anisotropy))
            .min_lod(desc.lod_min)
            .max_lod(desc.lod_max)
            .compare_enable(desc.compare)
            .compare_op(if desc.compare {
                vk::CompareOp::LESS_OR_EQUAL
            } else {
                vk::CompareOp::ALWAYS
            })
            .border_color(vk::BorderColor::FLOAT_OPAQUE_BLACK);
        // SAFETY: descriptor is validated and the device outlives the sampler.
        let sampler =
            unsafe { self.context.device.create_sampler(&info, None) }.map_err(convert::map_vk)?;
        let mut store = self.context.store.lock();
        let id = store.next_id();
        store.samplers.insert(id, SamplerEntry { sampler });
        Ok(SamplerId(id))
    }

    fn destroy_sampler(&mut self, id: SamplerId) -> bool {
        let mut store = self.context.store.lock();
        let Some(entry) = store.samplers.remove(&id.0) else {
            return false;
        };
        // SAFETY: sampler belongs to this device and was removed above.
        unsafe { self.context.device.destroy_sampler(entry.sampler, None) };
        true
    }

    fn create_descriptor_set(
        &mut self,
        desc: &DescriptorSetDesc,
    ) -> Result<DescriptorSetId, RhiError> {
        if desc.bindings.is_empty() {
            return Err(RhiError::validation("descriptor set has no bindings"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for binding in &desc.bindings {
            if binding.count == 0 {
                return Err(RhiError::validation(
                    "descriptor binding count must be non-zero",
                ));
            }
            if !seen.insert(binding.binding) {
                return Err(RhiError::validation("duplicate descriptor binding"));
            }
        }
        let device = &self.context.device;
        let mut store = self.context.store.lock();
        let layout = store.ensure_descriptor_layout(device, &desc.bindings)?;
        let pool = store.ensure_descriptor_pool(device)?;
        let layouts = [layout];
        let info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(pool)
            .set_layouts(&layouts);
        // SAFETY: layout belongs to the device and the pool has capacity.
        let set = unsafe { device.allocate_descriptor_sets(&info) }.map_err(convert::map_vk)?[0];
        let id = store.next_id();
        store.descriptor_sets.insert(
            id,
            DescriptorSetEntry {
                set,
                bindings: desc.bindings.clone(),
            },
        );
        Ok(DescriptorSetId(id))
    }

    fn update_descriptor_set(
        &mut self,
        id: DescriptorSetId,
        writes: &[DescriptorWrite],
    ) -> Result<(), RhiError> {
        let device = &self.context.device;
        let store = self.context.store.lock();
        let entry = store.descriptor_set(id)?;
        let set = entry.set;
        let bindings = &entry.bindings;

        let mut buffer_infos: Vec<vk::DescriptorBufferInfo> = Vec::with_capacity(writes.len());
        let mut image_infos: Vec<vk::DescriptorImageInfo> = Vec::with_capacity(writes.len());
        let mut planned: Vec<(u32, u32, vk::DescriptorType, Option<usize>, bool)> =
            Vec::with_capacity(writes.len());

        for write in writes {
            let binding = bindings
                .iter()
                .find(|binding| binding.binding == write.binding)
                .ok_or_else(|| RhiError::validation("unknown descriptor binding"))?;
            if write.array_index >= binding.count {
                return Err(RhiError::validation(
                    "descriptor array index is out of bounds",
                ));
            }
            match write.resource {
                DescriptorResource::Buffer(buffer) => {
                    if !matches!(
                        binding.kind,
                        BindingKind::UniformBuffer | BindingKind::StorageBuffer
                    ) {
                        return Err(RhiError::validation("descriptor resource type mismatch"));
                    }
                    let resource = store
                        .buffers
                        .get(&buffer.0)
                        .ok_or_else(|| RhiError::validation("unknown descriptor buffer"))?;
                    buffer_infos.push(
                        vk::DescriptorBufferInfo::default()
                            .buffer(resource.buffer)
                            .offset(0)
                            .range(resource.size),
                    );
                    planned.push((
                        write.binding,
                        write.array_index,
                        descriptor_type(binding.kind),
                        Some(buffer_infos.len() - 1),
                        true,
                    ));
                }
                DescriptorResource::Image(image) => {
                    let layout = match binding.kind {
                        BindingKind::SampledImage => vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                        BindingKind::StorageImage => vk::ImageLayout::GENERAL,
                        _ => return Err(RhiError::validation("descriptor resource type mismatch")),
                    };
                    let resource = store
                        .images
                        .get(&image.0)
                        .ok_or_else(|| RhiError::validation("unknown descriptor image"))?;
                    image_infos.push(
                        vk::DescriptorImageInfo::default()
                            .image_view(resource.view)
                            .image_layout(layout),
                    );
                    planned.push((
                        write.binding,
                        write.array_index,
                        descriptor_type(binding.kind),
                        Some(image_infos.len() - 1),
                        false,
                    ));
                }
                DescriptorResource::Sampler(sampler) => {
                    if binding.kind != BindingKind::Sampler {
                        return Err(RhiError::validation("descriptor resource type mismatch"));
                    }
                    let resource = store
                        .samplers
                        .get(&sampler.0)
                        .ok_or_else(|| RhiError::validation("unknown descriptor sampler"))?;
                    image_infos.push(vk::DescriptorImageInfo::default().sampler(resource.sampler));
                    planned.push((
                        write.binding,
                        write.array_index,
                        descriptor_type(binding.kind),
                        Some(image_infos.len() - 1),
                        false,
                    ));
                }
            }
        }

        let mut vk_writes: Vec<vk::WriteDescriptorSet> = Vec::with_capacity(planned.len());
        for (binding, array_index, ty, which, is_buffer) in planned {
            let mut write = vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(binding)
                .dst_array_element(array_index)
                .descriptor_type(ty)
                .descriptor_count(1);
            if let Some(index) = which {
                write = if is_buffer {
                    write.buffer_info(std::slice::from_ref(&buffer_infos[index]))
                } else {
                    write.image_info(std::slice::from_ref(&image_infos[index]))
                };
            }
            vk_writes.push(write);
        }
        // SAFETY: infos outlive the update call and descriptors belong to the set.
        unsafe { device.update_descriptor_sets(&vk_writes, &[]) };
        Ok(())
    }

    fn destroy_descriptor_set(&mut self, id: DescriptorSetId) -> bool {
        let mut store = self.context.store.lock();
        let Some(entry) = store.descriptor_sets.remove(&id.0) else {
            return false;
        };
        if let Some(pool) = store.descriptor_pool {
            // SAFETY: set was allocated from this pool and removed above.
            unsafe {
                let _ = self
                    .context
                    .device
                    .free_descriptor_sets(pool, std::slice::from_ref(&entry.set));
            }
        }
        true
    }

    fn write_buffer(&mut self, id: BufferId, offset: u64, data: &[u8]) -> Result<(), RhiError> {
        let (memory, size, host_visible) = {
            let store = self.context.store.lock();
            let entry = store
                .buffers
                .get(&id.0)
                .ok_or_else(|| RhiError::validation("unknown buffer"))?;
            (
                entry.allocation.memory,
                entry.size,
                entry.allocation.host_visible && entry.usage.is_staging(),
            )
        };
        if !host_visible {
            return Err(RhiError::validation("buffer is not host-visible"));
        }
        let end = offset
            .checked_add(data.len() as u64)
            .ok_or_else(|| RhiError::validation("buffer write overflows"))?;
        if end > size {
            return Err(RhiError::validation("buffer write is out of bounds"));
        }
        let device = &self.context.device;
        // SAFETY: mapping a host-visible allocation and copying within bounds.
        unsafe {
            let ptr = device
                .map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
                .map_err(convert::map_vk)?;
            std::ptr::copy_nonoverlapping(
                data.as_ptr(),
                ptr.cast::<u8>().add(offset as usize),
                data.len(),
            );
            device.unmap_memory(memory);
        }
        Ok(())
    }

    fn read_buffer(&mut self, id: BufferId) -> Result<Vec<u8>, RhiError> {
        let (memory, size, host_visible) = {
            let store = self.context.store.lock();
            let entry = store
                .buffers
                .get(&id.0)
                .ok_or_else(|| RhiError::validation("unknown buffer"))?;
            (
                entry.allocation.memory,
                entry.size,
                entry.allocation.host_visible && entry.usage.is_staging(),
            )
        };
        if !host_visible {
            return Err(RhiError::validation("buffer is not host-visible"));
        }
        let size = usize::try_from(size)
            .map_err(|_| RhiError::validation("buffer size exceeds address space"))?;
        let device = &self.context.device;
        // SAFETY: mapping a host-visible allocation and copying within bounds.
        unsafe {
            let ptr = device
                .map_memory(memory, 0, vk::WHOLE_SIZE, vk::MemoryMapFlags::empty())
                .map_err(convert::map_vk)?;
            let bytes = std::slice::from_raw_parts(ptr.cast::<u8>(), size).to_vec();
            device.unmap_memory(memory);
            Ok(bytes)
        }
    }

    fn create_command_buffer(&mut self) -> Result<Box<dyn CommandBuffer>, RhiError> {
        Ok(Box::new(VulkanCommandBuffer::new(self.context.clone())?))
    }

    fn create_surface(&mut self, target: &SurfaceTarget) -> Result<SurfaceId, RhiError> {
        if !self.context.surface_supported {
            return Err(RhiError::Unsupported {
                feature: "VK_KHR_surface",
            });
        }
        let surface = create_surface_handle(&self.context, target)?;
        let id = self.context.store.lock().next_id();
        self.context.store.lock().surfaces.insert(id, surface);
        Ok(SurfaceId(id))
    }

    fn destroy_surface(&mut self, id: SurfaceId) -> bool {
        let surface = self.context.store.lock().surfaces.remove(&id.0);
        match surface {
            Some(surface) => {
                // SAFETY: surface is owned by the store and removed above.
                unsafe { self.context.surface_loader.destroy_surface(surface, None) };
                true
            }
            None => false,
        }
    }

    fn create_swapchain(&mut self, desc: &SwapchainDesc) -> Result<Box<dyn Swapchain>, RhiError> {
        let Some(surface_id) = desc.surface else {
            return Ok(Box::new(VulkanSwapchain::new(self.context.clone(), desc)?));
        };
        let surface = *self
            .context
            .store
            .lock()
            .surfaces
            .get(&surface_id.0)
            .ok_or(RhiError::InvalidHandle {
                kind: "surface",
                id: surface_id.0,
            })?;
        let swapchain = VulkanPresentSwapchain::new(self.context.clone(), surface, desc)?;
        Ok(Box::new(swapchain))
    }

    fn wait_idle(&mut self) -> Result<(), RhiError> {
        // SAFETY: device is alive for the duration of the call.
        unsafe { self.context.device.device_wait_idle() }.map_err(convert::map_vk)
    }
}

impl VramAllocator for VulkanDevice {
    fn allocate(&mut self, bytes: u64, tag: MemoryTag) -> Result<VramLease, RhiError> {
        self.context.store.lock().memory.lease(bytes, tag)
    }

    fn free(&mut self, lease: VramLease) -> bool {
        self.context.store.lock().memory.free_lease(lease)
    }

    fn budget(&self) -> BudgetSnapshot {
        self.context.store.lock().memory.budget()
    }
}

struct BuiltDevice {
    physical: vk::PhysicalDevice,
    device: ash::Device,
    queue: vk::Queue,
    queue_family: u32,
    command_pool: vk::CommandPool,
    caps: Capabilities,
    memory: MemoryState,
    device_name: String,
    swapchain_supported: bool,
}

/// Доступные instance-расширения поверхности.
struct SurfaceExtensions {
    surface: bool,
    headless: bool,
    #[cfg(target_os = "windows")]
    win32: bool,
    #[cfg(all(unix, not(target_os = "macos")))]
    xlib: bool,
    #[cfg(all(unix, not(target_os = "macos")))]
    wayland: bool,
    #[cfg(all(unix, not(target_os = "macos")))]
    xcb: bool,
}

impl SurfaceExtensions {
    fn detect(entry: &ash::Entry) -> Result<Self, RhiError> {
        // SAFETY: entry is valid; the query has no other preconditions.
        let available = unsafe { entry.enumerate_instance_extension_properties(None) }
            .map_err(convert::map_vk)?;
        let has = |name: &CStr| {
            available
                .iter()
                .any(|extension| extension.extension_name_as_c_str() == Ok(name))
        };
        Ok(Self {
            surface: has(ash::khr::surface::NAME),
            headless: has(ash::ext::headless_surface::NAME),
            #[cfg(target_os = "windows")]
            win32: has(ash::khr::win32_surface::NAME),
            #[cfg(all(unix, not(target_os = "macos")))]
            xlib: has(ash::khr::xlib_surface::NAME),
            #[cfg(all(unix, not(target_os = "macos")))]
            wayland: has(ash::khr::wayland_surface::NAME),
            #[cfg(all(unix, not(target_os = "macos")))]
            xcb: has(ash::khr::xcb_surface::NAME),
        })
    }

    /// Имена расширений, которые нужно включить в instance.
    fn enabled(&self) -> Vec<&'static CStr> {
        let mut names = Vec::new();
        if self.surface {
            names.push(ash::khr::surface::NAME);
        }
        if self.headless {
            names.push(ash::ext::headless_surface::NAME);
        }
        #[cfg(target_os = "windows")]
        if self.win32 {
            names.push(ash::khr::win32_surface::NAME);
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            if self.xlib {
                names.push(ash::khr::xlib_surface::NAME);
            }
            if self.wayland {
                names.push(ash::khr::wayland_surface::NAME);
            }
            if self.xcb {
                names.push(ash::khr::xcb_surface::NAME);
            }
        }
        names
    }
}

fn create_instance(
    entry: &ash::Entry,
    instance_version: u32,
    debug_requested: bool,
    surface: &SurfaceExtensions,
) -> Result<(ash::Instance, bool), RhiError> {
    let app_info = vk::ApplicationInfo::default()
        .application_name(c"FluxEngine")
        .application_version(vk::make_api_version(0, 0, 1, 0))
        .engine_name(c"flux-graphics")
        .engine_version(vk::make_api_version(0, 0, 1, 0))
        .api_version(if instance_version >= vk::API_VERSION_1_2 {
            vk::API_VERSION_1_2
        } else {
            vk::API_VERSION_1_0
        });
    let mut extensions: Vec<*const std::ffi::c_char> =
        surface.enabled().iter().map(|name| name.as_ptr()).collect();
    if debug_requested {
        extensions.push(ash::ext::debug_utils::NAME.as_ptr());
        let layers = [c"VK_LAYER_KHRONOS_validation".as_ptr()];
        let info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_layer_names(&layers)
            .enabled_extension_names(&extensions);
        // SAFETY: create info is valid; failure falls back to a plain instance.
        if let Ok(instance) = unsafe { entry.create_instance(&info, None) } {
            return Ok((instance, true));
        }
        extensions.pop();
    }
    let info = vk::InstanceCreateInfo::default()
        .application_info(&app_info)
        .enabled_extension_names(&extensions);
    // SAFETY: create info is valid.
    let instance = unsafe { entry.create_instance(&info, None) }.map_err(convert::map_vk)?;
    Ok((instance, false))
}

fn create_context(
    entry: ash::Entry,
    instance: ash::Instance,
    api_version: u32,
    debug_enabled: bool,
    surface: SurfaceExtensions,
) -> Result<DeviceContext, RhiError> {
    let built = match build_device(&instance, api_version) {
        Ok(built) => built,
        Err(error) => {
            // SAFETY: instance has no child objects on this path.
            unsafe { instance.destroy_instance(None) };
            return Err(error);
        }
    };
    let cache_info = vk::PipelineCacheCreateInfo::default();
    // SAFETY: device is valid; an empty cache has no external dependencies.
    let pipeline_cache = match unsafe { built.device.create_pipeline_cache(&cache_info, None) } {
        Ok(cache) => cache,
        Err(error) => {
            // SAFETY: command pool and device are owned by `built`; instance is ours.
            unsafe {
                built.device.destroy_command_pool(built.command_pool, None);
                built.device.destroy_device(None);
                instance.destroy_instance(None);
            }
            return Err(convert::map_vk(error));
        }
    };
    let validation = if debug_enabled {
        debug::DebugMessenger::new(&entry, &instance)
    } else {
        None
    };
    let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
    let headless_surface = surface
        .headless
        .then(|| ash::ext::headless_surface::Instance::new(&entry, &instance));
    #[cfg(target_os = "windows")]
    let win32_surface = surface
        .win32
        .then(|| ash::khr::win32_surface::Instance::new(&entry, &instance));
    #[cfg(all(unix, not(target_os = "macos")))]
    let xlib_surface = surface
        .xlib
        .then(|| ash::khr::xlib_surface::Instance::new(&entry, &instance));
    #[cfg(all(unix, not(target_os = "macos")))]
    let wayland_surface = surface
        .wayland
        .then(|| ash::khr::wayland_surface::Instance::new(&entry, &instance));
    #[cfg(all(unix, not(target_os = "macos")))]
    let xcb_surface = surface
        .xcb
        .then(|| ash::khr::xcb_surface::Instance::new(&entry, &instance));
    let swapchain_loader = built
        .swapchain_supported
        .then(|| ash::khr::swapchain::Device::new(&instance, &built.device));
    Ok(DeviceContext {
        entry,
        instance,
        physical: built.physical,
        device: built.device,
        queue: built.queue,
        queue_family: built.queue_family,
        command_pool: built.command_pool,
        caps: built.caps,
        validation,
        store: Mutex::new(ResourceStore::new(built.memory)),
        pipeline_cache: Mutex::new(Some(pipeline_cache)),
        surface_loader,
        headless_surface,
        #[cfg(target_os = "windows")]
        win32_surface,
        #[cfg(all(unix, not(target_os = "macos")))]
        xlib_surface,
        #[cfg(all(unix, not(target_os = "macos")))]
        wayland_surface,
        #[cfg(all(unix, not(target_os = "macos")))]
        xcb_surface,
        swapchain_loader,
        surface_supported: surface.surface,
        api_version,
        device_name: built.device_name,
    })
}

/// Создаёт `VkSurfaceKHR` из headless- или оконного таргета.
fn create_surface_handle(
    context: &DeviceContext,
    target: &SurfaceTarget,
) -> Result<vk::SurfaceKHR, RhiError> {
    match target {
        SurfaceTarget::Headless => {
            let loader = context
                .headless_surface
                .as_ref()
                .ok_or(RhiError::Unsupported {
                    feature: "VK_EXT_headless_surface",
                })?;
            let info = vk::HeadlessSurfaceCreateInfoEXT::default();
            // SAFETY: instance owns the loader; create info is default/valid.
            unsafe { loader.create_headless_surface(&info, None) }.map_err(convert::map_vk)
        }
        SurfaceTarget::Window(window) => create_window_surface(context, window),
    }
}

#[cfg(target_os = "windows")]
fn create_window_surface(
    context: &DeviceContext,
    window: &WindowSurface,
) -> Result<vk::SurfaceKHR, RhiError> {
    use raw_window_handle::RawWindowHandle;
    if let RawWindowHandle::Win32(handle) = window.window {
        let loader = context
            .win32_surface
            .as_ref()
            .ok_or(RhiError::Unsupported {
                feature: "VK_KHR_win32_surface",
            })?;
        let info = vk::Win32SurfaceCreateInfoKHR::default()
            .hinstance(handle.hinstance.map(|value| value.get()).unwrap_or(0))
            .hwnd(handle.hwnd.get());
        // SAFETY: handles come from the window owner and outlive the surface.
        return unsafe { loader.create_win32_surface(&info, None) }.map_err(convert::map_vk);
    }
    Err(RhiError::Unsupported {
        feature: "window surface (unsupported handle)",
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn create_window_surface(
    context: &DeviceContext,
    window: &WindowSurface,
) -> Result<vk::SurfaceKHR, RhiError> {
    use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
    match window.window {
        RawWindowHandle::Xlib(handle) => {
            let display: *mut std::ffi::c_void = match window.display {
                RawDisplayHandle::Xlib(display) => display
                    .display
                    .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast()),
                _ => std::ptr::null_mut(),
            };
            if display.is_null() {
                return Err(RhiError::Surface("xlib display handle is missing".into()));
            }
            let loader = context.xlib_surface.as_ref().ok_or(RhiError::Unsupported {
                feature: "VK_KHR_xlib_surface",
            })?;
            let info = vk::XlibSurfaceCreateInfoKHR::default()
                .dpy(display)
                .window(handle.window);
            // SAFETY: handles come from the window owner and outlive the surface.
            unsafe { loader.create_xlib_surface(&info, None) }.map_err(convert::map_vk)
        }
        RawWindowHandle::Xcb(handle) => {
            let connection: *mut std::ffi::c_void = match window.display {
                RawDisplayHandle::Xcb(display) => display
                    .connection
                    .map_or(std::ptr::null_mut(), |value| value.as_ptr().cast()),
                _ => std::ptr::null_mut(),
            };
            if connection.is_null() {
                return Err(RhiError::Surface("xcb connection is missing".into()));
            }
            let loader = context.xcb_surface.as_ref().ok_or(RhiError::Unsupported {
                feature: "VK_KHR_xcb_surface",
            })?;
            let info = vk::XcbSurfaceCreateInfoKHR::default()
                .connection(connection)
                .window(handle.window.get());
            // SAFETY: handles come from the window owner and outlive the surface.
            unsafe { loader.create_xcb_surface(&info, None) }.map_err(convert::map_vk)
        }
        RawWindowHandle::Wayland(handle) => {
            let display: *mut std::ffi::c_void = match window.display {
                RawDisplayHandle::Wayland(display) => display.display.as_ptr().cast(),
                _ => return Err(RhiError::Surface("wayland display is missing".into())),
            };
            let loader = context
                .wayland_surface
                .as_ref()
                .ok_or(RhiError::Unsupported {
                    feature: "VK_KHR_wayland_surface",
                })?;
            let info = vk::WaylandSurfaceCreateInfoKHR::default()
                .display(display)
                .surface(handle.surface.as_ptr().cast());
            // SAFETY: handles come from the window owner and outlive the surface.
            unsafe { loader.create_wayland_surface(&info, None) }.map_err(convert::map_vk)
        }
        _ => Err(RhiError::Unsupported {
            feature: "window surface (unsupported handle)",
        }),
    }
}

#[cfg(not(any(target_os = "windows", all(unix, not(target_os = "macos")))))]
fn create_window_surface(
    _context: &DeviceContext,
    _window: &WindowSurface,
) -> Result<vk::SurfaceKHR, RhiError> {
    Err(RhiError::Unsupported {
        feature: "window surface on this platform",
    })
}

fn build_device(instance: &ash::Instance, api_version: u32) -> Result<BuiltDevice, RhiError> {
    // SAFETY: instance is valid.
    let physical_devices =
        unsafe { instance.enumerate_physical_devices() }.map_err(convert::map_vk)?;
    let mut selected = None;
    for physical in physical_devices {
        // SAFETY: physical device belongs to the instance.
        let families = unsafe { instance.get_physical_device_queue_family_properties(physical) };
        if let Some(index) = families
            .iter()
            .position(|family| family.queue_flags.contains(vk::QueueFlags::GRAPHICS))
        {
            selected = Some((physical, index as u32));
            break;
        }
    }
    let Some((physical, queue_family)) = selected else {
        return Err(RhiError::Unsupported {
            feature: "vulkan graphics queue",
        });
    };
    // SAFETY: physical device belongs to the instance.
    let properties = unsafe { instance.get_physical_device_properties(physical) };
    // SAFETY: physical device belongs to the instance.
    let features = unsafe { instance.get_physical_device_features(physical) };
    // SAFETY: physical device belongs to the instance.
    let memory_properties = unsafe { instance.get_physical_device_memory_properties(physical) };
    let device_name = properties
        .device_name_as_c_str()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".to_owned());
    // SAFETY: physical device belongs to the instance.
    let device_extensions = unsafe { instance.enumerate_device_extension_properties(physical) }
        .map_err(convert::map_vk)?;
    let swapchain_supported = device_extensions
        .iter()
        .any(|extension| extension.extension_name_as_c_str() == Ok(ash::khr::swapchain::NAME));

    let mut timeline = vk::PhysicalDeviceTimelineSemaphoreFeatures::default();
    let mut indexing = vk::PhysicalDeviceDescriptorIndexingFeatures::default();
    let mut features2 = vk::PhysicalDeviceFeatures2::default()
        .push_next(&mut timeline)
        .push_next(&mut indexing);
    if api_version >= vk::API_VERSION_1_1 {
        // SAFETY: instance and physical device are valid; chained structs are writable.
        unsafe { instance.get_physical_device_features2(physical, &mut features2) };
    }
    let timeline_supported =
        api_version >= vk::API_VERSION_1_2 && timeline.timeline_semaphore == vk::TRUE;
    let descriptor_indexing = indexing.runtime_descriptor_array == vk::TRUE
        && indexing.descriptor_binding_partially_bound == vk::TRUE;

    let mut caps = build_capabilities(
        instance,
        physical,
        &properties,
        descriptor_indexing,
        features.sampler_anisotropy == vk::TRUE,
    )?;
    caps.supports_timeline_semaphores = timeline_supported;

    let priorities = [1.0f32];
    let queue_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(queue_family)
        .queue_priorities(&priorities);
    let queue_infos = [queue_info];
    let mut timeline_enable = vk::PhysicalDeviceTimelineSemaphoreFeatures::default()
        .timeline_semaphore(timeline_supported);
    let mut device_extension_names: Vec<*const std::ffi::c_char> = Vec::new();
    if swapchain_supported {
        device_extension_names.push(ash::khr::swapchain::NAME.as_ptr());
    }
    let mut create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(&device_extension_names);
    if timeline_supported {
        create_info = create_info.push_next(&mut timeline_enable);
    }
    // SAFETY: physical device and queue description are valid.
    let device =
        unsafe { instance.create_device(physical, &create_info, None) }.map_err(convert::map_vk)?;
    // SAFETY: queue family was validated to support graphics.
    let queue = unsafe { device.get_device_queue(queue_family, 0) };
    let pool_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(queue_family)
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
    // SAFETY: queue family index is valid.
    let command_pool = match unsafe { device.create_command_pool(&pool_info, None) } {
        Ok(pool) => pool,
        Err(error) => {
            // SAFETY: device has no other children on this path.
            unsafe { device.destroy_device(None) };
            return Err(convert::map_vk(error));
        }
    };
    Ok(BuiltDevice {
        physical,
        device,
        queue,
        queue_family,
        command_pool,
        caps,
        memory: MemoryState::new(memory_properties),
        device_name,
        swapchain_supported,
    })
}

fn build_capabilities(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    properties: &vk::PhysicalDeviceProperties,
    descriptor_indexing: bool,
    sampler_anisotropy: bool,
) -> Result<Capabilities, RhiError> {
    let candidates = [
        (
            TextureFormat::Rgba8Srgb,
            vk::FormatFeatureFlags::SAMPLED_IMAGE | vk::FormatFeatureFlags::COLOR_ATTACHMENT,
        ),
        (
            TextureFormat::Rgba8Unorm,
            vk::FormatFeatureFlags::SAMPLED_IMAGE | vk::FormatFeatureFlags::COLOR_ATTACHMENT,
        ),
        (
            TextureFormat::Bgra8Srgb,
            vk::FormatFeatureFlags::SAMPLED_IMAGE | vk::FormatFeatureFlags::COLOR_ATTACHMENT,
        ),
        (
            TextureFormat::Rgba16Float,
            vk::FormatFeatureFlags::SAMPLED_IMAGE | vk::FormatFeatureFlags::COLOR_ATTACHMENT,
        ),
        (
            TextureFormat::R8Unorm,
            vk::FormatFeatureFlags::SAMPLED_IMAGE,
        ),
    ];
    let mut color_formats = Vec::new();
    for (format, required) in candidates {
        // SAFETY: physical device belongs to the instance.
        let format_properties = unsafe {
            instance.get_physical_device_format_properties(physical, convert::format(format))
        };
        if format_properties.optimal_tiling_features.contains(required) {
            color_formats.push(format);
        }
    }
    if color_formats.is_empty() {
        return Err(RhiError::Unsupported {
            feature: "vulkan color attachment format",
        });
    }
    // SAFETY: physical device belongs to the instance.
    let depth_properties =
        unsafe { instance.get_physical_device_format_properties(physical, vk::Format::D32_SFLOAT) };
    let mut depth_formats = Vec::new();
    if depth_properties
        .optimal_tiling_features
        .contains(vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT)
    {
        depth_formats.push(TextureFormat::D32Float);
    }
    Ok(Capabilities {
        backend: BackendKind::Vulkan,
        max_texture_dimension_2d: properties.limits.max_image_dimension2_d,
        max_frames_in_flight: 2,
        max_bindless_descriptors: if descriptor_indexing {
            properties
                .limits
                .max_per_stage_descriptor_sampled_images
                .min(65_536)
        } else {
            0
        },
        supports_timeline_semaphores: false,
        supports_descriptor_indexing: descriptor_indexing,
        supports_sampler_anisotropy: sampler_anisotropy,
        present_modes: vec![
            PresentMode::Fifo,
            PresentMode::Mailbox,
            PresentMode::Immediate,
        ],
        color_formats,
        depth_formats,
    })
}

#[cfg(test)]
mod tests {
    use super::super::cmd::{BarrierBatch, ImageState};
    use super::super::types::{
        BufferUsage, GpuBufferDesc, GpuImageDesc, PipelineDesc, PipelineKind, QueryPoolDesc,
        RenderPassDesc, SwapchainDesc, TextureFormat, TextureUsage,
    };
    use super::super::RhiDevice;
    use super::{VramAllocator, VulkanDevice};
    use flux_core::memory::MemoryTag;

    fn runtime_device() -> Option<VulkanDevice> {
        match VulkanDevice::new() {
            Ok(device) => Some(device),
            Err(error) => {
                // На GPU-раннере отсутствие устройства — ошибка, а не skip.
                if std::env::var_os("FLUX_REQUIRE_VULKAN").is_some() {
                    panic!("FLUX_REQUIRE_VULKAN is set, but Vulkan is unavailable: {error}");
                }
                eprintln!("skipping Vulkan test: {error}");
                None
            }
        }
    }

    #[test]
    fn capabilities_describe_the_device() {
        let Some(device) = runtime_device() else {
            return;
        };
        let caps = device.capabilities();
        assert_eq!(caps.backend, super::BackendKind::Vulkan);
        assert!(caps.max_texture_dimension_2d >= 1024);
        assert!(!caps.color_formats.is_empty());
        assert!(device.api_version() >= super::vk::API_VERSION_1_0);
    }

    #[test]
    fn buffers_and_images_are_freed_without_leaks() {
        let Some(mut device) = runtime_device() else {
            return;
        };
        let before = device.allocated_bytes();
        let buffer = device
            .create_buffer(&GpuBufferDesc::new("staging", 4096, BufferUsage::staging()))
            .unwrap();
        let image = device
            .create_image(
                &GpuImageDesc::new("texture", 64, 64, TextureFormat::Rgba8Srgb)
                    .with_usage(TextureUsage::sampled()),
            )
            .unwrap();
        assert_eq!(device.live_resources(), 2);
        assert!(device.allocated_bytes() > before);
        assert!(device.destroy_buffer(buffer));
        assert!(device.destroy_image(image));
        assert_eq!(device.live_resources(), 0);
        assert_eq!(device.allocated_bytes(), before);
    }

    #[test]
    fn zero_sized_and_huge_buffers_are_typed_errors() {
        let Some(mut device) = runtime_device() else {
            return;
        };
        assert!(matches!(
            device.create_buffer(&GpuBufferDesc::new("empty", 0, BufferUsage::vertex())),
            Err(super::RhiError::Validation(_))
        ));
        assert!(matches!(
            device.create_buffer(&GpuBufferDesc::new(
                "huge",
                u64::MAX / 2,
                BufferUsage::vertex()
            )),
            Err(super::RhiError::OutOfMemory { .. })
        ));
    }

    #[test]
    fn image_usage_is_validated() {
        let Some(mut device) = runtime_device() else {
            return;
        };
        assert!(matches!(
            device.create_image(&GpuImageDesc::new(
                "bad-depth",
                32,
                32,
                TextureFormat::D32Float
            )),
            Err(super::RhiError::Validation(_))
        ));
        assert!(matches!(
            device.create_image(
                &GpuImageDesc::new("bad-color", 32, 32, TextureFormat::Rgba8Srgb)
                    .with_usage(TextureUsage::depth_stencil())
            ),
            Err(super::RhiError::Validation(_))
        ));
    }

    #[test]
    fn clear_color_offscreen_frame_runs_without_validation_errors() {
        let Some(mut device) = runtime_device() else {
            return;
        };
        let before = device.validation_errors();
        let usage = TextureUsage {
            render_target: true,
            transfer_src: true,
            ..TextureUsage::none()
        };
        let target = device
            .create_image(
                &GpuImageDesc::new("clear-target", 256, 256, TextureFormat::Rgba8Srgb)
                    .with_usage(usage),
            )
            .unwrap();
        let mut commands = device.create_command_buffer().unwrap();
        commands.begin().unwrap();
        let mut batch = BarrierBatch::new();
        batch.transition(target, ImageState::Undefined, ImageState::ColorAttachment);
        commands.barriers(&batch).unwrap();
        let pass = RenderPassDesc::new("clear")
            .with_color(target)
            .with_clear([0.1, 0.2, 0.3, 1.0]);
        commands.begin_render_pass(&pass).unwrap();
        commands.end_render_pass().unwrap();
        let mut batch = BarrierBatch::new();
        batch.transition(target, ImageState::ColorAttachment, ImageState::TransferSrc);
        commands.barriers(&batch).unwrap();
        commands.end().unwrap();
        commands.finish().unwrap();
        device.wait_idle().unwrap();
        assert_eq!(device.validation_errors(), before);
        assert!(device.destroy_image(target));
    }

    #[test]
    fn offscreen_swapchain_acquires_and_resizes() {
        let Some(mut device) = runtime_device() else {
            return;
        };
        let mut swapchain = device
            .create_swapchain(&SwapchainDesc::new("offscreen", 320, 200))
            .unwrap();
        assert_eq!(swapchain.size(), (320, 200));
        let image = swapchain.acquire().unwrap();
        assert!(swapchain.acquire().is_err());
        swapchain.present(image).unwrap();
        swapchain.resize(640, 400).unwrap();
        assert_eq!(swapchain.size(), (640, 400));
        let image = swapchain.acquire().unwrap();
        swapchain.present(image).unwrap();
    }

    #[test]
    fn invalid_spirv_is_a_typed_error() {
        let Some(mut device) = runtime_device() else {
            return;
        };
        let garbage: &'static [u8] = Box::leak(vec![0_u8; 16].into_boxed_slice());
        assert!(matches!(
            device.create_pipeline(&PipelineDesc::compute("bad", garbage)),
            Err(super::RhiError::Validation(_))
        ));
        assert_eq!(device.pipeline_count(), 0);
    }

    #[test]
    fn pipeline_without_shaders_is_a_typed_error() {
        let Some(mut device) = runtime_device() else {
            return;
        };
        assert!(matches!(
            device.create_pipeline(&PipelineDesc::new("empty", PipelineKind::Graphics)),
            Err(super::RhiError::Validation(_))
        ));
        assert!(matches!(
            device.create_pipeline(&PipelineDesc::new("empty", PipelineKind::Compute)),
            Err(super::RhiError::Validation(_))
        ));
    }

    #[test]
    fn vram_leases_are_accounted() {
        let Some(mut device) = runtime_device() else {
            return;
        };
        let before = device.budget_snapshot().used;
        let lease = VramAllocator::allocate(&mut device, 4096, MemoryTag::Rendering).unwrap();
        assert_eq!(device.budget_snapshot().used, before + 4096);
        assert!(VramAllocator::free(&mut device, lease));
        assert_eq!(device.budget_snapshot().used, before);
    }

    #[test]
    fn query_pools_record_timestamps() {
        let Some(mut device) = runtime_device() else {
            return;
        };
        let pool = device
            .create_query_pool(&QueryPoolDesc::new("zones", 2))
            .unwrap();
        let mut commands = device.create_command_buffer().unwrap();
        commands.begin().unwrap();
        commands.reset_query_pool(pool, 0, 2).unwrap();
        commands.write_timestamp(pool, 0).unwrap();
        commands.write_timestamp(pool, 1).unwrap();
        assert!(commands.write_timestamp(pool, 2).is_err());
        commands.end().unwrap();
        commands.finish().unwrap();

        let results = device.read_query_pool(pool).unwrap();
        assert_eq!(results.len(), 2);
        assert!(device.destroy_query_pool(pool));
        assert!(device.read_query_pool(pool).is_err());
    }
}
