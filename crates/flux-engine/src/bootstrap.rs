//! Точка сборки: конфиг → логи → паник-хук → job system → движок.
//!
//! Порядок инициализации канона: Config (CLI + конфиги) → logging/profiling →
//! job system → platform → assets → ECS → render → physics/audio/ui → Engine.
//! На этапе 1 подсистемы ещё не реализованы: собираются только ядро и плагины.

use core::time::Duration;
use std::path::PathBuf;
use std::sync::{Arc, Once};

use flux_core::config::ConfigSection;
use flux_core::crash::{install_panic_hook, CrashHandler};
use flux_core::jobs::JobSystem;
use flux_core::logging::{init_logging, LoggingError};
use flux_core::platform::available_parallelism;
use flux_core::time::DEFAULT_FIXED_STEP;
use serde::Deserialize;
use thiserror::Error;

use crate::plugins::Plugin;
use crate::runtime::{Engine, RunSummary};
use crate::settings::UserSettings;

/// Графический бэкенд, выбираемый из конфига.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub enum GfxBackend {
    /// Без окна и GPU: сервер, тесты, soak.
    #[default]
    Null,
    /// Vulkan.
    Vulkan,
    /// DirectX 12.
    Dx12,
}

/// Конфигурация движка (bootstrap-уровень).
#[derive(Clone, Debug)]
pub struct EngineConfig {
    /// Шаг фиксированной симуляции.
    pub fixed_step: Duration,
    /// Анти-spiral: максимум шагов за кадр.
    pub max_steps_per_frame: u32,
    /// Фоновые воркеры; `0` — авто (`available_parallelism - 1`).
    pub worker_threads: usize,
    /// Фильтр `tracing` (`EnvFilter`).
    pub log_filter: String,
    /// Инициализировать глобальный логгер.
    pub logging: bool,
    /// Ставить паник-хук с breadcrumbs и дампом.
    pub crash_handler: bool,
    /// Каталог краш-дампов.
    pub dump_dir: PathBuf,
    /// Выбранный графический бэкенд.
    pub backend: GfxBackend,
    /// Пользовательские настройки.
    pub settings: UserSettings,
    /// Ограничивать FPS в `run_loop` (headless/тесты выключают).
    pub throttle: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            fixed_step: DEFAULT_FIXED_STEP,
            max_steps_per_frame: 8,
            worker_threads: 0,
            log_filter: "info".to_owned(),
            logging: true,
            crash_handler: false,
            dump_dir: PathBuf::from("crash-dumps"),
            backend: GfxBackend::Null,
            settings: UserSettings::default(),
            throttle: true,
        }
    }
}

impl EngineConfig {
    /// Проверяет инварианты конфигурации.
    ///
    /// # Errors
    ///
    /// Возвращает текст ошибки, если шаг нулевой, лимит шагов нулевой или
    /// пользовательские настройки не проходят валидацию.
    pub fn validate(&self) -> Result<(), String> {
        if self.fixed_step.is_zero() {
            return Err("fixed_step must be non-zero".to_owned());
        }
        if self.max_steps_per_frame == 0 {
            return Err("max_steps_per_frame must be at least 1".to_owned());
        }
        if self.worker_threads > 1024 {
            return Err(format!(
                "worker_threads looks unreasonable: {}",
                self.worker_threads
            ));
        }
        self.settings.validate()
    }
}

/// Ошибка сборки движка.
#[derive(Debug, Error)]
pub enum EngineError {
    /// Недопустимая конфигурация.
    #[error("invalid engine config: {0}")]
    InvalidConfig(String),
    /// Некорректный фильтр логирования.
    #[error("logging init failed: {0}")]
    Logging(String),
}

static PANIC_HOOK: Once = Once::new();

/// Сборщик движка.
pub struct EngineBuilder {
    config: EngineConfig,
    plugins: Vec<Box<dyn Plugin>>,
}

