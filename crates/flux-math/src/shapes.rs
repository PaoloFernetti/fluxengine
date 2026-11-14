//! Геометрические примитивы для culling, picking и физики.
//!
//! Все типы работают в sim-пространстве (`f32`); канон [`crate::WorldPos`]
//! конвертируется на границах.

use glam::{Mat4, Quat, Vec3, Vec4};

/// Осевой bounding box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    /// Минимальный угол.
    pub min: Vec3,
    /// Максимальный угол.
    pub max: Vec3,
}

impl Aabb {
    /// Создаёт AABB, нормализуя порядок углов.
    #[must_use]
    pub fn new(a: Vec3, b: Vec3) -> Self {
        Self {
            min: a.min(b),
            max: a.max(b),
        }
    }

    /// AABB по центру и половине размеров.
    #[must_use]
    pub fn from_center_half_extents(center: Vec3, half_extents: Vec3) -> Self {
        Self {
            min: center - half_extents,
            max: center + half_extents,
        }
    }

    /// Центр.
    #[must_use]
    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Половина размеров.
    #[must_use]
    pub fn half_extents(self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    /// Содержит ли точку.
    #[must_use]
    pub fn contains(self, point: Vec3) -> bool {
        point.cmpge(self.min).all() && point.cmple(self.max).all()
    }

    /// Пересекается ли с другим AABB.
    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        self.min.cmple(other.max).all() && self.max.cmpge(other.min).all()
    }

    /// Объединение.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    /// Расширение на `amount` по всем осям.
    pub fn expand(&mut self, amount: f32) -> &mut Self {
        self.min -= Vec3::splat(amount);
        self.max += Vec3::splat(amount);
        self
    }

    /// Пустой ли AABB (вырожденный).
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.min.cmpgt(self.max).any()
    }
}

/// Сфера.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sphere {
    /// Центр.
    pub center: Vec3,
    /// Радиус.
    pub radius: f32,
}

impl Sphere {
    /// Пересечение двух сфер.
    #[must_use]
    pub fn intersects_sphere(self, other: Self) -> bool {
        let radius = self.radius + other.radius;
        self.center.distance_squared(other.center) <= radius * radius
    }

    /// Пересечение с AABB.
    #[must_use]
    pub fn intersects_aabb(self, aabb: Aabb) -> bool {
        let closest = self.center.clamp(aabb.min, aabb.max);
        self.center.distance_squared(closest) <= self.radius * self.radius
    }
}

/// Ориентированный bounding box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obb {
    /// Центр.
    pub center: Vec3,
    /// Половина размеров в локальных осях.
    pub half_extents: Vec3,
    /// Ориентация.
    pub rotation: Quat,
}

impl Obb {
    /// Содержит ли точку.
    #[must_use]
    pub fn contains(self, point: Vec3) -> bool {
        let local = self.rotation.inverse() * (point - self.center);
        local.abs().cmple(self.half_extents).all()
    }

    /// Приблизительный AABB по восьми углам.
    #[must_use]
    pub fn aabb(self) -> Aabb {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for sx in [-1.0_f32, 1.0] {
            for sy in [-1.0_f32, 1.0] {
                for sz in [-1.0_f32, 1.0] {
                    let corner =
                        self.center + self.rotation * (self.half_extents * Vec3::new(sx, sy, sz));
                    min = min.min(corner);
                    max = max.max(corner);
                }
            }
        }
        Aabb { min, max }
    }
}

/// Фрустум из шести плоскостей (`xyz` — нормаль, `w` — смещение).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frustum {
    /// Плоскости: left, right, bottom, top, near, far.
    pub planes: [Vec4; 6],
}

impl Frustum {
    /// Строит фрустум из матрицы view-projection.
    #[must_use]
    pub fn from_view_projection(matrix: Mat4) -> Self {
        let r0 = matrix.row(0);
        let r1 = matrix.row(1);
        let r2 = matrix.row(2);
        let r3 = matrix.row(3);

        let mut planes = [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r3 + r2, r3 - r2];

        for plane in &mut planes {
            let length = plane.truncate().length();
            if length > f32::EPSILON {
                *plane /= length;
            }
        }

        Self { planes }
    }

