//! Базовые типы RHI: бэкенды, форматы, дескрипторы, хэндлы, capabilities.

use serde::{Deserialize, Serialize};

/// Графический бэкенд.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BackendKind {
    /// Headless/null: ресурсы учитываются, GPU-команд нет.
    #[default]
    Null,
    /// Vulkan (feature `vulkan`, итерация I1).
    Vulkan,
    /// DirectX 12 (feature `dx12`, позже).
    Dx12,
}

impl BackendKind {
    /// Имя бэкенда для логов и диагностики.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Vulkan => "vulkan",
            Self::Dx12 => "dx12",
        }
    }
}

/// Режим представления swapchain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PresentMode {
    /// Вертикальная синхронизация (обязательный режим).
    #[default]
    Fifo,
    /// Тройная буферизация без блокировки.
    Mailbox,
    /// Без синхронизации.
    Immediate,
}

/// Формат текстуры.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextureFormat {
    /// RGBA8 sRGB (albedo/G-buffer RT0).
    Rgba8Srgb,
    /// RGBA8 linear.
    Rgba8Unorm,
    /// BGRA8 sRGB (типичный swapchain).
    Bgra8Srgb,
    /// RGBA16F (oct-normal G-buffer RT1).
    Rgba16Float,
    /// R8 (маски/одноканальные данные).
    R8Unorm,
    /// D32 float (Reversed-Z depth).
    D32Float,
}

impl TextureFormat {
    /// Байт на пиксель (без сжатия).
    #[must_use]
    pub const fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::Rgba8Srgb | Self::Rgba8Unorm | Self::Bgra8Srgb | Self::D32Float => 4,
            Self::Rgba16Float => 8,
            Self::R8Unorm => 1,
        }
    }

    /// Глубинный ли формат.
    #[must_use]
    pub const fn is_depth(self) -> bool {
        matches!(self, Self::D32Float)
    }

    /// sRGB ли формат.
    #[must_use]
    pub const fn is_srgb(self) -> bool {
        matches!(self, Self::Rgba8Srgb | Self::Bgra8Srgb)
    }
}

/// Использование GPU-буфера.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BufferUsage {
    /// Вершинный буфер.
    pub vertex: bool,
    /// Индексный буфер.
    pub index: bool,
    /// Uniform-буфер.
    pub uniform: bool,
    /// Storage-буфер.
    pub storage: bool,
    /// Источник копирования.
    pub transfer_src: bool,
    /// Приёмник копирования.
    pub transfer_dst: bool,
    /// Косвенные аргументы отрисовки.
    pub indirect: bool,
}

impl BufferUsage {
    /// Вершинный буфер.
    #[must_use]
    pub const fn vertex() -> Self {
        Self {
            vertex: true,
            ..Self::none()
        }
    }

    /// Индексный буфер.
    #[must_use]
    pub const fn index() -> Self {
        Self {
            index: true,
            ..Self::none()
        }
    }

    /// Uniform-буфер.
    #[must_use]
    pub const fn uniform() -> Self {
        Self {
            uniform: true,
            ..Self::none()
        }
    }

    /// Storage-буфер.
    #[must_use]
    pub const fn storage() -> Self {
        Self {
            storage: true,
            ..Self::none()
        }
    }

    /// Staging-буфер: источник и приёмник копирования.
    #[must_use]
    pub const fn staging() -> Self {
        Self {
            transfer_src: true,
            transfer_dst: true,
            ..Self::none()
        }
    }

    /// Ни одного флага.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            vertex: false,
            index: false,
            uniform: false,
            storage: false,
            transfer_src: false,
            transfer_dst: false,
            indirect: false,
        }
    }

    /// Есть ли хотя бы один флаг.
    #[must_use]
    pub const fn any(self) -> bool {
        self.vertex
            || self.index
            || self.uniform
            || self.storage
            || self.transfer_src
            || self.transfer_dst
            || self.indirect
    }
}

/// Использование GPU-изображения.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextureUsage {
    /// Читается шейдером.
    pub sampled: bool,
    /// Цветовой render target.
    pub render_target: bool,
    /// Depth/stencil target.
    pub depth_stencil: bool,
    /// Storage-изображение.
    pub storage: bool,
    /// Источник копирования.
    pub transfer_src: bool,
    /// Приёмник копирования.
    pub transfer_dst: bool,
}

