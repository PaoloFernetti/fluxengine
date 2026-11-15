//! Offscreen swapchain Vulkan: headless PresentTarget без поверхности окна.
//!
//! Реальный `VkSwapchainKHR` требует `VkSurfaceKHR` из оконной системы и
//! появится вместе с present-проходом Render Graph (I3). Здесь реализован
//! headless-таргет: изображения создаются и уничтожаются устройством,
//! acquire/present детерминированы, resize пересоздаёт изображения.

#![allow(unsafe_code)]

use std::sync::Arc;

use ash::vk;

use super::super::error::RhiError;
use super::super::swapchain::Swapchain;
use super::super::types::{ImageId, PresentMode, SwapchainDesc, SwapchainId, TextureFormat};
use super::memory::Allocation;
use super::{convert, DeviceContext, ImageEntry};

struct SwapchainImage {
    image: vk::Image,
    view: vk::ImageView,
    allocation: Allocation,
}

/// Offscreen swapchain Vulkan.
pub struct VulkanSwapchain {
    context: Arc<DeviceContext>,
    id: SwapchainId,
    format: TextureFormat,
    present_mode: PresentMode,
    width: u32,
    height: u32,
    frames_in_flight: u32,
    images: Vec<SwapchainImage>,
    /// Хэндлы образов в реестре устройства (ADR-0045).
    image_ids: Vec<ImageId>,
    acquired: Option<u32>,
    next: u32,
}

impl VulkanSwapchain {
    /// Создаёт offscreen swapchain.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — размер/формат/режим/кадры не поддержаны.
    pub(crate) fn new(context: Arc<DeviceContext>, desc: &SwapchainDesc) -> Result<Self, RhiError> {
        if desc.width == 0 || desc.height == 0 {
            return Err(RhiError::validation("swapchain size must be non-zero"));
        }
        if desc.format.is_depth() {
            return Err(RhiError::validation(
                "swapchain format must be a color format",
            ));
        }
        if !context.caps.supports_color_format(desc.format) {
            return Err(RhiError::validation("unsupported swapchain format"));
        }
        if !context.caps.supports_present_mode(desc.present_mode) {
            return Err(RhiError::validation("unsupported present mode"));
        }
        if desc.frames_in_flight == 0 || desc.frames_in_flight > context.caps.max_frames_in_flight {
            return Err(RhiError::validation("invalid frames_in_flight"));
        }
        let id = SwapchainId(context.store.lock().next_id());
        let mut swapchain = Self {
            context,
            id,
            format: desc.format,
            present_mode: desc.present_mode,
            width: desc.width,
            height: desc.height,
            frames_in_flight: desc.frames_in_flight,
            images: Vec::new(),
            image_ids: Vec::new(),
            acquired: None,
            next: 0,
        };
        swapchain.create_images()?;
        Ok(swapchain)
    }

