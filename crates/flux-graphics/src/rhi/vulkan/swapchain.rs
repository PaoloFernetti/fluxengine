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
use super::super::types::{PresentMode, SwapchainDesc, SwapchainId, TextureFormat};
use super::memory::Allocation;
use super::{convert, DeviceContext};

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