impl TextureUsage {
    /// Sampled-текстура.
    #[must_use]
    pub const fn sampled() -> Self {
        Self {
            sampled: true,
            ..Self::none()
        }
    }

    /// Цветовой render target.
    #[must_use]
    pub const fn render_target() -> Self {
        Self {
            render_target: true,
            ..Self::none()
        }
    }

    /// Depth/stencil target.
    #[must_use]
    pub const fn depth_stencil() -> Self {
        Self {
            depth_stencil: true,
            ..Self::none()
        }
    }

    /// Storage-изображение.
    #[must_use]
    pub const fn storage() -> Self {
        Self {
            storage: true,
            ..Self::none()
        }
    }

    /// Staging-изображение.
    #[must_use]
    pub const fn staging() -> Self {
        Self {
            transfer_src: true,
            transfer_dst: true,
            ..Self::none()
        }
    }

    /// Ни одного флага.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            sampled: false,
            render_target: false,
            depth_stencil: false,
            storage: false,
            transfer_src: false,
            transfer_dst: false,
        }
    }

    /// Есть ли хотя бы один флаг.
    #[must_use]
    pub const fn any(self) -> bool {
        self.sampled
            || self.render_target
            || self.depth_stencil
            || self.storage
            || self.transfer_src
            || self.transfer_dst
    }
}

macro_rules! handle {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
        )]
        pub struct $name(pub u64);
    };
}

handle!(BufferId, "Хэндл GPU-буфера.");
handle!(ImageId, "Хэндл GPU-изображения.");
handle!(PipelineId, "Хэндл pipeline state object.");
handle!(DescriptorSetId, "Хэндл дескрипторного набора.");
handle!(SwapchainId, "Хэндл swapchain.");
handle!(QueryPoolId, "Хэндл пула GPU-запросов (таймстемпы).");

/// Дескриптор GPU-буфера.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuBufferDesc {
    /// Метка для диагностики.
    pub label: &'static str,
    /// Размер в байтах.
    pub size: u64,
    /// Использование.
    pub usage: BufferUsage,
}

impl GpuBufferDesc {
    /// Буфер с меткой, размером и использованием.
    #[must_use]
    pub const fn new(label: &'static str, size: u64, usage: BufferUsage) -> Self {
        Self { label, size, usage }
    }
}

/// Дескриптор GPU-изображения.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuImageDesc {
    /// Метка для диагностики.
    pub label: &'static str,
    /// Ширина.
    pub width: u32,
    /// Высота.
    pub height: u32,
    /// Формат.
    pub format: TextureFormat,
    /// Использование.
    pub usage: TextureUsage,
    /// Число mip-уровней.
    pub mip_levels: u32,
}

impl GpuImageDesc {
    /// Изображение с меткой, размером и форматом.
    #[must_use]
    pub const fn new(label: &'static str, width: u32, height: u32, format: TextureFormat) -> Self {
        Self {
            label,
            width,
            height,
            format,
            usage: TextureUsage::sampled(),
            mip_levels: 1,
        }
    }

    /// Задаёт использование.
    #[must_use]
    pub const fn with_usage(mut self, usage: TextureUsage) -> Self {
        self.usage = usage;
        self
    }

    /// Задаёт число mip-уровней.
    #[must_use]
    pub const fn with_mips(mut self, mip_levels: u32) -> Self {
        self.mip_levels = mip_levels;
        self
    }
}

/// Тип ресурса в дескрипторном наборе.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BindingKind {
    /// Uniform-буфер (`ConstantBuffer` в HLSL).
    UniformBuffer,
    /// Storage-буфер (`StructuredBuffer`/`RWStructuredBuffer`).
    StorageBuffer,
    /// Изображение только для чтения (`Texture2D`).
    SampledImage,
    /// Storage-изображение (`RWTexture2D`).
    StorageImage,
}

/// Одно объявление дескрипторного биндинга.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DescriptorBinding {
    /// Индекс биндинга (set 0).
    pub binding: u32,
    /// Тип ресурса.
    pub kind: BindingKind,
    /// Размер массива (bindless-элементы); `1` — одиночный ресурс.
    pub count: u32,
}

impl DescriptorBinding {
    /// Одиночный биндинг.
    #[must_use]
    pub const fn new(binding: u32, kind: BindingKind) -> Self {
        Self {
            binding,
            kind,
            count: 1,
        }
    }

