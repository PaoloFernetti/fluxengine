//! RHI: контракты устройства, swapchain, команд и VRAM.
//!
//! Модуль не зависит от Render Graph и конкретных API. Vulkan-бэкенд
//! подключён feature-гейтом `vulkan` (I1/I5, ADR-0034/0035); DX12 — следующим
//! этапом. `PipelineDesc` принимает SPIR-V и описывает PSO без привязки к API.

pub mod cmd;
pub mod device;
pub mod error;
pub mod null;
pub mod swapchain;
pub mod types;
pub mod vram;
#[cfg(feature = "vulkan")]
pub mod vulkan;

pub use cmd::{BarrierBatch, CommandBuffer, ImageBarrier, ImageState};
pub use device::RhiDevice;
pub use error::RhiError;
pub use null::{NullCommandBuffer, NullDevice, NullSwapchain, NullVramAllocator};
pub use swapchain::Swapchain;
pub use types::{
    BackendKind, BindingKind, BufferId, BufferUsage, Capabilities, DescriptorBinding,
    DescriptorResource, DescriptorSetDesc, DescriptorSetId, DescriptorWrite, GpuBufferDesc,
    GpuImageDesc, ImageId, PipelineDesc, PipelineId, PipelineKind, PresentMode, PrimitiveTopology,
    QueryPoolDesc, QueryPoolId, RenderPassDesc, SwapchainDesc, SwapchainId, TextureFormat,
    TextureUsage,
};
pub use vram::{BudgetSnapshot, Pressure, VramAllocator, VramLease};
#[cfg(feature = "vulkan")]
pub use vulkan::VulkanDevice;

/// Создаёт устройство выбранного бэкенда.
///
/// # Errors
///
/// [`RhiError::Unsupported`] — feature бэкенда выключен, нет Vulkan loader-а
/// или подходящего GPU; [`RhiError::Validation`]/[`RhiError::OutOfMemory`] —
/// драйвер отверг instance/device.
pub fn create_device(backend: BackendKind) -> Result<Box<dyn RhiDevice>, RhiError> {
    match backend {
        BackendKind::Null => Ok(Box::new(NullDevice::new())),
        BackendKind::Vulkan => {
            #[cfg(feature = "vulkan")]
            {
                Ok(Box::new(vulkan::VulkanDevice::new()?))
            }
            #[cfg(not(feature = "vulkan"))]
            {
                Err(RhiError::Unsupported {
                    feature: "vulkan backend (feature `vulkan` is disabled)",
                })
            }
        }
        BackendKind::Dx12 => Err(RhiError::Unsupported {
            feature: "dx12 backend (I2)",
        }),
    }
}
