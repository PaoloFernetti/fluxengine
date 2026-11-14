//! Контракт swapchain.

use super::{
    error::RhiError,
    types::{PresentMode, SwapchainDesc, SwapchainId, TextureFormat},
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

    /// Представляет образ на экран.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — образ не был получен или индекс не совпадает.
    fn present(&mut self, image_index: u32) -> Result<(), RhiError>;
}

/// Дескриптор swapchain — реэкспорт для удобства реализаций.
pub type SwapchainDescriptor = SwapchainDesc;
