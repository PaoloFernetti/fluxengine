//! Конверсии RHI-типов в Vulkan и отображение ошибок.

#![allow(unsafe_code)]

use ash::vk;

use super::super::cmd::ImageState;
use super::super::error::RhiError;
use super::super::types::{
    AddressMode, BufferUsage, FilterMode, MipmapMode, PresentMode, TextureFormat, TextureUsage,
};

/// Отображает `vk::Result` в типизированную ошибку RHI.
pub(crate) fn map_vk(result: vk::Result) -> RhiError {
    match result {
        vk::Result::ERROR_OUT_OF_DEVICE_MEMORY | vk::Result::ERROR_OUT_OF_HOST_MEMORY => {
            RhiError::OutOfMemory { requested: 0 }
        }
        vk::Result::ERROR_DEVICE_LOST => RhiError::DeviceLost,
        other => RhiError::Validation(format!("vulkan: {other:?}")),
    }
}

/// Фильтр Vulkan для режима фильтрации (ADR-0041).
pub(crate) const fn filter(mode: FilterMode) -> vk::Filter {
    match mode {
        FilterMode::Nearest => vk::Filter::NEAREST,
        FilterMode::Linear => vk::Filter::LINEAR,
    }
}

/// Режим mip-фильтрации Vulkan (ADR-0041).
pub(crate) const fn mipmap(mode: MipmapMode) -> vk::SamplerMipmapMode {
    match mode {
        MipmapMode::Nearest => vk::SamplerMipmapMode::NEAREST,
        MipmapMode::Linear => vk::SamplerMipmapMode::LINEAR,
    }
}

/// Адресация Vulkan для режима за границей текстуры (ADR-0041).
pub(crate) const fn address(mode: AddressMode) -> vk::SamplerAddressMode {
    match mode {
        AddressMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        AddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
        AddressMode::MirrorRepeat => vk::SamplerAddressMode::MIRRORED_REPEAT,
        AddressMode::ClampToBorder => vk::SamplerAddressMode::CLAMP_TO_BORDER,
    }
}

/// RHI-формат для формата поверхности Vulkan.
pub(crate) const fn texture_format(format: vk::Format) -> Option<TextureFormat> {
    match format {
        vk::Format::R8G8B8A8_SRGB => Some(TextureFormat::Rgba8Srgb),
        vk::Format::R8G8B8A8_UNORM => Some(TextureFormat::Rgba8Unorm),
        vk::Format::B8G8R8A8_SRGB => Some(TextureFormat::Bgra8Srgb),
        vk::Format::R16G16B16A16_SFLOAT => Some(TextureFormat::Rgba16Float),
        vk::Format::R8_UNORM => Some(TextureFormat::R8Unorm),
        vk::Format::D32_SFLOAT => Some(TextureFormat::D32Float),
        _ => None,
    }
}

/// Режим представления Vulkan для RHI-режима.
pub(crate) const fn present_mode(mode: PresentMode) -> vk::PresentModeKHR {
    match mode {
        PresentMode::Fifo => vk::PresentModeKHR::FIFO,
        PresentMode::Mailbox => vk::PresentModeKHR::MAILBOX,
        PresentMode::Immediate => vk::PresentModeKHR::IMMEDIATE,
    }
}

/// RHI-режим для режима представления Vulkan.
pub(crate) const fn rhi_present_mode(mode: vk::PresentModeKHR) -> Option<PresentMode> {
    match mode {
        vk::PresentModeKHR::FIFO => Some(PresentMode::Fifo),
        vk::PresentModeKHR::MAILBOX => Some(PresentMode::Mailbox),
        vk::PresentModeKHR::IMMEDIATE => Some(PresentMode::Immediate),
        _ => None,
    }
}

/// Формат Vulkan для RHI-формата.
pub(crate) const fn format(format: TextureFormat) -> vk::Format {
    match format {
        TextureFormat::Rgba8Srgb => vk::Format::R8G8B8A8_SRGB,
        TextureFormat::Rgba8Unorm => vk::Format::R8G8B8A8_UNORM,
        TextureFormat::Bgra8Srgb => vk::Format::B8G8R8A8_SRGB,
        TextureFormat::Rgba16Float => vk::Format::R16G16B16A16_SFLOAT,
        TextureFormat::R8Unorm => vk::Format::R8_UNORM,
        TextureFormat::D32Float => vk::Format::D32_SFLOAT,
    }
}

