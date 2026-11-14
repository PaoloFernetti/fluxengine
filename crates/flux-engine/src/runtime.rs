//! Главный цикл кадра: `tick` → `frame` → `acquire`.
//!
//! Циклом владеет вызывающий (ADR-0027). Движок экспонирует шаги:
//! фиксированную симуляцию, кадр рендера и ожидание GPU отдельно.

use core::time::Duration;
use std::sync::Arc;
use std::time::Instant;

use flux_core::contracts::{AudioDevice, NetBackend, Platform, Renderer, ScriptVM};
use flux_core::crash::breadcrumbs;
use flux_core::events::EventRegistry;
use flux_core::jobs::{JobHandle, JobStatus, JobSystem};
use flux_core::logging::FrameTelemetry;
use flux_core::memory::FrameArena;
use flux_core::time::{SimTick, TimeContext};

use crate::bootstrap::EngineConfig;
use crate::context::{EngineContext, EngineServices};
use crate::overlay::OverlayData;
use crate::plugins::Plugin;
use crate::settings::UserSettings;

/// Ёмкость frame-арены по умолчанию: 4 МиБ на кадр.
const FRAME_ARENA_CAPACITY: usize = 4 * 1024 * 1024;

/// Окно телеметрии: 4 секунды при 60 Гц.
const TELEMETRY_WINDOW: usize = 240;

/// Результат шага симуляции.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TickResult {
    /// Сколько фиксированных шагов выполнено.
    pub steps: u32,
    /// Текущий тик симуляции.
    pub sim_tick: SimTick,
    /// Доля до следующего шага для интерполяции рендера.
    pub alpha: f32,
    /// Шаги, отброшенные анти-spiral ограничителем.
    pub dropped_steps: u64,
}

/// Статистика кадра.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameStats {
    /// Номер кадра.
    pub frame_index: u64,
    /// Длительность кадра в миллисекундах.
    pub frame_ms: f32,
    /// Средний FPS по окну телеметрии.
    pub fps: f32,
    /// Медиана frametime.
    pub p50_ms: f32,
    /// 99-й перцентиль frametime.
    pub p99_ms: f32,
    /// Задач, поставленных плагинами за кадр.
    pub jobs: usize,
    /// Задач, завершившихся паникой.
    pub panicked_jobs: usize,
}

/// Итог прогона фиксированного числа кадров.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunSummary {
    /// Отыгранные кадры.
    pub frames: u64,
    /// Выполненные фиксированные шаги.
    pub ticks: u64,
    /// Задачи с паникой за прогон.
    pub panicked_jobs: usize,
}

/// Движок: владеет подсистемами и плагинами.
pub struct Engine {
    config: EngineConfig,
    time: TimeContext,
    jobs: Arc<JobSystem>,
    events: Arc<EventRegistry>,
    settings: UserSettings,
    plugins: Vec<Box<dyn Plugin>>,
    telemetry: FrameTelemetry,
    arena: FrameArena,
    frame_jobs: Vec<JobHandle>,
    renderer: Option<Arc<dyn Renderer>>,
    platform: Option<Arc<dyn Platform>>,
    audio: Option<Arc<dyn AudioDevice>>,
    net: Option<Arc<dyn NetBackend>>,
    script: Option<Arc<dyn ScriptVM>>,
    frame_index: u64,
    last_frame: FrameStats,
    entity_count: Option<usize>,
    running: bool,
}

impl Engine {
    pub(crate) fn new(
        config: EngineConfig,
        jobs: Arc<JobSystem>,
        plugins: Vec<Box<dyn Plugin>>,
    ) -> Self {
        let time = TimeContext::new(config.fixed_step, config.max_steps_per_frame);
        Self {
            settings: config.settings.clone(),
            config,
            time,
            jobs,
            events: Arc::new(EventRegistry::new()),
            plugins,
            telemetry: FrameTelemetry::new(TELEMETRY_WINDOW),
            arena: FrameArena::new(FRAME_ARENA_CAPACITY),
            frame_jobs: Vec::new(),
            renderer: None,
            platform: None,
            audio: None,
            net: None,
            script: None,
            frame_index: 0,
            last_frame: FrameStats::default(),
            entity_count: None,
            running: true,
        }
    }

    /// Вызывает `Plugin::build` для всех плагинов.
    pub(crate) fn build_plugins(&mut self) {
        for plugin in &mut self.plugins {
            breadcrumbs().push(format!("plugin build: {}", plugin.name()));
            let services = EngineServices {
                time: &self.time,
                events: self.events.as_ref(),
                jobs: self.jobs.as_ref(),
                settings: &self.settings,
                renderer: self.renderer.as_deref(),
                platform: self.platform.as_deref(),
                audio: self.audio.as_deref(),
                net: self.net.as_deref(),
                script: self.script.as_deref(),
            };
            let mut ctx = EngineContext::new(services, &mut self.arena, &mut self.frame_jobs);
            plugin.build(&mut ctx);
        }
    }

    /// Шаг симуляции: обновляет время и выполняет 0..N фиксированных шагов.
    ///
    /// Пауза — отсутствие ненулевых шагов: `TimeContext` возвращает `steps = 0`.
    #[must_use]
    pub fn tick(&mut self) -> TickResult {
        let update = self.time.update();

        for step in 0..update.steps {
            breadcrumbs().push(format!("tick {} step {}", update.sim_tick.get(), step));
            for plugin in &mut self.plugins {
                let services = EngineServices {
                    time: &self.time,
                    events: self.events.as_ref(),
                    jobs: self.jobs.as_ref(),
                    settings: &self.settings,
                    renderer: self.renderer.as_deref(),
                    platform: self.platform.as_deref(),
                    audio: self.audio.as_deref(),
                    net: self.net.as_deref(),
                    script: self.script.as_deref(),
                };
                let mut ctx = EngineContext::new(services, &mut self.arena, &mut self.frame_jobs);
                plugin.update_fixed(&mut ctx, step);
            }
        }

        TickResult {
            steps: update.steps,
            sim_tick: update.sim_tick,
            alpha: update.alpha,
            dropped_steps: self.time.fixed().dropped_steps(),
        }
    }

