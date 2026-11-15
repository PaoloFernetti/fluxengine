//! Контракт swapchain.
//!
//! Образы swapchain доступны как [`ImageId`] (ADR-0045): их можно указать
//! цветовым аттачментом в [`RenderPassDesc`](super::RenderPassDesc), прочитать
//! через копирование и представить. Образ принадлежит swapchain, а не
//! вызывающему: [`RhiDevice::destroy_image`](super::RhiDevice::destroy_image)
//! для него возвращает `false`, а хэндл перестаёт быть валидным после
//! `resize`/уничтожения swapchain.

use super::{
    error::RhiError,
    types::{ImageId, PresentMode, SwapchainDesc, SwapchainId, TextureFormat},
};

/// Цепочка представления, привязанная к окну/таргету.
pub trait Swapchain: Send {
    /// Хэндл swapchain.
    fn id(&self) -> SwapchainId;

    /// Формат изображений.
    fn format(&self) -> TextureFormat;

    /// Режим представления.
    fn present_mode(&self) -> PresentMode;

    /// Текущий размер `(width, height)`.
    fn size(&self) -> (u32, u32);

    /// Число образов в цепочке.
    fn image_count(&self) -> usize;

    /// Образ по индексу (валиден до `resize`/уничтожения swapchain).
    ///
    /// Образ зарегистрирован в устройстве как «принадлежащий swapchain»:
    /// его можно использовать в render pass, барьерах и копировании, но
    /// нельзя освобождать через `destroy_image`.
    fn image(&self, index: u32) -> Option<ImageId>;

    /// Образ, полученный последним [`Self::acquire`].
    ///
    /// После `acquire` образ находится в состоянии `ColorAttachment`; после
    /// [`Self::present`] хэндл недействителен до следующего acquire.
    fn acquired_image(&self) -> Option<ImageId>;

    /// Пересоздаёт изображения под новый размер.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — нулевой размер или превышен лимит.
    fn resize(&mut self, width: u32, height: u32) -> Result<(), RhiError>;

    /// Забирает следующий образ для записи.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — предыдущий образ не представлен.
    fn acquire(&mut self) -> Result<u32, RhiError>;

    /// Переводит образ в `Present` и представляет его.
    ///
    /// Ожидает, что содержимое записано в образ как в `ColorAttachment`
    /// (render pass или копирование с переходом в `ColorAttachment`).
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — образ не был получен или индекс не совпадает.
    fn present(&mut self, image_index: u32) -> Result<(), RhiError>;
}

/// Дескриптор swapchain — реэкспорт для удобства реализаций.
pub type SwapchainDescriptor = SwapchainDesc;