    fn create_images(&mut self) -> Result<(), RhiError> {
        let device = &self.context.device;
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::SAMPLED;
        let info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(convert::format(self.format))
            .extent(vk::Extent3D {
                width: self.width,
                height: self.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED);
        for _ in 0..self.frames_in_flight {
            // SAFETY: create info is valid for a 2D color image.
            let image = unsafe { device.create_image(&info, None) }.map_err(convert::map_vk)?;
            // SAFETY: image was just created.
            let requirements = unsafe { device.get_image_memory_requirements(image) };
            let requested = u64::from(self.width)
                .saturating_mul(u64::from(self.height))
                .saturating_mul(u64::from(self.format.bytes_per_pixel()));
            let allocation = {
                let mut store = self.context.store.lock();
                match store.memory.allocate(
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
                }
            };
            // SAFETY: allocation satisfies the image requirements.
            if let Err(error) = unsafe { device.bind_image_memory(image, allocation.memory, 0) } {
                self.context.store.lock().memory.free(device, allocation);
                // SAFETY: image is unbound and destroyed on the error path.
                unsafe { device.destroy_image(image, None) };
                return Err(convert::map_vk(error));
            }
            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(convert::format(self.format))
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1),
                );
            // SAFETY: image is valid and the view description is complete.
            let view = match unsafe { device.create_image_view(&view_info, None) } {
                Ok(view) => view,
                Err(error) => {
                    self.context.store.lock().memory.free(device, allocation);
                    // SAFETY: image is unbound and destroyed on the error path.
                    unsafe { device.destroy_image(image, None) };
                    return Err(convert::map_vk(error));
                }
            };
            let id = {
                let mut store = self.context.store.lock();
                let id = store.next_id();
                store.images.insert(
                    id,
                    ImageEntry {
                        image,
                        view,
                        width: self.width,
                        height: self.height,
                        format: self.format,
                        allocation: None,
                        owned: false,
                    },
                );
                ImageId(id)
            };
            self.image_ids.push(id);
            self.images.push(SwapchainImage {
                image,
                view,
                allocation,
            });
        }
        Ok(())
    }

    fn destroy_images(&mut self) {
        let device = &self.context.device;
        let mut store = self.context.store.lock();
        store.remove_swapchain_images(&self.image_ids);
        self.image_ids.clear();
        // Framebuffer-ы ссылаются на view, которые сейчас уничтожаются.
        let framebuffers = store.take_framebuffers();
        drop(store);
        for framebuffer in framebuffers {
            // SAFETY: framebuffer belongs to the store cache and is destroyed once.
            unsafe { device.destroy_framebuffer(framebuffer, None) };
        }
        let mut store = self.context.store.lock();
        for entry in self.images.drain(..) {
            // SAFETY: images are owned by this swapchain and freed once.
            unsafe {
                device.destroy_image_view(entry.view, None);
                device.destroy_image(entry.image, None);
            }
            store.memory.free(device, entry.allocation);
        }
    }
}

impl Swapchain for VulkanSwapchain {
    fn id(&self) -> SwapchainId {
        self.id
    }

    fn format(&self) -> TextureFormat {
        self.format
    }

    fn present_mode(&self) -> PresentMode {
        self.present_mode
    }

    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn image_count(&self) -> usize {
        self.images.len()
    }

    fn image(&self, index: u32) -> Option<ImageId> {
        self.image_ids.get(index as usize).copied()
    }

    fn acquired_image(&self) -> Option<ImageId> {
        self.acquired.and_then(|index| self.image(index))
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), RhiError> {
        if width == 0 || height == 0 {
            return Err(RhiError::validation("swapchain size must be non-zero"));
        }
        if width > self.context.caps.max_texture_dimension_2d
            || height > self.context.caps.max_texture_dimension_2d
        {
            return Err(RhiError::validation(
                "swapchain exceeds max texture dimension",
            ));
        }
        if self.acquired.is_some() {
            return Err(RhiError::validation("swapchain image is still acquired"));
        }
        self.destroy_images();
        self.width = width;
        self.height = height;
        self.next = 0;
        self.create_images()
    }

    fn acquire(&mut self) -> Result<u32, RhiError> {
        if self.acquired.is_some() {
            return Err(RhiError::validation("swapchain image already acquired"));
        }
        if self.images.is_empty() {
            return Err(RhiError::validation("swapchain has no images"));
        }
        let index = self.next % self.images.len() as u32;
        self.next = self.next.wrapping_add(1);
        self.acquired = Some(index);
        Ok(index)
    }

    fn present(&mut self, image_index: u32) -> Result<(), RhiError> {
        if self.acquired != Some(image_index) {
            return Err(RhiError::validation(
                "presented image index does not match the acquired image",
            ));
        }
        self.acquired = None;
        Ok(())
    }
}

impl Drop for VulkanSwapchain {
    fn drop(&mut self) {
        self.destroy_images();
    }
}