    /// Биндинг-массив (bindless).
    #[must_use]
    pub const fn with_count(mut self, count: u32) -> Self {
        self.count = count;
        self
    }
}

/// Описание дескрипторного набора (layout set 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DescriptorSetDesc {
    /// Метка для диагностики.
    pub label: &'static str,
    /// Биндинги в порядке объявления.
    pub bindings: Vec<DescriptorBinding>,
}

impl DescriptorSetDesc {
    /// Пустой набор.
    #[must_use]
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
            bindings: Vec::new(),
        }
    }

    /// Задаёт биндинги.
    #[must_use]
    pub fn with_bindings(mut self, bindings: &[DescriptorBinding]) -> Self {
        self.bindings = bindings.to_vec();
        self
    }
}

/// Ресурс, привязываемый к дескриптору.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorResource {
    /// Буфер.
    Buffer(BufferId),
    /// Изображение.
    Image(ImageId),
}

/// Запись дескриптора: биндинг, индекс в массиве и ресурс.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DescriptorWrite {
    /// Индекс биндинга.
    pub binding: u32,
    /// Индекс в bindless-массиве.
    pub array_index: u32,
    /// Ресурс.
    pub resource: DescriptorResource,
}

impl DescriptorWrite {
    /// Запись буфера.
    #[must_use]
    pub const fn buffer(binding: u32, array_index: u32, buffer: BufferId) -> Self {
        Self {
            binding,
            array_index,
            resource: DescriptorResource::Buffer(buffer),
        }
    }

    /// Запись изображения.
    #[must_use]
    pub const fn image(binding: u32, array_index: u32, image: ImageId) -> Self {
        Self {
            binding,
            array_index,
            resource: DescriptorResource::Image(image),
        }
    }
}

/// Описание пула GPU-запросов.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryPoolDesc {
    /// Метка для диагностики.
    pub label: &'static str,
    /// Число запросов (1..=4096).
    pub count: u32,
}

impl QueryPoolDesc {
    /// Пул таймстемпов.
    #[must_use]
    pub const fn new(label: &'static str, count: u32) -> Self {
        Self { label, count }
    }
}

/// Дескриптор swapchain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SwapchainDesc {
    /// Метка.
    pub label: &'static str,
    /// Ширина.
    pub width: u32,
    /// Высота.
    pub height: u32,
    /// Формат изображений.
    pub format: TextureFormat,
    /// Режим представления.
    pub present_mode: PresentMode,
    /// Кадров в полёте.
    pub frames_in_flight: u32,
}

impl SwapchainDesc {
    /// Swapchain с меткой и размером.
    #[must_use]
    pub const fn new(label: &'static str, width: u32, height: u32) -> Self {
        Self {
            label,
            width,
            height,
            format: TextureFormat::Bgra8Srgb,
            present_mode: PresentMode::Fifo,
            frames_in_flight: 2,
        }
    }

    /// Задаёт формат.
    #[must_use]
    pub const fn with_format(mut self, format: TextureFormat) -> Self {
        self.format = format;
        self
    }

    /// Задаёт режим представления.
    #[must_use]
    pub const fn with_present_mode(mut self, present_mode: PresentMode) -> Self {
        self.present_mode = present_mode;
        self
    }

    /// Задаёт число кадров в полёте.
    #[must_use]
    pub const fn with_frames_in_flight(mut self, frames_in_flight: u32) -> Self {
        self.frames_in_flight = frames_in_flight;
        self
    }
}

/// Тип pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PipelineKind {
    /// Графический pipeline.
    Graphics,
    /// Compute pipeline.
    Compute,
}

/// Топология примитивов.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrimitiveTopology {
    /// Треугольники.
    #[default]
    TriangleList,
    /// Линии.
    LineList,
    /// Точки.
    PointList,
}

