//! Платформенные сервисы ядра: приоритеты потоков, параллелизм.
//!
//! Per-OS реализация окна/ввода живёт в `flux-platform` (этап 3).

use std::num::NonZeroUsize;

use thiserror::Error;

/// Ошибка платформенного сервиса.
#[derive(Debug, Error)]
pub enum PlatformError {
    /// Приоритет потока недоступен.
    #[error("thread priority is unavailable: {0}")]
    ThreadPriority(String),
}

/// Приоритет потока.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    /// Фоновые задачи.
    Low,
    /// Обычный.
    Normal,
    /// Горячий путь.
    High,
    /// Аудио/рендер.
    Critical,
}

/// Запрашивает приоритет текущего потока.
///
/// MVP: фиксирует намерение; платформенная реализация — этап 3.
///
/// # Errors
///
/// Возвращает ошибку, если платформа не поддерживает приоритеты.
pub fn set_current_thread_priority(priority: Priority) -> Result<(), PlatformError> {
    tracing::debug!(?priority, "thread priority requested (MVP: no-op)");
    Ok(())
}

/// Доступный параллелизм машины (минимум 1).
#[must_use]
pub fn available_parallelism() -> usize {
    std::thread::available_parallelism().map_or(1, NonZeroUsize::get)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallelism_is_positive() {
        assert!(available_parallelism() >= 1);
    }

    #[test]
    fn priority_request_is_accepted() {
        assert!(set_current_thread_priority(Priority::High).is_ok());
    }
}