    /// Кадр: сброс арены → публикация событий → переменные системы →
    /// fence job-ов → телеметрия.
    #[must_use]
    pub fn frame(&mut self) -> FrameStats {
        let started = Instant::now();
        self.arena.reset();
        self.events.update();

        for plugin in &mut self.plugins {
            let services = EngineServices {
                time: &self.time,
                events: self.events.as_ref(),
                jobs: self.jobs.as_ref(),
                settings: &self.settings,
                renderer: self.renderer.as_deref(),
                platform: self.platform.as_deref(),
                audio: self.audio.as_deref(),
                net: self.net.as_deref(),
                script: self.script.as_deref(),
            };
            let mut ctx = EngineContext::new(services, &mut self.arena, &mut self.frame_jobs);
            plugin.update_variable(&mut ctx);
        }

        let jobs = self.frame_jobs.len();
        let mut panicked_jobs = 0;
        for handle in self.frame_jobs.drain(..) {
            if handle.wait() == JobStatus::Panicked {
                panicked_jobs += 1;
                breadcrumbs().push("job panicked (frame fence)");
            }
        }

        let frame_ms = started.elapsed().as_secs_f32() * 1_000.0;
        self.telemetry.push(frame_ms);
        self.frame_index = self.frame_index.saturating_add(1);

        let stats = FrameStats {
            frame_index: self.frame_index,
            frame_ms,
            fps: self.telemetry.fps(),
            p50_ms: self.telemetry.p50(),
            p99_ms: self.telemetry.p99(),
            jobs,
            panicked_jobs,
        };
        self.last_frame = stats;
        stats
    }

    /// Ожидание GPU. Отдельный шаг: редактор в edit-режиме может пропускать.
    pub fn acquire(&mut self) {
        // Стаб этапа 1: реальный fence появится вместе с RHI (этап 4).
    }

    /// Прогоняет цикл до `request_shutdown`, затем штатно завершает процесс.
    pub fn run_loop(mut self) -> ! {
        while self.running {
            let _ = self.tick();
            let stats = self.frame();
            self.acquire();

            if self.config.throttle {
                let budget_ms = self.config.fixed_step.as_secs_f32() * 1_000.0;
                let remaining_ms = budget_ms - stats.frame_ms;
                if remaining_ms > 0.0 {
                    std::thread::sleep(Duration::from_secs_f32(remaining_ms / 1_000.0));
                }
            }
        }

        self.shutdown();
        std::process::exit(0);
    }

    /// Прогоняет ровно `frames` кадров без throttle (headless, тесты, soak).
    pub fn run_frames(mut self, frames: u64) -> RunSummary {
        let mut ticks = 0_u64;
        let mut panicked_jobs = 0_usize;

        for _ in 0..frames {
            ticks = ticks.saturating_add(u64::from(self.tick().steps));
            panicked_jobs += self.frame().panicked_jobs;
            self.acquire();
        }

        self.shutdown();
        RunSummary {
            frames,
            ticks,
            panicked_jobs,
        }
    }

    /// Останавливает цикл `run_loop` после текущего кадра.
    pub fn request_shutdown(&mut self) {
        self.running = false;
    }

    /// Признак работы цикла.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Штатная остановка: `Plugin::shutdown` и очистка очередей событий.
    pub fn shutdown(&mut self) {
        for plugin in &mut self.plugins {
            breadcrumbs().push(format!("plugin shutdown: {}", plugin.name()));
            plugin.shutdown();
        }
        self.events.clear_all();
        self.running = false;
    }

    /// Конфигурация движка.
    #[must_use]
    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Контекст времени (пауза, масштаб, тик).
    #[must_use]
    pub fn time(&self) -> &TimeContext {
        &self.time
    }

    /// Мутабельный контекст времени: пауза и time scale из UI/скриптов.
    pub fn time_mut(&mut self) -> &mut TimeContext {
        &mut self.time
    }

    /// Пользовательские настройки.
    #[must_use]
    pub fn settings(&self) -> &UserSettings {
        &self.settings
    }

    /// Мутабельные настройки: применяются к подсистемам на следующем кадре.
    pub fn settings_mut(&mut self) -> &mut UserSettings {
        &mut self.settings
    }

    /// Job system.
    #[must_use]
    pub fn jobs(&self) -> &JobSystem {
        self.jobs.as_ref()
    }

    /// Реестр событий.
    #[must_use]
    pub fn events(&self) -> &EventRegistry {
        self.events.as_ref()
    }

    /// Номер последнего кадра.
    #[must_use]
    pub fn frame_index(&self) -> u64 {
        self.frame_index
    }

    /// Снимок dev-overlay: тик, кадр, воркеры и (опционально) число сущностей.
    ///
    /// Метод read-only: не меняет время, события и порядок систем.
    #[must_use]
    pub fn overlay(&self) -> OverlayData {
        OverlayData {
            sim_tick: self.time.sim_tick(),
            frame: self.last_frame,
            worker_threads: self.jobs.worker_count(),
            entities: self.entity_count,
        }
    }

    /// Сообщает движку число сущностей для overlay (игра/плагин).
    pub fn set_entity_count(&mut self, count: Option<usize>) {
        self.entity_count = count;
    }
}
