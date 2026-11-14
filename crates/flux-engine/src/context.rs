//! `EngineContext` — структурированный доступ плагинов к подсистемам.
//!
//! Плагин видит только trait-объекты контрактов и базовые сервисы ядра:
//! время, события, job system, настройки, frame-арену. Конкретные типы
//! бэкендов сюда не попадают.

use flux_core::contracts::{AudioDevice, NetBackend, Platform, Renderer, ScriptVM};
use flux_core::events::EventRegistry;
use flux_core::jobs::{JobHandle, JobSystem};
use flux_core::memory::FrameArena;
use flux_core::time::TimeContext;

use crate::settings::UserSettings;

/// Сервисы движка, доступные плагинам во время шага.
pub(crate) struct EngineServices<'a> {
    pub time: &'a TimeContext,
    pub events: &'a EventRegistry,
    pub jobs: &'a JobSystem,
    pub settings: &'a UserSettings,
    pub renderer: Option<&'a dyn Renderer>,
    pub platform: Option<&'a dyn Platform>,
    pub audio: Option<&'a dyn AudioDevice>,
    pub net: Option<&'a dyn NetBackend>,
    pub script: Option<&'a dyn ScriptVM>,
}

/// Контекст шага: ручка доступа к подсистемам для плагинов.
pub struct EngineContext<'a> {
    services: EngineServices<'a>,
    arena: &'a mut FrameArena,
    frame_jobs: &'a mut Vec<JobHandle>,
}

impl<'a> EngineContext<'a> {
    pub(crate) fn new(
        services: EngineServices<'a>,
        arena: &'a mut FrameArena,
        frame_jobs: &'a mut Vec<JobHandle>,
    ) -> Self {
        Self {
            services,
            arena,
            frame_jobs,
        }
    }

    /// Контекст времени: тик, alpha, пауза, масштаб.
    #[must_use]
    pub fn time(&self) -> &TimeContext {
        self.services.time
    }

    /// Реестр типизированных событий.
    #[must_use]
    pub fn events(&self) -> &EventRegistry {
        self.services.events
    }

    /// Job system.
    #[must_use]
    pub fn jobs(&self) -> &JobSystem {
        self.services.jobs
    }

    /// Пользовательские настройки.
    #[must_use]
    pub fn settings(&self) -> &UserSettings {
        self.services.settings
    }

    /// Frame-арена: сбрасывается в начале кадра.
    pub fn arena(&mut self) -> &mut FrameArena {
        self.arena
    }

    /// Рендер-бэкенд как контракт (без деталей RHI).
    #[must_use]
    pub fn renderer(&self) -> Option<&dyn Renderer> {
        self.services.renderer
    }

    /// Платформенный слой.
    #[must_use]
    pub fn platform(&self) -> Option<&dyn Platform> {
        self.services.platform
    }

    /// Аудио-устройство.
    #[must_use]
    pub fn audio(&self) -> Option<&dyn AudioDevice> {
        self.services.audio
    }

    /// Сетевой транспорт.
    #[must_use]
    pub fn net(&self) -> Option<&dyn NetBackend> {
        self.services.net
    }

    /// Скриптовая VM.
    #[must_use]
    pub fn script(&self) -> Option<&dyn ScriptVM> {
        self.services.script
    }

    /// Ставит задачу в job system и регистрирует её для fence кадра.
    ///
    /// Движок дожидается всех задач, поставленных через контекст, в конце
    /// [`crate::Engine::frame`]. Паника внутри задачи не роняет воркер:
    /// статус `Panicked` учитывается в статистике кадра.
    pub fn spawn<F>(&mut self, job: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let handle = self.services.jobs.spawn(job);
        self.frame_jobs.push(handle);
    }
}
