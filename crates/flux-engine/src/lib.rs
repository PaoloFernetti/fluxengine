//! FluxEngine: фасад, точка сборки и главный цикл кадра.
//!
//! Крейт знает конкретные подсистемы; подсистемы видят только trait-контракты
//! из [`flux_core::contracts`]. Канон: `docs/canon.md`, план:
//! `docs/architecture.md`, раздел «Фасад и точка сборки».
//!
//! Контракт цикла: циклом владеет вызывающий. `Engine` экспонирует шаги
//! [`Engine::tick`], [`Engine::frame`], [`Engine::acquire`]; `EngineBuilder::run`
//! — тонкая обёртка для игр без собственного event loop.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod bootstrap;
pub mod context;
pub mod determinism;
pub mod overlay;
pub mod plugins;
pub mod render_extract;
pub mod runtime;
pub mod settings;
pub mod simulation;

pub use bootstrap::{EngineBuilder, EngineConfig, EngineError, GfxBackend};
pub use context::EngineContext;
pub use determinism::{DeterminismHarness, StateDiff, StateHash};
pub use overlay::OverlayData;
pub use plugins::Plugin;
pub use render_extract::{ExtractStats, MeshRenderer, RenderExtractor, RenderTransform};
pub use runtime::{Engine, FrameStats, RunSummary, TickResult};
pub use settings::{GraphicsQuality, UserSettings};
pub use simulation::{
    controller_post_system, controller_system, install_controller_schedule, CharacterBody,
    PhysicsStep, CONTROLLER_POST_SYSTEM, CONTROLLER_SYSTEM,
};

/// Контракты подсистем — единственный канал доступа плагинов к бэкендам.
pub use flux_core::contracts;
/// Базовые сервисы ядра, доступные плагинам через контекст.
pub use flux_core::{events, jobs, time};
