//! Командные буферы и барьеры.
//!
//! Render Graph строит [`BarrierBatch`] из read/write отношений пассов;
//! RHI только транслирует барьеры в API.

use super::{
    error::RhiError,
    types::{BufferId, DescriptorSetId, ImageId, PipelineId, QueryPoolId, RenderPassDesc},
};

/// Состояние изображения для барьера.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageState {
    /// Содержимое не определено.
    Undefined,
    /// Цветовой render target.
    ColorAttachment,
    /// Depth/stencil target.
    DepthAttachment,
    /// Чтение шейдером.
    ShaderRead,
    /// Storage-изображение (compute/graphics write).
    Storage,
    /// Источник копирования.
    TransferSrc,
    /// Приёмник копирования.
    TransferDst,
    /// Готово к представлению.
    Present,
}

/// Один барьер изображения.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageBarrier {
    /// Изображение.
    pub image: ImageId,
    /// Исходное состояние.
    pub from: ImageState,
    /// Целевое состояние.
    pub to: ImageState,
}

/// Набор барьеров, построенный Render Graph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BarrierBatch {
    barriers: Vec<ImageBarrier>,
}

impl BarrierBatch {
    /// Пустой набор.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Добавляет переход изображения.
    pub fn transition(&mut self, image: ImageId, from: ImageState, to: ImageState) {
        self.barriers.push(ImageBarrier { image, from, to });
    }

    /// Добавляет барьеры другого набора.
    pub fn extend(&mut self, other: &Self) {
        self.barriers.extend_from_slice(&other.barriers);
    }

    /// Число барьеров.
    #[must_use]
    pub fn len(&self) -> usize {
        self.barriers.len()
    }

    /// Пуст ли набор.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.barriers.is_empty()
    }

    /// Очищает набор.
    pub fn clear(&mut self) {
        self.barriers.clear();
    }

    /// Барьеры в порядке добавления.
    #[must_use]
    pub fn barriers(&self) -> &[ImageBarrier] {
        &self.barriers
    }
}

/// Командный буфер.
pub trait CommandBuffer: Send {
    /// Начинает запись команд.
    fn begin(&mut self) -> Result<(), RhiError>;

    /// Завершает запись команд.
    fn end(&mut self) -> Result<(), RhiError>;

    /// Начинает render pass.
    fn begin_render_pass(&mut self, desc: &RenderPassDesc) -> Result<(), RhiError>;

    /// Завершает render pass.
    fn end_render_pass(&mut self) -> Result<(), RhiError>;

    /// Сбрасывает диапазон запросов пула.
    fn reset_query_pool(
        &mut self,
        pool: QueryPoolId,
        first: u32,
        count: u32,
    ) -> Result<(), RhiError>;

    /// Пишет таймстемп в запрос `index`.
    fn write_timestamp(&mut self, pool: QueryPoolId, index: u32) -> Result<(), RhiError>;

    /// Устанавливает pipeline.
    fn set_pipeline(&mut self, pipeline: PipelineId) -> Result<(), RhiError>;

    /// Биндит дескрипторный набор set 0 для текущего pipeline.
    fn bind_descriptor_set(&mut self, set: DescriptorSetId) -> Result<(), RhiError>;

    /// Загружает push constants в текущий pipeline (ADR-0042).
    ///
    /// `data` непустая и кратна 4 байтам; размер не должен превышать
    /// `PipelineDesc::push_constant_size` привязанного pipeline.
    fn push_constants(&mut self, data: &[u8]) -> Result<(), RhiError>;

    /// Копирует буфер в изображение (mip 0, слой 0, весь размер).
    ///
    /// Источник обязан быть staging-буфером. Вызов допустим только внутри
    /// copy-узла Render Graph (ADR-0039): барьеры выставляет граф.
    fn copy_buffer_to_image(&mut self, buffer: BufferId, image: ImageId) -> Result<(), RhiError>;

    /// Копирует изображение в буфер (mip 0, слой 0, весь размер).
    ///
    /// Приёмник обязан быть staging-буфером (readback). Вызов допустим только
    /// внутри copy-узла Render Graph (ADR-0039).
    fn copy_image_to_buffer(&mut self, image: ImageId, buffer: BufferId) -> Result<(), RhiError>;

    /// Рисует вершины.
    fn draw(&mut self, vertices: u32, instances: u32) -> Result<(), RhiError>;

    /// Запускает compute dispatch.
    fn dispatch(&mut self, groups: [u32; 3]) -> Result<(), RhiError>;

    /// Применяет барьеры.
    fn barriers(&mut self, batch: &BarrierBatch) -> Result<(), RhiError>;

    /// Завершает буфер и передаёт его на исполнение.
    fn finish(self: Box<Self>) -> Result<(), RhiError>;
}