/// Swapchain Vulkan, привязанный к `VkSurfaceKHR` (окно или headless).
///
/// В отличие от offscreen-варианта образы принадлежат `VkSwapchainKHR`:
/// `acquire` забирает образ через семафор, `present` возвращает его в очередь
/// представления. `resize` пересоздаёт цепочку (с ожиданием idle), поэтому
/// десять подряд изменений размера не оставляют висящих образов.
pub struct VulkanPresentSwapchain {
    context: Arc<DeviceContext>,
    id: SwapchainId,
    surface: vk::SurfaceKHR,
    swapchain: Option<vk::SwapchainKHR>,
    format: TextureFormat,
    present_mode: PresentMode,
    width: u32,
    height: u32,
    frames_in_flight: u32,
    images: Vec<vk::Image>,
    views: Vec<vk::ImageView>,
    /// Хэндлы образов в реестре устройства (ADR-0045).
    image_ids: Vec<ImageId>,
    acquire_fence: vk::Fence,
    transition_fence: vk::Fence,
    acquired: Option<u32>,
}

impl VulkanPresentSwapchain {
    pub(crate) fn new(
        context: Arc<DeviceContext>,
        surface: vk::SurfaceKHR,
        desc: &SwapchainDesc,
    ) -> Result<Self, RhiError> {
        if desc.width == 0 || desc.height == 0 {
            return Err(RhiError::validation("swapchain size must be non-zero"));
        }
        if desc.format.is_depth() {
            return Err(RhiError::validation(
                "swapchain format must be a color format",
            ));
        }
        if desc.frames_in_flight == 0 || desc.frames_in_flight > context.caps.max_frames_in_flight {
            return Err(RhiError::validation("invalid frames_in_flight"));
        }
        if context.swapchain_loader.is_none() {
            return Err(RhiError::Unsupported {
                feature: "VK_KHR_swapchain",
            });
        }
        // SAFETY: physical device, queue family and surface are valid.
        let present_supported = unsafe {
            context.surface_loader.get_physical_device_surface_support(
                context.physical,
                context.queue_family,
                surface,
            )
        }
        .map_err(convert::map_vk)?;
        if !present_supported {
            return Err(RhiError::Surface(
                "graphics queue cannot present to the surface".into(),
            ));
        }
        let fence_info = vk::FenceCreateInfo::default();
        // SAFETY: create info is default/valid.
        let acquire_fence =
            unsafe { context.device.create_fence(&fence_info, None) }.map_err(convert::map_vk)?;
        // SAFETY: create info is default/valid.
        let transition_fence = match unsafe { context.device.create_fence(&fence_info, None) } {
            Ok(fence) => fence,
            Err(error) => {
                // SAFETY: fence is owned by this call.
                unsafe { context.device.destroy_fence(acquire_fence, None) };
                return Err(convert::map_vk(error));
            }
        };
        let id = SwapchainId(context.store.lock().next_id());
        let mut swapchain = Self {
            context,
            id,
            surface,
            swapchain: None,
            format: desc.format,
            present_mode: desc.present_mode,
            width: desc.width,
            height: desc.height,
            frames_in_flight: desc.frames_in_flight,
            images: Vec::new(),
            views: Vec::new(),
            image_ids: Vec::new(),
            acquire_fence,
            transition_fence,
            acquired: None,
        };
        swapchain.create_swapchain()?;
        Ok(swapchain)
    }

