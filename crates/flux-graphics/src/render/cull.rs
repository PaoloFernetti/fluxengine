//! CPU-culling для extract (4.5): camera-relative AABB против фрустума.
//!
//! Отсечение детерминировано: инстансы обходятся по возрастанию индекса,
//! результат — `Vec<u32>` с индексами видимых. Модуль не зависит от RHI и
//! работает на `flux_math`; GPU-пайплайн геометрии использует его вывод как
//! instance-список.

use flux_math::{Aabb, Frustum, Vec3};

/// Единичный bounding box инстанса по умолчанию (куб со стороной 1).
#[must_use]
pub fn unit_bounds() -> Aabb {
    Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(0.5))
}

/// Итог отсечения.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CullStats {
    /// Проверено инстансов.
    pub tested: usize,
    /// Прошло фрустум.
    pub visible: usize,
}

impl CullStats {
    /// Отсечено инстансов.
    #[must_use]
    pub const fn culled(self) -> usize {
        self.tested.saturating_sub(self.visible)
    }

    /// Пустой ли результат.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.tested == 0
    }
}

/// Итог extract сцены: интерполяция позиций + culling.
///
/// Не путать с `flux_engine::ExtractStats` (число сущностей ECS).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SceneExtractStats {
    /// Инстансов в сцене.
    pub instances: usize,
    /// Видимых после culling.
    pub visible: usize,
    /// Отсечённых.
    pub culled: usize,
}

impl SceneExtractStats {
    /// Собирает статистику из результата culling.
    #[must_use]
    pub const fn from_cull(instances: usize, stats: CullStats) -> Self {
        Self {
            instances,
            visible: stats.visible,
            culled: stats.culled(),
        }
    }

    /// Все ли инстансы видимы (или сцена пуста).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.instances == 0
    }
}

/// Отсекает инстансы по camera-relative AABB.
///
/// `positions` — camera-relative позиции (после интерполяции), `bounds` —
/// локальные границы инстанса. Если границ меньше, чем позиций, недостающие
/// считаются единичным кубом. `out` очищается и заполняется индексами
/// видимых инстансов по возрастанию.
#[must_use]
pub fn cull_instances(
    positions: &[Vec3],
    bounds: &[Aabb],
    frustum: &Frustum,
    out: &mut Vec<u32>,
) -> CullStats {
    out.clear();
    let fallback = unit_bounds();
    for (index, position) in positions.iter().enumerate() {
        let local = bounds.get(index).copied().unwrap_or(fallback);
        let world = Aabb::new(local.min + *position, local.max + *position);
        if frustum.intersects_aabb(world) {
            out.push(index as u32);
        }
    }
    CullStats {
        tested: positions.len(),
        visible: out.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Camera;

    fn frustum() -> Frustum {
        // Камера в начале координат смотрит в -Z (Reversed-Z).
        Frustum::from_view_projection(Camera::default().view_projection())
    }

    #[test]
    fn unit_bounds_are_centered() {
        let bounds = unit_bounds();
        assert_eq!(bounds.min, Vec3::splat(-0.5));
        assert_eq!(bounds.max, Vec3::splat(0.5));
    }

    #[test]
    fn culling_keeps_only_visible_instances() {
        let positions = [
            Vec3::new(0.0, 0.0, -5.0),   // перед камерой
            Vec3::new(0.0, 0.0, 5.0),    // за камерой
            Vec3::new(500.0, 0.0, -5.0), // вне конуса
            Vec3::new(0.0, 500.0, -5.0), // вне конуса по вертикали
        ];
        let mut visible = Vec::new();
        let stats = cull_instances(&positions, &[], &frustum(), &mut visible);
        assert_eq!(stats.tested, 4);
        assert_eq!(stats.visible, 1);
        assert_eq!(stats.culled(), 3);
        assert_eq!(visible, vec![0]);
    }

    #[test]
    fn bounds_extend_visibility() {
        let positions = [Vec3::new(0.0, 0.0, 200.0)];
        let small = [unit_bounds()];
        let huge = [Aabb::from_center_half_extents(
            Vec3::ZERO,
            Vec3::splat(500.0),
        )];
        let mut visible = Vec::new();
        let _ = cull_instances(&positions, &small, &frustum(), &mut visible);
        assert!(visible.is_empty());
        let _ = cull_instances(&positions, &huge, &frustum(), &mut visible);
        assert_eq!(visible, vec![0]);
    }

    #[test]
    fn result_is_sorted_and_reuses_buffer() {
        let positions: Vec<Vec3> = (0..8)
            .map(|index| Vec3::new(0.0, 0.0, -1.0 - index as f32))
            .collect();
        let mut visible = vec![99, 98, 97];
        let stats = cull_instances(&positions, &[], &frustum(), &mut visible);
        assert_eq!(stats.visible, 8);
        assert_eq!(visible, (0..8).collect::<Vec<u32>>());
    }

    #[test]
    fn empty_scene_is_empty() {
        let mut visible = vec![1];
        let stats = cull_instances(&[], &[], &frustum(), &mut visible);
        assert!(stats.is_empty());
        assert_eq!(stats.visible, 0);
        assert!(visible.is_empty());
    }
}
