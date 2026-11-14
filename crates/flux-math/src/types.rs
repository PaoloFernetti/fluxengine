//! Публичный математический API поверх glam.
//!
//! Контракт: без аллокаций, детерминизм операций, выравнивание для SIMD.
//! `Vec3` (align 4) — для полей структур; `Vec3A` (align 16) — для SoA и SIMD.

pub use glam::{DMat4, DQuat, DVec2, DVec3, DVec4, Mat3, Mat4, Quat, Vec2, Vec3, Vec3A, Vec4};

/// Допуск сравнения позиций в sim-пространстве (метры).
pub const SIM_EPSILON: f32 = 1.0e-4;

/// Допуск сравнения в каноне (нанометры).
pub const CANON_EPSILON_NM: i64 = 1;

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, size_of};

    #[test]
    fn vec3_and_vec3a_alignment_contract() {
        assert_eq!(align_of::<Vec3>(), 4);
        assert_eq!(size_of::<Vec3>(), 12);
        assert_eq!(align_of::<Vec3A>(), 16);
        assert_eq!(size_of::<Vec3A>(), 16);
    }

    #[test]
    fn f64_types_are_available() {
        let p = DVec3::new(1.0, 2.0, 3.0);
        let q = DQuat::IDENTITY;
        assert_eq!(p.x, 1.0);
        assert_eq!(q.w, 1.0);
    }
}