    /// Пересекается ли AABB с фрустумом.
    #[must_use]
    pub fn intersects_aabb(&self, aabb: Aabb) -> bool {
        for plane in &self.planes {
            let normal = plane.truncate();
            let positive = Vec3::new(
                if normal.x >= 0.0 {
                    aabb.max.x
                } else {
                    aabb.min.x
                },
                if normal.y >= 0.0 {
                    aabb.max.y
                } else {
                    aabb.min.y
                },
                if normal.z >= 0.0 {
                    aabb.max.z
                } else {
                    aabb.min.z
                },
            );
            if normal.dot(positive) + plane.w < 0.0 {
                return false;
            }
        }
        true
    }
}

/// Заготовка BVH для picking и culling (этап 7).
#[derive(Clone, Debug, Default)]
pub struct Bvh {
    nodes: Vec<BvhNode>,
}

/// Узел BVH.
#[derive(Clone, Copy, Debug)]
pub struct BvhNode {
    /// Границы узла.
    pub bounds: Aabb,
}

impl Bvh {
    /// Строит BVH по AABB. TODO(этап 7): реальное построение.
    #[must_use]
    pub fn build(_items: &[Aabb]) -> Self {
        Self { nodes: Vec::new() }
    }

    /// Пуст ли BVH.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_intersection_and_union() {
        let a = Aabb::new(Vec3::ZERO, Vec3::splat(1.0));
        let b = Aabb::new(Vec3::splat(0.5), Vec3::splat(2.0));
        let c = Aabb::new(Vec3::splat(3.0), Vec3::splat(4.0));

        assert!(a.intersects(b));
        assert!(!a.intersects(c));

        let union = a.union(c);
        assert_eq!(union.min, Vec3::ZERO);
        assert_eq!(union.max, Vec3::splat(4.0));
    }

    #[test]
    fn aabb_contains_and_expand() {
        let mut a = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(1.0));
        assert!(a.contains(Vec3::new(0.5, -0.5, 1.0)));
        assert!(!a.contains(Vec3::new(2.0, 0.0, 0.0)));

        a.expand(1.0);
        assert!(a.contains(Vec3::new(1.5, 0.0, 0.0)));
    }

    #[test]
    fn sphere_intersections() {
        let a = Sphere {
            center: Vec3::ZERO,
            radius: 1.0,
        };
        let b = Sphere {
            center: Vec3::new(1.5, 0.0, 0.0),
            radius: 1.0,
        };
        assert!(a.intersects_sphere(b));
        assert!(a.intersects_aabb(Aabb::from_center_half_extents(
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::splat(0.25)
        )));
    }

    #[test]
    fn obb_contains_respects_rotation() {
        let obb = Obb {
            center: Vec3::ZERO,
            half_extents: Vec3::new(2.0, 0.5, 0.5),
            rotation: Quat::from_rotation_y(core::f32::consts::FRAC_PI_2),
        };
        assert!(obb.contains(Vec3::new(0.0, 0.0, 1.5)));
        assert!(!obb.contains(Vec3::new(1.5, 0.0, 0.0)));
    }

    #[test]
    fn frustum_identity_accepts_inside_and_rejects_outside() {
        let frustum = Frustum::from_view_projection(Mat4::IDENTITY);
        let inside = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(0.5));
        let outside = Aabb::from_center_half_extents(Vec3::new(3.0, 0.0, 0.0), Vec3::splat(0.5));
        assert!(frustum.intersects_aabb(inside));
        assert!(!frustum.intersects_aabb(outside));
    }

    #[test]
    fn bvh_stub_is_empty() {
        assert!(Bvh::build(&[]).is_empty());
    }
}
