//! Физика f64, collision, KCC.
//!
//! Этап 3 — collision-минимум и KCC-минимум (ADR-0032, ADR-0024):
//! тайловый grid, свипы капсулы/луча и чистая функция `move_character`.
//! Полный solver — этап 6, контракт запросов при этом сохраняется.
//!
//! Канон: `docs/canon.md`. План: `docs/architecture.md`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod collision;
pub mod kcc;

pub use collision::{Capsule, Contact, Ray, Tile, TileGrid};
pub use kcc::{move_character, CharacterConfig, CharacterState, MoveResult};

pub use flux_core::flux_math::{DVec2, DVec3};