/// Дескриптор pipeline: SPIR-V, vertex layout и совместимость с render pass.
///
/// `PipelineDesc::new` оставлен для null-бэкенда и тестов контракта (пустые
/// шейдеры); реальные бэкенды требуют `graphics`/`compute` с SPIR-V.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipelineDesc {
    /// Метка.
    pub label: &'static str,
    /// Тип.
    pub kind: PipelineKind,
    /// SPIR-V вершинного шейдера.
    pub vertex_shader: &'static [u8],
    /// SPIR-V фрагментного шейдера.
    pub fragment_shader: &'static [u8],
    /// SPIR-V compute-шейдера.
    pub compute_shader: &'static [u8],
    /// Имя entry point вершинного шейдера.
    pub vertex_entry: &'static str,
    /// Имя entry point фрагментного шейдера.
    pub fragment_entry: &'static str,
    /// Имя entry point compute-шейдера.
    pub compute_entry: &'static str,
    /// Форматы цветовых аттачментов render pass.
    pub color_formats: &'static [TextureFormat],
    /// Формат depth-аттачмента.
    pub depth_format: Option<TextureFormat>,
    /// Шаг вершины в байтах (позиция `f32x3` — 12).
    pub vertex_stride: u32,
    /// Топология.
    pub topology: PrimitiveTopology,
    /// Дескрипторные биндинги set 0 (layout без дескрипторов по умолчанию).
    pub bindings: &'static [DescriptorBinding],
}

impl PipelineDesc {
    /// Pipeline с меткой и типом без шейдеров (null/контрактные тесты).
    #[must_use]
    pub const fn new(label: &'static str, kind: PipelineKind) -> Self {
        Self {
            label,
            kind,
            vertex_shader: &[],
            fragment_shader: &[],
            compute_shader: &[],
            vertex_entry: "vs_main",
            fragment_entry: "fs_main",
            compute_entry: "cs_main",
            color_formats: &[],
            depth_format: None,
            vertex_stride: 12,
            topology: PrimitiveTopology::TriangleList,
            bindings: &[],
        }
    }

    /// Графический pipeline с SPIR-V вершинного и фрагментного шейдеров.
    #[must_use]
    pub const fn graphics(
        label: &'static str,
        vertex_shader: &'static [u8],
        fragment_shader: &'static [u8],
    ) -> Self {
        Self {
            kind: PipelineKind::Graphics,
            vertex_shader,
            fragment_shader,
            ..Self::new(label, PipelineKind::Graphics)
        }
    }

    /// Compute pipeline с SPIR-V.
    #[must_use]
    pub const fn compute(label: &'static str, compute_shader: &'static [u8]) -> Self {
        Self {
            kind: PipelineKind::Compute,
            compute_shader,
            ..Self::new(label, PipelineKind::Compute)
        }
    }

    /// Задаёт форматы цветовых аттачментов.
    #[must_use]
    pub const fn with_color_formats(mut self, formats: &'static [TextureFormat]) -> Self {
        self.color_formats = formats;
        self
    }

    /// Задаёт depth-формат.
    #[must_use]
    pub const fn with_depth_format(mut self, format: TextureFormat) -> Self {
        self.depth_format = Some(format);
        self
    }

    /// Задаёт шаг вершины.
    #[must_use]
    pub const fn with_vertex_stride(mut self, stride: u32) -> Self {
        self.vertex_stride = stride;
        self
    }

    /// Задаёт топологию.
    #[must_use]
    pub const fn with_topology(mut self, topology: PrimitiveTopology) -> Self {
        self.topology = topology;
        self
    }

    /// Задаёт дескрипторные биндинги set 0.
    #[must_use]
    pub const fn with_bindings(mut self, bindings: &'static [DescriptorBinding]) -> Self {
        self.bindings = bindings;
        self
    }

    /// Задаёт имена entry point.
    #[must_use]
    pub const fn with_entries(
        mut self,
        vertex_entry: &'static str,
        fragment_entry: &'static str,
    ) -> Self {
        self.vertex_entry = vertex_entry;
        self.fragment_entry = fragment_entry;
        self
    }
}

/// Описание render pass.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderPassDesc {
    /// Метка.
    pub label: &'static str,
    /// Цветовые аттачменты.
    pub color_attachments: Vec<ImageId>,
    /// Depth-аттачмент.
    pub depth_attachment: Option<ImageId>,
    /// Цвет очистки.
    pub clear_color: [f32; 4],
}

impl RenderPassDesc {
    /// Пустой pass с меткой.
    #[must_use]
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
            color_attachments: Vec::new(),
            depth_attachment: None,
            clear_color: [0.0, 0.0, 0.0, 1.0],
        }
    }

    /// Добавляет цветовой аттачмент.
    #[must_use]
    pub fn with_color(mut self, image: ImageId) -> Self {
        self.color_attachments.push(image);
        self
    }

    /// Задаёт depth-аттачмент.
    #[must_use]
    pub fn with_depth(mut self, image: ImageId) -> Self {
        self.depth_attachment = Some(image);
        self
    }

    /// Задаёт цвет очистки.
    #[must_use]
    pub fn with_clear(mut self, color: [f32; 4]) -> Self {
        self.clear_color = color;
        self
    }

    /// Есть ли хотя бы один аттачмент.
    #[must_use]
    pub fn has_attachments(&self) -> bool {
        !self.color_attachments.is_empty() || self.depth_attachment.is_some()
    }
}

