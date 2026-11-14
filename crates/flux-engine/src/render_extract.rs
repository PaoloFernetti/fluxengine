//! Extract ECS → RenderScene (I4).
//!
//! Снапшот кадра: SoA-буферы `RenderScene`, alpha интерполяции между тиками,
//! camera-relative позиции. Экстрактор переиспользует внутренний буфер, поэтому
//! после прогрева не аллоцирует. Порядок детерминирован: сортировка по
//! `EntityUuid`.

use std::time::{Duration, Instant};

use flux_core::schema::{uuid_from_u128, TypeId};
use flux_ecs::component::Component;
use flux_ecs::entity::Entity;
use flux_ecs::world::World;
use flux_graphics::{Camera, RenderScene};
use flux_math::Vec3;

/// Компонент трансформа для рендера: текущая и предыдущая позиции.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderTransform {
    /// Позиция на текущем тике.
    pub current: Vec3,
    /// Позиция на предыдущем тике (для интерполяции).
    pub previous: Vec3,
}

impl Component for RenderTransform {
    const TYPE_ID: TypeId =
        TypeId::from_uuid(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0800));
}

/// Компонент отрисовываемого меша.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeshRenderer {
    /// Индекс меша.
    pub mesh: u32,
    /// Индекс материала.
    pub material: u32,
}

impl Component for MeshRenderer {
    const TYPE_ID: TypeId =
        TypeId::from_uuid(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0801));
}

/// Метрики экстракции.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExtractStats {
    /// Число инстансов, попавших в сцену.
    pub entities: usize,
    /// Время экстракции (гейт G4: ≤ 1 мс на эталонной сцене).
    pub elapsed: Duration,
}

/// Экстрактор ECS → [`RenderScene`] с переиспользуемым буфером.
#[derive(Clone, Debug, Default)]
pub struct RenderExtractor {
    transforms: Vec<(Entity, RenderTransform)>,
}

impl RenderExtractor {
    /// Пустой экстрактор.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Заполняет `scene` из мира.
    ///
    /// Сущности без [`MeshRenderer`] пропускаются; порядок — по `EntityUuid`.
    /// `alpha` — доля интерполяции между предыдущим и текущим тиком.
    pub fn extract(
        &mut self,
        world: &mut World,
        scene: &mut RenderScene,
        camera: &Camera,
        alpha: f32,
    ) -> ExtractStats {
        let start = Instant::now();
        self.transforms.clear();
        for (entity, transform) in world.query::<RenderTransform>() {
            self.transforms.push((entity, *transform));
        }
        self.transforms
            .sort_by_key(|(entity, _)| world.uuid(*entity));

        scene.clear();
        scene.set_camera_position(camera.position);
        let mut entities = 0;
        for (entity, transform) in &self.transforms {
            let Some(mesh) = world.get::<MeshRenderer>(*entity) else {
                continue;
            };
            scene.push(
                transform.current,
                transform.previous,
                mesh.mesh,
                mesh.material,
            );
            entities += 1;
        }
        scene.interpolate(alpha);
        ExtractStats {
            entities,
            elapsed: start.elapsed(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_ecs::entity::EntityUuid;
    use uuid::Uuid;

    fn uuid(value: u128) -> EntityUuid {
        EntityUuid(Uuid::from_u128(value))
    }

    fn camera() -> Camera {
        Camera::new(Vec3::new(5.0, 0.0, 0.0), -Vec3::Z, Vec3::Y)
    }

    #[test]
    fn extract_fills_scene_in_uuid_order() {
        let mut world = World::new();
        let later = world.spawn_with_uuid(uuid(2));
        world
            .insert(
                later,
                RenderTransform {
                    current: Vec3::new(11.0, 0.0, 0.0),
                    previous: Vec3::new(9.0, 0.0, 0.0),
                },
            )
            .unwrap();
        world
            .insert(
                later,
                MeshRenderer {
                    mesh: 2,
                    material: 20,
                },
            )
            .unwrap();
        let earlier = world.spawn_with_uuid(uuid(1));
        world
            .insert(
                earlier,
                RenderTransform {
                    current: Vec3::new(6.0, 1.0, 0.0),
                    previous: Vec3::new(6.0, 1.0, 0.0),
                },
            )
            .unwrap();
        world
            .insert(
                earlier,
                MeshRenderer {
                    mesh: 1,
                    material: 10,
                },
            )
            .unwrap();

        let mut extractor = RenderExtractor::new();
        let mut scene = RenderScene::new();
        let stats = extractor.extract(&mut world, &mut scene, &camera(), 0.5);
        assert_eq!(stats.entities, 2);
        assert_eq!(scene.len(), 2);
        assert_eq!(scene.mesh_ids(), &[1, 2]);
        assert_eq!(scene.material_ids(), &[10, 20]);
        // camera-relative: (6,1,0)-5 = (1,1,0); (9..11 lerp .5 = 10)-5 = 5.
        assert_eq!(
            scene.render_positions(),
            &[Vec3::new(1.0, 1.0, 0.0), Vec3::new(5.0, 0.0, 0.0)]
        );
    }

    #[test]
    fn entities_without_mesh_are_skipped() {
        let mut world = World::new();
        let entity = world.spawn_with_uuid(uuid(3));
        world
            .insert(
                entity,
                RenderTransform {
                    current: Vec3::ONE,
                    previous: Vec3::ONE,
                },
            )
            .unwrap();
        let mut extractor = RenderExtractor::new();
        let mut scene = RenderScene::new();
        let stats = extractor.extract(&mut world, &mut scene, &Camera::default(), 0.0);
        assert_eq!(stats.entities, 0);
        assert!(scene.is_empty());
    }

    #[test]
    fn repeated_extract_reuses_capacity() {
        let mut world = World::new();
        for value in 0..32_u128 {
            let entity = world.spawn_with_uuid(uuid(value + 1));
            world.insert(entity, RenderTransform::default()).unwrap();
            world
                .insert(
                    entity,
                    MeshRenderer {
                        mesh: value as u32,
                        material: 0,
                    },
                )
                .unwrap();
        }
        let mut extractor = RenderExtractor::new();
        let mut scene = RenderScene::new();
        extractor.extract(&mut world, &mut scene, &Camera::default(), 0.0);
        let capacity = scene.capacity();
        for _ in 0..3 {
            let stats = extractor.extract(&mut world, &mut scene, &Camera::default(), 0.5);
            assert_eq!(stats.entities, 32);
        }
        assert_eq!(scene.capacity(), capacity);
    }
}
