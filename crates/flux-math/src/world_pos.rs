//! Каноническая позиция мира: `i64` в нанометрах.
//!
//! Единственный источник истины для позиции. Используется в сейвах, сети и
//! манифестах сцены; конверсии в `f32`/`f64` выполняются только на границах.

use glam::{DVec3, Vec3};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Нанометров в метре.
pub const NANOMETERS_PER_METER: i64 = 1_000_000_000;

/// Максимальная представимая координата в метрах.
pub const MAX_METERS: f64 = i64::MAX as f64 / NANOMETERS_PER_METER as f64;

/// Ошибка конверсии координаты.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum PositionError {
    /// Координата не является конечным числом.
    #[error("coordinate is not finite")]
    NotFinite,
    /// Координата выходит за диапазон `i64` нанометров.
    #[error("coordinate {0} m is out of canonical range")]
    OutOfRange(String),
}

/// Каноническая позиция мира в нанометрах.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorldPos {
    /// Смещение по X, нанометры.
    pub x: i64,
    /// Смещение по Y, нанометры.
    pub y: i64,
    /// Смещение по Z, нанометры.
    pub z: i64,
}

impl WorldPos {
    /// Начало координат.
    pub const ORIGIN: Self = Self { x: 0, y: 0, z: 0 };

    /// Создаёт позицию из нанометров.
    #[must_use]
    pub const fn from_nanometers(x: i64, y: i64, z: i64) -> Self {
        Self { x, y, z }
    }

    /// Создаёт позицию из метров с проверкой диапазона.
    ///
    /// # Errors
    ///
    /// Возвращает [`PositionError`], если координата не конечна или не
    /// помещается в `i64` нанометров.
    pub fn from_meters(x: f64, y: f64, z: f64) -> Result<Self, PositionError> {
        Ok(Self {
            x: meters_to_nm(x)?,
            y: meters_to_nm(y)?,
            z: meters_to_nm(z)?,
        })
    }

    /// Создаёт позицию из метров, насыщая на границах диапазона.
    #[must_use]
    pub fn saturating_from_meters(x: f64, y: f64, z: f64) -> Self {
        Self {
            x: saturating_meters_to_nm(x),
            y: saturating_meters_to_nm(y),
            z: saturating_meters_to_nm(z),
        }
    }

    /// Позиция в метрах как `f64`.
    #[must_use]
    pub fn to_meters(self) -> DVec3 {
        DVec3::new(
            self.x as f64 / NANOMETERS_PER_METER as f64,
            self.y as f64 / NANOMETERS_PER_METER as f64,
            self.z as f64 / NANOMETERS_PER_METER as f64,
        )
    }

    /// Вектор от `self` к `other` в метрах (`f64`).
    #[must_use]
    pub fn delta_meters(self, other: Self) -> DVec3 {
        DVec3::new(
            (other.x - self.x) as f64 / NANOMETERS_PER_METER as f64,
            (other.y - self.y) as f64 / NANOMETERS_PER_METER as f64,
            (other.z - self.z) as f64 / NANOMETERS_PER_METER as f64,
        )
    }

    /// Сложение с проверкой переполнения.
    #[must_use]
    pub fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            x: self.x.checked_add(other.x)?,
            y: self.y.checked_add(other.y)?,
            z: self.z.checked_add(other.z)?,
        })
    }

    /// Позиция в sim-пространстве относительно `origin`.
    #[must_use]
    pub fn to_sim_f32(self, origin: Self) -> Vec3 {
        let delta = origin.delta_meters(self);
        Vec3::new(delta.x as f32, delta.y as f32, delta.z as f32)
    }
}

/// Конверсия метров в нанометры с проверкой.
pub(crate) fn meters_to_nm(value: f64) -> Result<i64, PositionError> {
    if !value.is_finite() {
        return Err(PositionError::NotFinite);
    }
    let nm = value * NANOMETERS_PER_METER as f64;
    if nm < i64::MIN as f64 || nm > i64::MAX as f64 {
        return Err(PositionError::OutOfRange(format!("{value}")));
    }
    Ok(nm.round() as i64)
}

fn saturating_meters_to_nm(value: f64) -> i64 {
    if value.is_nan() {
        return 0;
    }
    let nm = value * NANOMETERS_PER_METER as f64;
    if nm <= i64::MIN as f64 {
        i64::MIN
    } else if nm >= i64::MAX as f64 {
        i64::MAX
    } else {
        nm.round() as i64
    }
}

/// Конверсия sim-координаты (`f32`, метры) в канон относительно `origin`.
#[must_use]
pub fn sim_f32_to_world(origin: WorldPos, sim: Vec3) -> WorldPos {
    let delta = WorldPos::saturating_from_meters(sim.x as f64, sim.y as f64, sim.z as f64);
    origin.checked_add(delta).unwrap_or(WorldPos {
        x: origin.x.saturating_add(delta.x),
        y: origin.y.saturating_add(delta.y),
        z: origin.z.saturating_add(delta.z),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_meters_nanometers() {
        let pos = WorldPos::from_meters(1.5, -2.25, 12345.678).unwrap();
        assert_eq!(pos.x, 1_500_000_000);
        assert_eq!(pos.y, -2_250_000_000);
        assert_eq!(pos.z, 12_345_678_000_000);

        let meters = pos.to_meters();
        assert!((meters.x - 1.5).abs() < 1e-9);
        assert!((meters.y + 2.25).abs() < 1e-9);
    }

    #[test]
    fn rejects_non_finite_and_out_of_range() {
        assert_eq!(
            WorldPos::from_meters(f64::NAN, 0.0, 0.0),
            Err(PositionError::NotFinite)
        );
        assert!(matches!(
            WorldPos::from_meters(1e30, 0.0, 0.0),
            Err(PositionError::OutOfRange(_))
        ));
    }

    #[test]
    fn delta_is_exact_in_nanometers() {
        let a = WorldPos::from_nanometers(10, 20, 30);
        let b = WorldPos::from_nanometers(1_000_000_010, 20, 30);
        let delta = a.delta_meters(b);
        assert!((delta.x - 1.0).abs() < 1e-12);
        assert_eq!(delta.y, 0.0);
    }

    #[test]
    fn sim_conversion_roundtrip_is_within_tolerance() {
        let origin = WorldPos::from_meters(1000.0, -500.0, 2000.0).unwrap();
        let world = WorldPos::from_meters(1001.5, -499.25, 2003.0).unwrap();
        let sim = world.to_sim_f32(origin);
        let back = sim_f32_to_world(origin, sim);
        assert!((back.x - world.x).abs() <= 200);
        assert!((back.y - world.y).abs() <= 200);
        assert!((back.z - world.z).abs() <= 200);
    }

    #[test]
    fn saturating_conversion_never_panics() {
        let pos = WorldPos::saturating_from_meters(f64::INFINITY, f64::NEG_INFINITY, f64::NAN);
        assert_eq!(pos.x, i64::MAX);
        assert_eq!(pos.y, i64::MIN);
        assert_eq!(pos.z, 0);
    }
}
