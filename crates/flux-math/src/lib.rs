//! Математика FluxEngine: типы, канон позиций, sim-пространство, формы.
//!
//! Канон: `docs/canon.md` (`WorldPos`, `SimRegion`, `Origin rebasing`).
//! План: `docs/architecture.md`, раздел «Математика и системы координат».
//!
//! Три пространства координат:
//! 1. [`WorldPos`] — канон, `i64` в нанометрах (сейвы, сеть, манифесты).
//! 2. [`SimRegion`] — `f32` floating origin для симуляции.
//! 3. Camera-relative `f32` — вычисляется рендером (этап 4).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod shapes;
pub mod sim_region;
pub mod types;
pub mod world_pos;

pub use shapes::{Aabb, Bvh, Frustum, Obb, Sphere};
pub use sim_region::{SimRegion, DEFAULT_MAX_SIM_RADIUS_M};
pub use types::*;
pub use world_pos::{PositionError, WorldPos, NANOMETERS_PER_METER};
