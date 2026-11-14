//! Логирование и телеметрия кадра.

use std::collections::VecDeque;

use thiserror::Error;
use tracing_subscriber::EnvFilter;

/// Каналы логирования (targets).
pub mod channels {
    /// Рендер.
    pub const RENDER: &str = "render";
    /// ECS.
    pub const ECS: &str = "ecs";
    /// Физика.
    pub const PHYSICS: &str = "physics";
    /// Анимации.
    pub const ANIM: &str = "anim";
    /// AI.
    pub const AI: &str = "ai";
    /// Аудио.
    pub const AUDIO: &str = "audio";
    /// Сеть.
    pub const NET: &str = "net";
    /// Ассеты.
    pub const ASSET: &str = "asset";
    /// Скрипты.
    pub const SCRIPT: &str = "script";
    /// Редактор.
    pub const EDITOR: &str = "editor";
    /// Ядро.
    pub const CORE: &str = "core";
}

/// Ошибка инициализации логов.
#[derive(Debug, Error)]
pub enum LoggingError {
    /// Некорректный фильтр.
    #[error("invalid log filter: {0}")]
    InvalidFilter(String),
    /// Логгер уже инициализирован.
    #[error("logger already initialized")]
    AlreadyInitialized,
}

/// Инициализирует глобальный логгер.
///
/// # Errors
///
/// Возвращает ошибку при некорректном фильтре или повторной инициализации.
pub fn init_logging(filter: &str) -> Result<(), LoggingError> {
    let env_filter = EnvFilter::try_new(filter)
        .map_err(|error| LoggingError::InvalidFilter(error.to_string()))?;

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(true)
        .try_init()
        .map_err(|_| LoggingError::AlreadyInitialized)
}

/// Телеметрия кадра: FPS и перцентили frametime.
#[derive(Debug)]
pub struct FrameTelemetry {
    samples: VecDeque<f32>,
    capacity: usize,
}

impl FrameTelemetry {
    /// Создаёт окно на `capacity` кадров.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    /// Добавляет frametime в миллисекундах.
    pub fn push(&mut self, frame_ms: f32) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back(frame_ms);
    }

    /// Медиана frametime.
    #[must_use]
    pub fn p50(&self) -> f32 {
        self.percentile(50.0)
    }

    /// 99-й перцентиль frametime.
    #[must_use]
    pub fn p99(&self) -> f32 {
        self.percentile(99.0)
    }

    /// Средний FPS.
    #[must_use]
    pub fn fps(&self) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let sum: f32 = self.samples.iter().sum();
        let average = sum / self.samples.len() as f32;
        if average <= 0.0 {
            0.0
        } else {
            1_000.0 / average
        }
    }

    fn percentile(&self, percentile: f32) -> f32 {
        if self.samples.is_empty() {
            return 0.0;
        }
        let mut values: Vec<f32> = self.samples.iter().copied().collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
        let index = ((values.len() as f32 - 1.0) * percentile / 100.0).round() as usize;
        values.get(index).copied().unwrap_or(0.0)
    }
}

/// Проверка инварианта: логирует ошибку, но не паникует.
#[macro_export]
macro_rules! verify {
    ($condition:expr) => {
        if !$condition {
            tracing::error!("verify failed: {}", stringify!($condition));
        }
    };
    ($condition:expr, $($arg:tt)*) => {
        if !$condition {
            tracing::error!($($arg)*);
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_percentiles_are_ordered() {
        let mut telemetry = FrameTelemetry::new(100);
        for frame in 1..=100 {
            telemetry.push(frame as f32);
        }
        assert!(telemetry.p50() <= telemetry.p99());
        assert!(telemetry.p99() >= 99.0);
        assert!(telemetry.fps() > 0.0);
    }

    #[test]
    fn telemetry_window_is_bounded() {
        let mut telemetry = FrameTelemetry::new(3);
        for frame in 0..10 {
            telemetry.push(frame as f32);
        }
        assert_eq!(telemetry.samples.len(), 3);
    }

    #[test]
    fn invalid_filter_is_reported() {
        assert!(matches!(
            init_logging("=invalid=filter"),
            Err(LoggingError::InvalidFilter(_))
        ));
    }
}