/// Возможности устройства.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capabilities {
    /// Бэкенд.
    pub backend: BackendKind,
    /// Максимальный размер 2D-текстуры.
    pub max_texture_dimension_2d: u32,
    /// Максимум кадров в полёте.
    pub max_frames_in_flight: u32,
    /// Максимум bindless-дескрипторов.
    pub max_bindless_descriptors: u32,
    /// Поддержка timeline semaphores.
    pub supports_timeline_semaphores: bool,
    /// Поддержка descriptor indexing (bindless).
    pub supports_descriptor_indexing: bool,
    /// Доступные режимы представления.
    pub present_modes: Vec<PresentMode>,
    /// Поддерживаемые цветовые форматы.
    pub color_formats: Vec<TextureFormat>,
    /// Поддерживаемые глубинные форматы.
    pub depth_formats: Vec<TextureFormat>,
}

impl Capabilities {
    /// Консервативные возможности headless-null.
    #[must_use]
    pub fn headless() -> Self {
        Self {
            backend: BackendKind::Null,
            max_texture_dimension_2d: 16_384,
            max_frames_in_flight: 2,
            max_bindless_descriptors: 65_536,
            supports_timeline_semaphores: false,
            supports_descriptor_indexing: true,
            present_modes: vec![PresentMode::Fifo],
            color_formats: vec![TextureFormat::Rgba8Srgb, TextureFormat::Bgra8Srgb],
            depth_formats: vec![TextureFormat::D32Float],
        }
    }

    /// Поддерживается ли режим представления.
    #[must_use]
    pub fn supports_present_mode(&self, mode: PresentMode) -> bool {
        self.present_modes.contains(&mode)
    }

    /// Поддерживается ли цветовой формат.
    #[must_use]
    pub fn supports_color_format(&self, format: TextureFormat) -> bool {
        self.color_formats.contains(&format)
    }

    /// Поддерживается ли глубинный формат.
    #[must_use]
    pub fn supports_depth_format(&self, format: TextureFormat) -> bool {
        self.depth_formats.contains(&format)
    }
}

impl Default for Capabilities {
    fn default() -> Self {
        Self::headless()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_metadata_is_consistent() {
        assert_eq!(TextureFormat::Rgba8Srgb.bytes_per_pixel(), 4);
        assert_eq!(TextureFormat::Rgba16Float.bytes_per_pixel(), 8);
        assert!(TextureFormat::D32Float.is_depth());
        assert!(TextureFormat::Rgba8Srgb.is_srgb());
        assert!(!TextureFormat::Rgba8Unorm.is_srgb());
    }

    #[test]
    fn usage_flags_are_typed() {
        assert!(BufferUsage::vertex().any());
        assert!(!BufferUsage::none().any());
        assert!(BufferUsage::staging().transfer_dst);
        assert!(TextureUsage::depth_stencil().depth_stencil);
        assert!(!TextureUsage::none().any());
    }

    #[test]
    fn headless_capabilities_cover_render_graph_requests() {
        let caps = Capabilities::headless();
        assert_eq!(caps.backend, BackendKind::Null);
        assert!(caps.supports_present_mode(PresentMode::Fifo));
        assert!(caps.supports_color_format(TextureFormat::Rgba8Srgb));
        assert!(caps.supports_depth_format(TextureFormat::D32Float));
        assert!(caps.max_bindless_descriptors >= 65_536);
    }

    #[test]
    fn render_pass_desc_tracks_attachments() {
        let pass = RenderPassDesc::new("main")
            .with_color(ImageId(1))
            .with_depth(ImageId(2))
            .with_clear([0.1, 0.2, 0.3, 1.0]);
        assert!(pass.has_attachments());
        assert_eq!(pass.color_attachments, vec![ImageId(1)]);
        assert_eq!(pass.clear_color, [0.1, 0.2, 0.3, 1.0]);
    }
}