/// Флаги использования буфера.
pub(crate) fn buffer_usage(usage: BufferUsage) -> vk::BufferUsageFlags {
    let mut flags = vk::BufferUsageFlags::empty();
    if usage.vertex {
        flags |= vk::BufferUsageFlags::VERTEX_BUFFER;
    }
    if usage.index {
        flags |= vk::BufferUsageFlags::INDEX_BUFFER;
    }
    if usage.uniform {
        flags |= vk::BufferUsageFlags::UNIFORM_BUFFER;
    }
    if usage.storage {
        flags |= vk::BufferUsageFlags::STORAGE_BUFFER;
    }
    if usage.transfer_src {
        flags |= vk::BufferUsageFlags::TRANSFER_SRC;
    }
    if usage.transfer_dst {
        flags |= vk::BufferUsageFlags::TRANSFER_DST;
    }
    if usage.indirect {
        flags |= vk::BufferUsageFlags::INDIRECT_BUFFER;
    }
    flags
}

/// Флаги использования изображения.
pub(crate) fn image_usage(usage: TextureUsage) -> vk::ImageUsageFlags {
    let mut flags = vk::ImageUsageFlags::empty();
    if usage.sampled {
        flags |= vk::ImageUsageFlags::SAMPLED;
    }
    if usage.render_target {
        flags |= vk::ImageUsageFlags::COLOR_ATTACHMENT;
    }
    if usage.depth_stencil {
        flags |= vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT;
    }
    if usage.storage {
        flags |= vk::ImageUsageFlags::STORAGE;
    }
    if usage.transfer_src {
        flags |= vk::ImageUsageFlags::TRANSFER_SRC;
    }
    if usage.transfer_dst {
        flags |= vk::ImageUsageFlags::TRANSFER_DST;
    }
    flags
}

/// Аспект изображения по формату.
pub(crate) const fn aspect(format: TextureFormat) -> vk::ImageAspectFlags {
    if format.is_depth() {
        vk::ImageAspectFlags::DEPTH
    } else {
        vk::ImageAspectFlags::COLOR
    }
}

/// Стадия, доступ и layout для состояния изображения.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StateDesc {
    /// Стадия конвейера.
    pub(crate) stage: vk::PipelineStageFlags,
    /// Маска доступа.
    pub(crate) access: vk::AccessFlags,
    /// Layout изображения.
    pub(crate) layout: vk::ImageLayout,
}

/// Описание состояния изображения для барьера.
pub(crate) fn image_state(state: ImageState) -> StateDesc {
    match state {
        ImageState::Undefined => StateDesc {
            stage: vk::PipelineStageFlags::TOP_OF_PIPE,
            access: vk::AccessFlags::empty(),
            layout: vk::ImageLayout::UNDEFINED,
        },
        ImageState::ColorAttachment => StateDesc {
            stage: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            access: vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        },
        ImageState::DepthAttachment => StateDesc {
            stage: vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS
                | vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
            access: vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        },
        ImageState::ShaderRead => StateDesc {
            stage: vk::PipelineStageFlags::FRAGMENT_SHADER | vk::PipelineStageFlags::COMPUTE_SHADER,
            access: vk::AccessFlags::SHADER_READ,
            layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        },
        ImageState::Storage => StateDesc {
            stage: vk::PipelineStageFlags::COMPUTE_SHADER | vk::PipelineStageFlags::FRAGMENT_SHADER,
            access: vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE,
            layout: vk::ImageLayout::GENERAL,
        },
        ImageState::TransferSrc => StateDesc {
            stage: vk::PipelineStageFlags::TRANSFER,
            access: vk::AccessFlags::TRANSFER_READ,
            layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        },
        ImageState::TransferDst => StateDesc {
            stage: vk::PipelineStageFlags::TRANSFER,
            access: vk::AccessFlags::TRANSFER_WRITE,
            layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        },
        ImageState::Present => StateDesc {
            stage: vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            access: vk::AccessFlags::empty(),
            layout: vk::ImageLayout::PRESENT_SRC_KHR,
        },
    }
}