    fn create_swapchain(&mut self) -> Result<(), RhiError> {
        let loader = self
            .context
            .swapchain_loader
            .as_ref()
            .ok_or(RhiError::Unsupported {
                feature: "VK_KHR_swapchain",
            })?;
        // SAFETY: surface belongs to the instance and physical device is valid.
        let capabilities = unsafe {
            self.context
                .surface_loader
                .get_physical_device_surface_capabilities(self.context.physical, self.surface)
        }
        .map_err(convert::map_vk)?;
        // SAFETY: same as above.
        let formats = unsafe {
            self.context
                .surface_loader
                .get_physical_device_surface_formats(self.context.physical, self.surface)
        }
        .map_err(convert::map_vk)?;
        // SAFETY: same as above.
        let present_modes = unsafe {
            self.context
                .surface_loader
                .get_physical_device_surface_present_modes(self.context.physical, self.surface)
        }
        .map_err(convert::map_vk)?;
        // Приоритет: точный запрошенный sRGB-формат → любой sRGB-формат,
        // который умеет RHI → любой RHI-формат. Headless WSI lavapipe отдаёт
        // только `R8G8B8A8_UNORM`, поэтому fallback на `formats.first()`
        // приводил к `Unsupported` уже на этапе вывода кадра.
        let supported = |format: vk::Format| convert::texture_format(format).is_some();
        let chosen_format = formats
            .iter()
            .find(|candidate| {
                candidate.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                    && candidate.format == convert::format(self.format)
            })
            .or_else(|| {
                formats.iter().find(|candidate| {
                    candidate.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                        && supported(candidate.format)
                })
            })
            .or_else(|| formats.iter().find(|candidate| supported(candidate.format)))
            .copied()
            .ok_or_else(|| RhiError::Surface("surface has no RHI-supported formats".into()))?;
        self.format = convert::texture_format(chosen_format.format).ok_or_else(|| {
            RhiError::Surface(format!(
                "surface format {:?} is not supported by RHI",
                chosen_format.format
            ))
        })?;
        let requested = convert::present_mode(self.present_mode);
        let chosen_mode = if present_modes.contains(&requested) {
            requested
        } else {
            present_modes
                .first()
                .copied()
                .ok_or_else(|| RhiError::Surface("surface has no present modes".into()))?
        };
        self.present_mode = convert::rhi_present_mode(chosen_mode).unwrap_or(PresentMode::Fifo);
        let extent = if capabilities.current_extent.width != u32::MAX {
            capabilities.current_extent
        } else {
            vk::Extent2D {
                width: self.width.clamp(
                    capabilities.min_image_extent.width,
                    capabilities.max_image_extent.width,
                ),
                height: self.height.clamp(
                    capabilities.min_image_extent.height,
                    capabilities.max_image_extent.height,
                ),
            }
        };
        self.width = extent.width;
        self.height = extent.height;
        let mut image_count = self.frames_in_flight.max(capabilities.min_image_count);
        if capabilities.max_image_count != 0 {
            image_count = image_count.min(capabilities.max_image_count);
        }
        let composite_alpha = [
            vk::CompositeAlphaFlagsKHR::OPAQUE,
            vk::CompositeAlphaFlagsKHR::INHERIT,
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
        ]
        .into_iter()
        .find(|flag| capabilities.supported_composite_alpha.contains(*flag))
        .unwrap_or(vk::CompositeAlphaFlagsKHR::OPAQUE);
        let pre_transform = if capabilities
            .supported_transforms
            .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
        {
            vk::SurfaceTransformFlagsKHR::IDENTITY
        } else {
            capabilities.current_transform
        };
        let info = vk::SwapchainCreateInfoKHR::default()
            .surface(self.surface)
            .min_image_count(image_count)
            .image_format(chosen_format.format)
            .image_color_space(chosen_format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(pre_transform)
            .composite_alpha(composite_alpha)
            .present_mode(chosen_mode)
            .clipped(true);
        // SAFETY: all descriptions reference live objects owned by the context.
        let swapchain = unsafe { loader.create_swapchain(&info, None) }.map_err(convert::map_vk)?;
        // SAFETY: swapchain was just created.
        let images = match unsafe { loader.get_swapchain_images(swapchain) } {
            Ok(images) => images,
            Err(error) => {
                // SAFETY: swapchain is owned by this call and has no views yet.
                unsafe { loader.destroy_swapchain(swapchain, None) };
                return Err(convert::map_vk(error));
            }
        };
        let mut views = Vec::with_capacity(images.len());
        for image in &images {
            let view_info = vk::ImageViewCreateInfo::default()
                .image(*image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(chosen_format.format)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .base_mip_level(0)
                        .level_count(1)
                        .base_array_layer(0)
                        .layer_count(1),
                );
            // SAFETY: image belongs to the swapchain; view description is complete.
            match unsafe { self.context.device.create_image_view(&view_info, None) } {
                Ok(view) => views.push(view),
                Err(error) => {
                    for view in views {
                        // SAFETY: views are owned by this call.
                        unsafe { self.context.device.destroy_image_view(view, None) };
                    }
                    // SAFETY: swapchain has no views after cleanup.
                    unsafe { loader.destroy_swapchain(swapchain, None) };
                    return Err(convert::map_vk(error));
                }
            }
        }
        let image_ids = {
            let mut store = self.context.store.lock();
            let mut ids = Vec::with_capacity(images.len());
            for (image, view) in images.iter().zip(&views) {
                let id = store.next_id();
                store.images.insert(
                    id,
                    ImageEntry {
                        image: *image,
                        view: *view,
                        width: extent.width,
                        height: extent.height,
                        format: self.format,
                        allocation: None,
                        owned: false,
                    },
                );
                ids.push(ImageId(id));
            }
            ids
        };
        self.images = images;
        self.views = views;
        self.image_ids = image_ids;
        self.swapchain = Some(swapchain);
        self.acquired = None;
        Ok(())
    }

    fn destroy_swapchain(&mut self) {
        let Some(swapchain) = self.swapchain.take() else {
            return;
        };
        let device = &self.context.device;
        {
            let mut store = self.context.store.lock();
            store.remove_swapchain_images(&self.image_ids);
            self.image_ids.clear();
            let framebuffers = store.take_framebuffers();
            drop(store);
            for framebuffer in framebuffers {
                // SAFETY: framebuffer belongs to the store cache and is destroyed once.
                unsafe { device.destroy_framebuffer(framebuffer, None) };
            }
        }
        for view in self.views.drain(..) {
            // SAFETY: views are owned by this swapchain and destroyed once.
            unsafe { device.destroy_image_view(view, None) };
        }
        if let Some(loader) = self.context.swapchain_loader.as_ref() {
            // SAFETY: swapchain is owned by this object and destroyed once.
            unsafe { loader.destroy_swapchain(swapchain, None) };
        }
        self.images.clear();
    }
}

impl Swapchain for VulkanPresentSwapchain {
    fn id(&self) -> SwapchainId {
        self.id
    }

