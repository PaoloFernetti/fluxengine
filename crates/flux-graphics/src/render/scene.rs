//! RenderScene: SoA-снапшот кадра, alpha интерполяции и камера (I4).
//!
//! Сцена хранит текущие и предыдущие позиции, mesh/material id и
//! camera-relative позиции для рендера. Буферы переиспользуются: после
//! прогрева `extract` не аллоцирует. Reversed-Z: near → 1.0, far → 0.0
//! (ADR-0008).
//!
//! `extract` (4.5) — интерполяция позиций и culling по camera-relative AABB:
//! результат — индексы видимых инстансов, статистика `ExtractStats`.

use flux_math::{Aabb, Frustum, Mat4, Vec3, Vec4};

use super::cull::{cull_instances, unit_bounds, CullStats, SceneExtractStats};

/// Камера рендера.
///
/// Позиция задаётся в sim-пространстве (`f32`); рендер работает
/// camera-relative: `RenderScene` вычитает позицию камеры из трансформов,
/// а view-матрица ставит камеру в начало координат.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Позиция камеры (sim-пространство).
    pub position: Vec3,
    /// Направление взгляда.
    pub forward: Vec3,
    /// Верх камеры.
    pub up: Vec3,
    /// Вертикальный FOV, радианы.
    pub fov_y: f32,
    /// Соотношение сторон.
    pub aspect: f32,
    /// Ближняя плоскость.
    pub near: f32,
    /// Дальняя плоскость.
    pub far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            forward: -Vec3::Z,
            up: Vec3::Y,
            fov_y: 60.0_f32.to_radians(),
            aspect: 16.0 / 9.0,
            near: 0.1,
            far: 1000.0,
        }
    }
}

impl Camera {
    /// Камера с позицией, направлением и верхом; остальное — по умолчанию.
    #[must_use]
    pub fn new(position: Vec3, forward: Vec3, up: Vec3) -> Self {
        Self {
            position,
            forward,
            up,
            ..Self::default()
        }
    }

    /// Camera-relative позиция: мир минус позиция камеры.
    #[must_use]
    pub fn camera_relative(&self, world: Vec3) -> Vec3 {
        world - self.position
    }

    /// View-матрица: камера в начале координат (camera-relative).
    #[must_use]
    pub fn view_matrix(&self) -> Mat4 {
        let forward = if self.forward.length_squared() > 0.0 {
            self.forward.normalize()
        } else {
            -Vec3::Z
        };
        let up = if self.up.length_squared() > 0.0 {
            self.up.normalize()
        } else {
            Vec3::Y
        };
        Mat4::look_at_rh(Vec3::ZERO, forward, up)
    }

    /// Reversed-Z projection: near → 1.0, far → 0.0 (диапазон `0..1`).
    ///
    /// Некорректные параметры санитизируются детерминированно, чтобы камера
    /// не производила NaN.
    #[must_use]
    pub fn projection_reversed_z(&self) -> Mat4 {
        let near = self.near.max(1e-4);
        let far = self.far.max(near * 1.0001);
        let aspect = if self.aspect > 0.0 { self.aspect } else { 1.0 };
        let fov = self.fov_y.clamp(0.01, std::f32::consts::PI - 0.01);
        let f = 1.0 / (fov * 0.5).tan();
        let a = near / (far - near);
        let b = near * far / (far - near);
        Mat4::from_cols(
            Vec4::new(f / aspect, 0.0, 0.0, 0.0),
            Vec4::new(0.0, f, 0.0, 0.0),
            Vec4::new(0.0, 0.0, a, -1.0),
            Vec4::new(0.0, 0.0, b, 0.0),
        )
    }

    /// `projection * view`.
    #[must_use]
    pub fn view_projection(&self) -> Mat4 {
        self.projection_reversed_z() * self.view_matrix()
    }
}

/// SoA-снапшот кадра для рендера.
#[derive(Clone, Debug, Default)]
pub struct RenderScene {
    current: Vec<Vec3>,
    previous: Vec<Vec3>,
    mesh_ids: Vec<u32>,
    material_ids: Vec<u32>,
    /// Локальные AABB инстансов (для culling).
    bounds: Vec<Aabb>,
    render: Vec<Vec3>,
    /// Индексы видимых инстансов после [`Self::extract`].
    visible: Vec<u32>,
    stats: SceneExtractStats,
    camera_position: Vec3,
}

impl RenderScene {
    /// Пустая сцена.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Число инстансов.
    #[must_use]
    pub fn len(&self) -> usize {
        self.current.len()
    }

