//! Архетипная ECS FluxEngine.
//!
//! Хранение компонентов по архетипам (SoA-чанки), запросы с change detection,
//! детерминированный планировщик, SchemaRegistry и SaveState-дельты.
//!
//! Канон: `docs/canon.md`. План: `docs/architecture.md`, «Собственная
//! архетипная ECS». Итерации: `plans/stage-2-execution.md` (I0–I5).
//!
//! Инварианты (проверяются тестами, не комментариями):
//! - `generation` не реюзится для слота;
//! - `entity_uuid` стабилен при миграциях;
//! - порядок компонентов/систем/полей — детерминированный (UUID, имя);
//! - кэш запросов инвалидируется только при создании/удалении архетипа.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

// Дерайв `Reflect` ссылается на `::flux_ecs`; внутри самого крейта это имя
// должно резолвиться (генерируемый код одинаков для пользователей и тестов).
extern crate self as flux_ecs;

pub mod archetypes;
pub mod change;
pub mod commands;
pub mod component;
pub mod entity;
pub mod events;
pub mod query;
pub mod resources;
pub mod save;
pub mod scheduler;
pub mod schema;
pub mod sim_time;
pub mod world;
