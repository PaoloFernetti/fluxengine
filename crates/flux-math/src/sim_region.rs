//! Sim-пространство: `f32` floating origin для детерминированной симуляции.
//!
//! Позиции в ECS хранятся относительно [`SimRegion::origin`]. Rebase
//! выполняется только на границах тиков и не влияет на канон [`WorldPos`].

use crate::world_pos::sim_f32_to_world;
use crate::WorldPos;
use glam::Vec3;

/// Радиус sim-региона по умолчанию (метры).
pub const DEFAULT_MAX_SIM_RADIUS_M: f64 = 8_192.0;

/// Текущее начало sim-пространства.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimRegion {
    origin: WorldPos,
}

impl SimRegion {
    /// Создаёт регион с заданным началом.
    #[must_use]
    pub const fn new(origin: WorldPos) -> Self {
        Self { origin }
    }

    /// Текущее начало в каноне.
    #[must_use]
    pub const fn origin(&self) -> WorldPos {
        self.origin
    }

    /// Переносит начало и возвращает прежнее значение.
    pub fn rebase(&mut self, origin: WorldPos) -> WorldPos {
        core::mem::replace(&mut self.origin, origin)
    }

    /// Канон → sim (`f32`, метры относительно начала).
    #[must_use]
    pub fn to_sim(&self, world: WorldPos) -> Vec3 {
        world.to_sim_f32(self.origin)
    }

    /// Sim → канон (с насыщением на границах диапазона).
    #[must_use]
    pub fn to_world(&self, sim: Vec3) -> WorldPos {
        sim_f32_to_world(self.origin, sim)
    }

    /// Нужен ли rebase: позиция дальше `radius_m` от начала.
    #[must_use]
    pub fn needs_rebase(&self, world: WorldPos, radius_m: f64) -> bool {
        let delta = self.origin.delta_meters(world);
        delta.length() > radius_m
    }

    /// Расстояние от начала до позиции (метры).
    #[must_use]
    pub fn distance_m(&self, world: WorldPos) -> f64 {
        self.origin.delta_meters(world).length()
    }
}

impl Default for SimRegion {
    fn default() -> Self {
        Self::new(WorldPos::ORIGIN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebase_returns_previous_origin() {
        let mut region = SimRegion::new(WorldPos::from_meters(1.0, 0.0, 0.0).unwrap());
        let previous = region.rebase(WorldPos::from_meters(2.0, 0.0, 0.0).unwrap());
        assert_eq!(previous, WorldPos::from_meters(1.0, 0.0, 0.0).unwrap());
        assert_eq!(
            region.origin(),
            WorldPos::from_meters(2.0, 0.0, 0.0).unwrap()
        );
    }

    #[test]
    fn to_sim_is_relative_to_origin() {
        let region = SimRegion::new(WorldPos::from_meters(1000.0, 0.0, 0.0).unwrap());
        let sim = region.to_sim(WorldPos::from_meters(1002.5, 0.0, 0.0).unwrap());
        assert!((sim.x - 2.5).abs() < 1e-4);
        assert_eq!(sim.y, 0.0);
    }

    #[test]
    fn ten_thousand_rebases_do_not_accumulate_drift() {
        let world = WorldPos::from_meters(1.0, 2.0, 3.0).unwrap();
        let mut region = SimRegion::default();
        let mut max_error_m = 0.0_f64;

        for step in 0..10_000_i64 {
            let origin = WorldPos::from_meters(step as f64 * 10.0, 0.0, 0.0).unwrap();
            region.rebase(origin);

            let sim = region.to_sim(world);
            let back = region.to_world(sim);
            let error = back.delta_meters(world).length();
            max_error_m = max_error_m.max(error);
        }

        assert!(max_error_m < 0.01, "drift too large: {max_error_m} m");
    }

    #[test]
    fn needs_rebase_uses_radius() {
        let region = SimRegion::new(WorldPos::from_meters(0.0, 0.0, 0.0).unwrap());
        let far = WorldPos::from_meters(9000.0, 0.0, 0.0).unwrap();
        assert!(region.needs_rebase(far, DEFAULT_MAX_SIM_RADIUS_M));
        assert!(!region.needs_rebase(
            WorldPos::from_meters(100.0, 0.0, 0.0).unwrap(),
            DEFAULT_MAX_SIM_RADIUS_M
        ));
    }
}