    fn format(&self) -> TextureFormat {
        self.format
    }

    fn present_mode(&self) -> PresentMode {
        self.present_mode
    }

    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn image_count(&self) -> usize {
        self.images.len()
    }

    fn image(&self, index: u32) -> Option<ImageId> {
        self.image_ids.get(index as usize).copied()
    }

    fn acquired_image(&self) -> Option<ImageId> {
        self.acquired.and_then(|index| self.image(index))
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), RhiError> {
        if width == 0 || height == 0 {
            return Err(RhiError::validation("swapchain size must be non-zero"));
        }
        if self.acquired.is_some() {
            return Err(RhiError::validation("swapchain image is still acquired"));
        }
        // SAFETY: waiting for idle is valid while the device is alive.
        unsafe { self.context.device.device_wait_idle() }.map_err(convert::map_vk)?;
        self.destroy_swapchain();
        self.width = width;
        self.height = height;
        self.create_swapchain()
    }

    fn acquire(&mut self) -> Result<u32, RhiError> {
        if self.acquired.is_some() {
            return Err(RhiError::validation("swapchain image already acquired"));
        }
        let swapchain = self
            .swapchain
            .ok_or_else(|| RhiError::Surface("swapchain is not initialized".into()))?;
        let loader = self
            .context
            .swapchain_loader
            .as_ref()
            .ok_or(RhiError::Unsupported {
                feature: "VK_KHR_swapchain",
            })?;
        // SAFETY: fence is unsignaled after the previous wait.
        unsafe { self.context.device.reset_fences(&[self.acquire_fence]) }
            .map_err(convert::map_vk)?;
        // SAFETY: swapchain and fence are alive; fence signals when the image is free.
        let (index, _suboptimal) = match unsafe {
            loader.acquire_next_image(
                swapchain,
                u64::MAX,
                vk::Semaphore::null(),
                self.acquire_fence,
            )
        } {
            Ok(result) => result,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                return Err(RhiError::Surface("swapchain is out of date".into()));
            }
            Err(error) => return Err(convert::map_vk(error)),
        };
        // SAFETY: fence belongs to this swapchain and is waited once.
        unsafe {
            self.context
                .device
                .wait_for_fences(&[self.acquire_fence], true, u64::MAX)
        }
        .map_err(convert::map_vk)?;
        // Образ отдаётся как ColorAttachment: Render Graph пишет в него
        // напрямую (ADR-0045). Содержимое не нужно — переход из UNDEFINED.
        self.transition_image(
            index,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
        )?;
        self.acquired = Some(index);
        Ok(index)
    }