impl Default for EngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineBuilder {
    /// Создаёт сборщик с конфигурацией по умолчанию.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: EngineConfig::default(),
            plugins: Vec::new(),
        }
    }

    /// Заменяет конфигурацию целиком.
    #[must_use]
    pub fn with_config(mut self, config: EngineConfig) -> Self {
        self.config = config;
        self
    }

    /// Регистрирует плагин. Порядок регистрации — порядок вызовов шагов.
    #[must_use]
    pub fn with_plugin<P: Plugin + 'static>(mut self, plugin: P) -> Self {
        self.plugins.push(Box::new(plugin));
        self
    }

    /// Число фоновых воркеров (`0` — авто).
    #[must_use]
    pub fn with_worker_threads(mut self, workers: usize) -> Self {
        self.config.worker_threads = workers;
        self
    }

    /// Выбирает графический бэкенд.
    #[must_use]
    pub fn with_backend(mut self, backend: GfxBackend) -> Self {
        self.config.backend = backend;
        self
    }

    /// Пользовательские настройки.
    #[must_use]
    pub fn with_settings(mut self, settings: UserSettings) -> Self {
        self.config.settings = settings;
        self
    }

    /// Включает паник-хук с breadcrumbs и текстовым дампом.
    #[must_use]
    pub fn with_crash_handler(mut self, dump_dir: impl Into<PathBuf>) -> Self {
        self.config.crash_handler = true;
        self.config.dump_dir = dump_dir.into();
        self
    }

    /// Собирает движок: конфиг → логи → паник-хук → jobs → плагины.
    ///
    /// # Errors
    ///
    /// Возвращает [`EngineError::InvalidConfig`] при недопустимом конфиге и
    /// [`EngineError::Logging`] при некорректном фильтре логирования.
    pub fn build(self) -> Result<Engine, EngineError> {
        self.config.validate().map_err(EngineError::InvalidConfig)?;

        if self.config.logging {
            match init_logging(&self.config.log_filter) {
                Ok(()) | Err(LoggingError::AlreadyInitialized) => {}
                Err(LoggingError::InvalidFilter(filter)) => {
                    return Err(EngineError::Logging(filter));
                }
            }
        }

        if self.config.crash_handler {
            let handler = CrashHandler::new(self.config.dump_dir.clone());
            PANIC_HOOK.call_once(move || install_panic_hook(handler));
        }

        let workers = if self.config.worker_threads == 0 {
            available_parallelism().saturating_sub(1).max(1)
        } else {
            self.config.worker_threads
        };
        let jobs = Arc::new(JobSystem::new(workers));

        let mut engine = Engine::new(self.config, jobs, self.plugins);
        engine.build_plugins();
        Ok(engine)
    }

    /// Тонкая обёртка для игр: bootstrap → цикл → shutdown.
    ///
    /// Окно и event loop не трогает: для игр с окном цикл принадлежит
    /// вызывающему (см. `examples/minimal.rs`).
    pub fn run(self) -> ! {
        match self.build() {
            Ok(engine) => engine.run_loop(),
            Err(error) => {
                eprintln!("flux-engine: {error}");
                std::process::exit(1);
            }
        }
    }

    /// Собирает движок и прогоняет ровно `frames` кадров без throttle.
    ///
    /// # Errors
    ///
    /// Те же, что у [`EngineBuilder::build`].
    pub fn run_frames(self, frames: u64) -> Result<RunSummary, EngineError> {
        let engine = self.build()?;
        Ok(engine.run_frames(frames))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_fixed_step() {
        let config = EngineConfig {
            fixed_step: Duration::ZERO,
            ..EngineConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_zero_max_steps() {
        let config = EngineConfig {
            max_steps_per_frame: 0,
            ..EngineConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_invalid_settings() {
        let config = EngineConfig {
            settings: UserSettings {
                master_volume: 2.0,
                ..UserSettings::default()
            },
            ..EngineConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn builder_builds_headless_engine() {
        let config = EngineConfig {
            logging: false,
            crash_handler: false,
            throttle: false,
            ..EngineConfig::default()
        };
        let engine = EngineBuilder::new()
            .with_config(config)
            .with_worker_threads(2)
            .build();
        assert!(engine.is_ok());
    }
}
