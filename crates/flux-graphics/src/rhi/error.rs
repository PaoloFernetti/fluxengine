//! Типизированные ошибки RHI.
//!
//! Ожидаемые отказы (неподдерживаемая фича, OOM, невалидный дескриптор)
//! возвращаются как `RhiError`, а не паника.

use thiserror::Error;

/// Ошибка RHI.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RhiError {
    /// Фича/бэкенд ещё не реализованы.
    #[error("unsupported RHI feature: {feature}")]
    Unsupported {
        /// Имя фичи или бэкенда.
        feature: &'static str,
    },
    /// Не хватает памяти под ресурс.
    #[error("out of memory: requested {requested} bytes")]
    OutOfMemory {
        /// Запрошено байт.
        requested: u64,
    },
    /// Устройство потеряно (драйвер/GPU).
    #[error("device lost")]
    DeviceLost,
    /// Хэндл не принадлежит устройству или уже уничтожен.
    #[error("invalid handle: {kind} #{id}")]
    InvalidHandle {
        /// Тип ресурса (`buffer`, `image`, `pipeline`, `swapchain`).
        kind: &'static str,
        /// Идентификатор.
        id: u64,
    },
    /// Нарушен контракт вызова или дескриптора.
    #[error("validation failed: {0}")]
    Validation(String),
    /// Ошибка поверхности/окна.
    #[error("surface error: {0}")]
    Surface(String),
}

impl RhiError {
    /// Ошибка валидации из строки.
    #[must_use]
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }
}