    fn present(&mut self, image_index: u32) -> Result<(), RhiError> {
        if self.acquired != Some(image_index) {
            return Err(RhiError::validation(
                "presented image index does not match the acquired image",
            ));
        }
        let swapchain = self
            .swapchain
            .ok_or_else(|| RhiError::Surface("swapchain is not initialized".into()))?;
        let loader = self
            .context
            .swapchain_loader
            .as_ref()
            .ok_or(RhiError::Unsupported {
                feature: "VK_KHR_swapchain",
            })?;
        // Содержимое записано как ColorAttachment; перед present — PRESENT_SRC.
        self.transition_image(
            image_index,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
        )?;
        let swapchains = [swapchain];
        let indices = [image_index];
        let info = vk::PresentInfoKHR::default()
            .swapchains(&swapchains)
            .image_indices(&indices);
        // SAFETY: transition completed on the same queue before this call.
        let result = unsafe { loader.queue_present(self.context.queue, &info) };
        self.acquired = None;
        match result {
            Ok(_) => Ok(()),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                Err(RhiError::Surface("swapchain is out of date".into()))
            }
            Err(error) => Err(convert::map_vk(error)),
        }
    }
}

impl VulkanPresentSwapchain {
    /// Переводит образ между layout-ами одним одноразовым submit-ом.
    fn transition_image(
        &self,
        image_index: u32,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
        src_stage: vk::PipelineStageFlags,
        dst_stage: vk::PipelineStageFlags,
    ) -> Result<(), RhiError> {
        let Some(&image) = self.images.get(image_index as usize) else {
            return Err(RhiError::Surface("acquired image is out of range".into()));
        };
        let device = &self.context.device;
        let allocation = vk::CommandBufferAllocateInfo::default()
            .command_pool(self.context.command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        // SAFETY: pool belongs to the device.
        let buffers =
            unsafe { device.allocate_command_buffers(&allocation) }.map_err(convert::map_vk)?;
        let Some(&command) = buffers.first() else {
            return Err(RhiError::validation(
                "command buffer allocation returned none",
            ));
        };
        let begin = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        // SAFETY: command buffer is fresh and not pending.
        unsafe { device.begin_command_buffer(command, &begin) }.map_err(convert::map_vk)?;
        let barrier = vk::ImageMemoryBarrier::default()
            .old_layout(old_layout)
            .new_layout(new_layout)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1),
            );
        // SAFETY: command buffer is recording; barrier references a swapchain image.
        unsafe {
            device.cmd_pipeline_barrier(
                command,
                src_stage,
                dst_stage,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
        }
        // SAFETY: command buffer is recording.
        unsafe { device.end_command_buffer(command) }.map_err(convert::map_vk)?;
        let commands = [command];
        let submit = vk::SubmitInfo::default().command_buffers(&commands);
        // SAFETY: fence is unsignaled and used once.
        unsafe { device.reset_fences(&[self.transition_fence]) }.map_err(convert::map_vk)?;
        // SAFETY: command buffer and fence are valid.
        unsafe { device.queue_submit(self.context.queue, &[submit], self.transition_fence) }
            .map_err(convert::map_vk)?;
        // SAFETY: fence belongs to this object and is waited once.
        let waited = unsafe { device.wait_for_fences(&[self.transition_fence], true, u64::MAX) };
        // SAFETY: submission completed (or failed); the buffer is no longer pending.
        unsafe { device.free_command_buffers(self.context.command_pool, &commands) };
        waited.map_err(convert::map_vk)
    }
}

impl Drop for VulkanPresentSwapchain {
    fn drop(&mut self) {
        // SAFETY: waiting for idle is valid while the device is alive.
        let _ = unsafe { self.context.device.device_wait_idle() };
        self.destroy_swapchain();
        // SAFETY: fences are owned by this object and destroyed once.
        unsafe {
            self.context.device.destroy_fence(self.acquire_fence, None);
            self.context
                .device
                .destroy_fence(self.transition_fence, None);
        }
    }
}