    /// Пуста ли сцена.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.current.is_empty()
    }

    /// Ёмкость буферов (для проверки отсутствия аллокаций после прогрева).
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.current.capacity()
    }

    /// Позиция камеры, относительно которой считаются render-позиции.
    #[must_use]
    pub fn camera_position(&self) -> Vec3 {
        self.camera_position
    }

    /// Задаёт позицию камеры.
    pub fn set_camera_position(&mut self, position: Vec3) {
        self.camera_position = position;
    }

    /// Очищает сцену, сохраняя ёмкость буферов.
    pub fn clear(&mut self) {
        self.current.clear();
        self.previous.clear();
        self.mesh_ids.clear();
        self.material_ids.clear();
        self.bounds.clear();
        self.render.clear();
        self.visible.clear();
        self.stats = SceneExtractStats::default();
        self.camera_position = Vec3::ZERO;
    }

    /// Добавляет инстанс с единичным bounding box.
    pub fn push(&mut self, current: Vec3, previous: Vec3, mesh: u32, material: u32) {
        self.push_with_bounds(current, previous, mesh, material, unit_bounds());
    }

    /// Добавляет инстанс с локальными границами (для точного culling).
    pub fn push_with_bounds(
        &mut self,
        current: Vec3,
        previous: Vec3,
        mesh: u32,
        material: u32,
        bounds: Aabb,
    ) {
        self.current.push(current);
        self.previous.push(previous);
        self.mesh_ids.push(mesh);
        self.material_ids.push(material);
        self.bounds.push(bounds);
    }

    /// Интерполирует позиции и переводит их в camera-relative.
    ///
    /// `alpha` ограничивается диапазоном `0..=1`.
    pub fn interpolate(&mut self, alpha: f32) {
        let alpha = alpha.clamp(0.0, 1.0);
        self.render.clear();
        for (current, previous) in self.current.iter().zip(&self.previous) {
            self.render
                .push(previous.lerp(*current, alpha) - self.camera_position);
        }
    }

    /// Camera-relative позиции после [`Self::interpolate`].
    #[must_use]
    pub fn render_positions(&self) -> &[Vec3] {
        &self.render
    }

    /// Текущие (не интерполированные) позиции.
    #[must_use]
    pub fn current_positions(&self) -> &[Vec3] {
        &self.current
    }

    /// Предыдущие позиции.
    #[must_use]
    pub fn previous_positions(&self) -> &[Vec3] {
        &self.previous
    }

    /// Mesh id по индексу.
    #[must_use]
    pub fn mesh_ids(&self) -> &[u32] {
        &self.mesh_ids
    }

    /// Material id по индексу.
    #[must_use]
    pub fn material_ids(&self) -> &[u32] {
        &self.material_ids
    }

    /// Локальные границы инстансов.
    #[must_use]
    pub fn bounds(&self) -> &[Aabb] {
        &self.bounds
    }

    /// Индексы видимых инстансов после [`Self::extract`].
    #[must_use]
    pub fn visible_indices(&self) -> &[u32] {
        &self.visible
    }

    /// Статистика последнего [`Self::extract`].
    #[must_use]
    pub fn stats(&self) -> SceneExtractStats {
        self.stats
    }

    /// Отсекает инстансы по фрустуму, используя camera-relative позиции
    /// после [`Self::interpolate`]; результат пишется в `out`.
    pub fn cull_into(&self, frustum: &Frustum, out: &mut Vec<u32>) -> CullStats {
        cull_instances(&self.render, &self.bounds, frustum, out)
    }

    /// Extract-шаг кадра (4.5): интерполяция позиций и culling.
    ///
    /// Возвращает статистику; индексы видимых доступны через
    /// [`Self::visible_indices`]. Буферы переиспользуются между кадрами.
    pub fn extract(&mut self, alpha: f32, frustum: &Frustum) -> SceneExtractStats {
        self.interpolate(alpha);
        // Поля заимствуются раздельно: render/bounds — чтение, visible — запись.
        let cull = cull_instances(&self.render, &self.bounds, frustum, &mut self.visible);
        self.stats = SceneExtractStats::from_cull(self.render.len(), cull);
        self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(camera: &Camera, view_space: Vec3) -> f32 {
        let clip = camera.projection_reversed_z() * view_space.extend(1.0);
        clip.z / clip.w
    }

    #[test]
    fn reversed_z_maps_near_to_one_and_far_to_zero() {
        let camera = Camera {
            near: 0.1,
            far: 100.0,
            ..Camera::default()
        };
        let near = project(&camera, Vec3::new(0.0, 0.0, -camera.near));
        let far = project(&camera, Vec3::new(0.0, 0.0, -camera.far));
        assert!((near - 1.0).abs() < 1e-4);
        assert!(far.abs() < 1e-4);
        assert!(near > far);
    }

    #[test]
    fn view_matrix_is_rotation_only_and_looks_down_negative_z() {
        let camera = Camera::new(Vec3::new(10.0, 5.0, -3.0), Vec3::X, Vec3::Y);
        let view = camera.view_matrix();
        assert!(view.w_axis.truncate().length() < 1e-4);
        let front = camera.camera_relative(camera.position + Vec3::X);
        let transformed = view * front.extend(1.0);
        assert!(transformed.x.abs() < 1e-4);
        assert!(transformed.y.abs() < 1e-4);
        assert!((transformed.z + 1.0).abs() < 1e-4);
    }

    #[test]
    fn camera_relative_subtracts_position() {
        let camera = Camera::new(Vec3::new(10.0, 0.0, 0.0), -Vec3::Z, Vec3::Y);
        assert_eq!(
            camera.camera_relative(Vec3::new(13.0, 2.0, -1.0)),
            Vec3::new(3.0, 2.0, -1.0)
        );
    }

    #[test]
    fn render_scene_interpolates_and_is_camera_relative() {
        let mut scene = RenderScene::new();
        scene.set_camera_position(Vec3::new(5.0, 0.0, 0.0));
        scene.push(Vec3::new(10.0, 0.0, 0.0), Vec3::new(8.0, 0.0, 0.0), 1, 2);
        scene.interpolate(0.5);
        assert_eq!(scene.render_positions(), &[Vec3::new(4.0, 0.0, 0.0)]);
        assert_eq!(scene.mesh_ids(), &[1]);
        assert_eq!(scene.material_ids(), &[2]);
    }

    #[test]
    fn alpha_is_clamped() {
        let mut scene = RenderScene::new();
        scene.push(Vec3::new(2.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 0.0), 0, 0);
        scene.interpolate(2.0);
        assert_eq!(scene.render_positions(), &[Vec3::new(2.0, 0.0, 0.0)]);
        scene.interpolate(-1.0);
        assert_eq!(scene.render_positions(), &[Vec3::ZERO]);
    }

    fn camera_frustum() -> Frustum {
        Frustum::from_view_projection(Camera::default().view_projection())
    }

    #[test]
    fn extract_interpolates_and_culls() {
        let mut scene = RenderScene::new();
        scene.push(Vec3::new(0.0, 0.0, -5.0), Vec3::new(0.0, 0.0, -4.0), 1, 2);
        scene.push(Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, 5.0), 3, 4);
        let stats = scene.extract(0.5, &camera_frustum());
        assert_eq!(
            stats,
            SceneExtractStats {
                instances: 2,
                visible: 1,
                culled: 1
            }
        );
        assert_eq!(scene.visible_indices(), &[0]);
        assert_eq!(scene.render_positions()[0], Vec3::new(0.0, 0.0, -4.5));
        assert_eq!(scene.stats(), stats);
    }

    #[test]
    fn custom_bounds_participate_in_culling() {
        let mut scene = RenderScene::new();
        let huge = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(500.0));
        scene.push_with_bounds(
            Vec3::new(0.0, 0.0, 200.0),
            Vec3::new(0.0, 0.0, 200.0),
            0,
            0,
            huge,
        );
        assert_eq!(scene.bounds(), &[huge]);
        let stats = scene.extract(1.0, &camera_frustum());
        assert_eq!(stats.visible, 1);
    }

    #[test]
    fn extract_reuses_buffers() {
        let mut scene = RenderScene::new();
        for index in 0..32 {
            let position = Vec3::new(0.0, 0.0, -1.0 - index as f32);
            scene.push(position, position, 0, 0);
        }
        scene.extract(1.0, &camera_frustum());
        let visible = scene.visible_indices().len();
        scene.extract(1.0, &camera_frustum());
        assert_eq!(scene.visible_indices().len(), visible);
        assert_eq!(scene.stats().instances, 32);
        scene.clear();
        assert_eq!(scene.stats(), SceneExtractStats::default());
        assert!(scene.visible_indices().is_empty());
    }

    #[test]
    fn buffers_are_reused_after_clear() {
        let mut scene = RenderScene::new();
        for index in 0..64 {
            let position = Vec3::splat(index as f32);
            scene.push(position, position, 0, 0);
        }
        let capacity = scene.capacity();
        scene.clear();
        assert!(scene.is_empty());
        for index in 0..64 {
            let position = Vec3::splat(index as f32);
            scene.push(position, position, 0, 0);
        }
        assert_eq!(scene.capacity(), capacity);
        scene.interpolate(1.0);
        assert_eq!(scene.render_positions().len(), 64);
    }
}
