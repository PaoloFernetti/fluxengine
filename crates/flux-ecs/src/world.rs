//! `World`: сущности, архетипы, миграции компонентов, UUID-карта, ресурсы,
//! запросы и change detection.
//!
//! `World` — единственный владелец хранилища. Отложенные операции — через
//! [`Commands`](crate::commands::Commands); прямые `spawn`/`despawn`/`insert`/
//! `remove` доступны системам, владеющим `&mut World`.
//!
//! Миграция `insert`/`remove` переносит общие компоненты перемещением
//! (без клонирования), сохраняет метки `added/changed` и обновляет позиции
//! переехавших сущностей.

use std::collections::HashMap;

use flux_core::time::SimTick;
use thiserror::Error;

use crate::archetypes::{
    Archetype, ArchetypeId, ArchetypeLayout, ComponentIndex, ComponentRegistry, Signature,
};
use crate::change::ComponentTick;
use crate::component::{ColumnVec, Component};
use crate::entity::{Entity, EntityAllocator, EntityUuid, Location};
use crate::query::{Query, QueryCache, QueryKey, QueryMut, QuerySpec};
use crate::resources::Resources;

/// Ошибка операции мира.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum EcsError {
    /// Сущность мертва (despawn или устаревшее поколение).
    #[error("entity is not alive")]
    DeadEntity,
    /// Позиция сущности не соответствует хранилищу.
    #[error("entity location is inconsistent")]
    InvalidLocation,
}

/// ECS-мир: один на симуляцию.
#[derive(Default)]
pub struct World {
    registry: ComponentRegistry,
    archetypes: Vec<Option<Archetype>>,
    by_signature: HashMap<Signature, ArchetypeId>,
    entities: EntityAllocator,
    uuid_index: HashMap<EntityUuid, Entity>,
    resources: Resources,
    query_cache: QueryCache,
    archetype_epoch: u64,
    tick: SimTick,
}

impl World {
    /// Создаёт пустой мир.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Регистрирует компонент (идемпотентно) и возвращает индекс.
    pub fn register<T: Component>(&mut self) -> ComponentIndex {
        self.registry.register::<T>()
    }

    /// Текущий тик симуляции (метки `added/changed`).
    #[must_use]
    pub fn tick(&self) -> SimTick {
        self.tick
    }

    /// Устанавливает тик (загрузка сейва, детерминированный тест).
    pub fn set_tick(&mut self, tick: SimTick) {
        self.tick = tick;
    }

    /// Продвигает тик на 1. Планировщик вызывает перед стадией.
    pub fn advance_tick(&mut self) {
        self.tick.advance(1);
    }

    /// Спавнит сущность со случайным UUID.
    pub fn spawn(&mut self) -> Entity {
        self.spawn_with_uuid(EntityUuid::new())
    }

    /// Спавнит сущность с заданным UUID (сейвы, сеть, тесты).
    pub fn spawn_with_uuid(&mut self, uuid: EntityUuid) -> Entity {
        let entity = self.entities.allocate(uuid);
        self.uuid_index.insert(uuid, entity);
        let tick = ComponentTick::new(self.tick);

        let archetype_id = self.archetype_for(Signature::EMPTY);
        let Some(archetype) = self.archetype_mut(archetype_id) else {
            panic!("empty archetype must exist after archetype_for");
        };
        let (chunk_index, slot) = archetype.allocate_slot();
        let Some(chunk) = archetype.chunk_mut(chunk_index) else {
            panic!("chunk must exist after allocate_slot");
        };
        chunk.finish_slot_with_ticks(entity, tick);
        archetype.on_insert();

        self.entities.set_location(
            entity,
            Some(Location {
                archetype: archetype_id,
                chunk: chunk_index,
                slot,
            }),
        );
        entity
    }

    /// Спавнит `count` пустых сущностей.
    pub fn spawn_batch(&mut self, count: usize) -> Vec<Entity> {
        (0..count).map(|_| self.spawn()).collect()
    }

    /// Удаляет сущность. UUID остаётся в карте для undo/сети.
    pub fn despawn(&mut self, entity: Entity) -> bool {
        let Some(location) = self.entities.location(entity) else {
            return false;
        };
        if !self.entities.free(entity) {
            return false;
        }

        if let Some(archetype) = self.archetype_mut(location.archetype) {
            if let Some(chunk) = archetype.chunk_mut(location.chunk) {
                let moved = chunk.remove_slot(location.slot as usize);
                archetype.on_remove();
                if let Some(moved) = moved {
                    self.entities.set_location(
                        moved,
                        Some(Location {
                            archetype: location.archetype,
                            chunk: location.chunk,
                            slot: location.slot,
                        }),
                    );
                }
            }
        }

        self.drop_if_empty(location.archetype);
        true
    }

    /// Вставляет компонент или заменяет существующий.
    ///
    /// # Errors
    ///
    /// [`EcsError::DeadEntity`] для мёртвой сущности;
    /// [`EcsError::InvalidLocation`] при рассогласовании хранилища.
    pub fn insert<T: Component>(&mut self, entity: Entity, value: T) -> Result<(), EcsError> {
        let index = self.registry.register::<T>();
        let location = self.entities.location(entity).ok_or(EcsError::DeadEntity)?;
        let now = self.tick;
        let tick = ComponentTick::new(now);

        let (old_signature, replace_position) = {
            let archetype = self
                .archetype_ref(location.archetype)
                .ok_or(EcsError::InvalidLocation)?;
            let position = archetype.layout().column_position(index);
            (archetype.signature(), position)
        };

        if let Some(position) = replace_position {
            let archetype = self
                .archetype_mut(location.archetype)
                .ok_or(EcsError::InvalidLocation)?;
            let chunk = archetype
                .chunk_mut(location.chunk)
                .ok_or(EcsError::InvalidLocation)?;
            let current = chunk
                .component_tick(position, location.slot as usize)
                .unwrap_or_default();
            let column = chunk
                .column_at_mut(position)
                .ok_or(EcsError::InvalidLocation)?;
            let column = column
                .as_any_mut()
                .downcast_mut::<ColumnVec<T>>()
                .ok_or(EcsError::InvalidLocation)?;
            let slot = column
                .get_mut(location.slot as usize)
                .ok_or(EcsError::InvalidLocation)?;
            *slot = value;
            chunk.set_component_tick(position, location.slot as usize, current.replaced(now));
            return Ok(());
        }

        let new_signature = old_signature.union(signature_of(index));
        let new_id = self.archetype_for(new_signature);
        self.migrate_add(entity, location, new_id, index, value, tick)
    }

    /// Удаляет компонент и возвращает его значение.
    ///
    /// # Errors
    ///
    /// Те же, что у [`World::insert`].
    pub fn remove<T: Component>(&mut self, entity: Entity) -> Result<Option<T>, EcsError> {
        let Some(index) = self.registry.index_of::<T>() else {
            return Ok(None);
        };
        let location = self.entities.location(entity).ok_or(EcsError::DeadEntity)?;

        let (old_signature, old_layout) = {
            let archetype = self
                .archetype_ref(location.archetype)
                .ok_or(EcsError::InvalidLocation)?;
            (archetype.signature(), archetype.layout().clone())
        };
        if !old_signature.contains(index) {
            return Ok(None);
        }

        let new_signature = old_signature.without(signature_of(index));
        let new_id = self.archetype_for(new_signature);
        self.migrate_remove::<T>(entity, location, new_id, index, &old_layout)
    }

    /// Компонент сущности.
    #[must_use]
    pub fn get<T: Component>(&self, entity: Entity) -> Option<&T> {
        let location = self.entities.location(entity)?;
        let archetype = self.archetype_ref(location.archetype)?;
        let index = self.registry.index_of::<T>()?;
        let position = archetype.layout().column_position(index)?;
        let chunk = archetype.chunk(location.chunk)?;
        let column = chunk.column_at(position)?;
        let column = column.as_any().downcast_ref::<ColumnVec<T>>()?;
        column.get(location.slot as usize)
    }

    /// Мутабельный компонент сущности.
    pub fn get_mut<T: Component>(&mut self, entity: Entity) -> Option<&mut T> {
        let location = self.entities.location(entity)?;
        let index = self.registry.index_of::<T>()?;
        let archetype = self.archetype_mut(location.archetype)?;
        let position = archetype.layout().column_position(index)?;
        let chunk = archetype.chunk_mut(location.chunk)?;
        let column = chunk.column_at_mut(position)?;
        let column = column.as_any_mut().downcast_mut::<ColumnVec<T>>()?;
        column.get_mut(location.slot as usize)
    }

    /// Есть ли у сущности компонент.
    #[must_use]
    pub fn contains<T: Component>(&self, entity: Entity) -> bool {
        let Some(location) = self.entities.location(entity) else {
            return false;
        };
        let Some(index) = self.registry.index_of::<T>() else {
            return false;
        };
        self.archetype_ref(location.archetype)
            .is_some_and(|archetype| archetype.signature().contains(index))
    }

    /// Метка `added/changed` компонента сущности.
    #[must_use]
    pub fn component_tick<T: Component>(&self, entity: Entity) -> Option<ComponentTick> {
        let location = self.entities.location(entity)?;
        let index = self.registry.index_of::<T>()?;
        let archetype = self.archetype_ref(location.archetype)?;
        let position = archetype.layout().column_position(index)?;
        archetype
            .chunk(location.chunk)?
            .component_tick(position, location.slot as usize)
    }

    /// UUID сущности.
    #[must_use]
    pub fn uuid(&self, entity: Entity) -> Option<EntityUuid> {
        self.entities.uuid(entity)
    }

    /// Сущность по UUID (может быть мертва после despawn — проверяйте [`World::is_alive`]).
    #[must_use]
    pub fn entity_by_uuid(&self, uuid: EntityUuid) -> Option<Entity> {
        self.uuid_index.get(&uuid).copied()
    }

    /// Убирает UUID из карты (после подтверждённого удаления).
    pub fn forget_uuid(&mut self, uuid: EntityUuid) {
        self.uuid_index.remove(&uuid);
    }

    /// Жива ли сущность.
    #[must_use]
    pub fn is_alive(&self, entity: Entity) -> bool {
        self.entities.is_alive(entity)
    }

    /// Число живых сущностей.
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Число непустых архетипов.
    #[must_use]
    pub fn archetype_count(&self) -> usize {
        self.archetypes.iter().flatten().count()
    }

    /// Реестр компонентов (только чтение).
    #[must_use]
    pub fn registry(&self) -> &ComponentRegistry {
        &self.registry
    }

    /// Кэш запросов (диагностика и тесты изоляции).
    #[must_use]
    pub fn query_cache(&self) -> &QueryCache {
        &self.query_cache
    }

    /// Ресурсы мира.
    #[must_use]
    pub fn resources(&self) -> &Resources {
        &self.resources
    }

    /// Мутабельные ресурсы мира.
    pub fn resources_mut(&mut self) -> &mut Resources {
        &mut self.resources
    }

    /// Вставляет ресурс, возвращая предыдущий.
    pub fn insert_resource<T: Send + Sync + 'static>(&mut self, value: T) -> Option<T> {
        self.resources.insert(value)
    }

    /// Ресурс по типу.
    #[must_use]
    pub fn resource<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.resources.get::<T>()
    }

    /// Мутабельный ресурс по типу.
    pub fn resource_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.resources.get_mut::<T>()
    }

    /// Удаляет ресурс.
    pub fn remove_resource<T: Send + Sync + 'static>(&mut self) -> Option<T> {
        self.resources.remove::<T>()
    }

    /// Запрос всех сущностей с компонентом `T`.
    pub fn query<T: Component>(&mut self) -> Query<'_, T> {
        self.query_filtered::<T>(&QuerySpec::new())
    }

    /// Запрос с фильтрами `With`/`Without`.
    pub fn query_filtered<T: Component>(&mut self, spec: &QuerySpec) -> Query<'_, T> {
        let index = self.registry.register::<T>();
        let (spec_with, spec_without) = spec.resolve(&mut self.registry);
        let key = QueryKey {
            with: signature_of(index).union(spec_with),
            without: spec_without,
        };
        let ids = self
            .query_cache
            .resolve(key, self.archetype_epoch, &self.archetypes);
        Query::new(&*self, ids, index)
    }

    /// Мутабельный запрос компонента `T` (батчи по слотам).
    pub fn query_mut<T: Component>(&mut self) -> QueryMut<'_, T> {
        let index = self.registry.register::<T>();
        let key = QueryKey {
            with: signature_of(index),
            without: Signature::EMPTY,
        };
        let ids = self
            .query_cache
            .resolve(key, self.archetype_epoch, &self.archetypes);
        QueryMut::new(self, ids, index)
    }

    /// Проверка инвариантов (тесты и debug-режим).
    pub fn validate(&self) {
        for archetype in self.archetypes.iter().flatten() {
            archetype.validate();
        }
    }

    pub(crate) fn archetype_ref(&self, id: ArchetypeId) -> Option<&Archetype> {
        self.archetypes.get(id.0 as usize)?.as_ref()
    }

    pub(crate) fn archetype_mut(&mut self, id: ArchetypeId) -> Option<&mut Archetype> {
        self.archetypes.get_mut(id.0 as usize)?.as_mut()
    }

    fn archetype_for(&mut self, signature: Signature) -> ArchetypeId {
        if let Some(&id) = self.by_signature.get(&signature) {
            if self.archetype_ref(id).is_some() {
                return id;
            }
        }

        let id = ArchetypeId(
            u32::try_from(self.archetypes.len())
                .unwrap_or_else(|_| panic!("archetype id space exhausted (u32::MAX)")),
        );
        let layout = self.registry.layout(signature);
        self.archetypes
            .push(Some(Archetype::new(id, signature, layout)));
        self.by_signature.insert(signature, id);
        self.archetype_epoch += 1;
        id
    }

    fn drop_if_empty(&mut self, id: ArchetypeId) {
        let Some(archetype) = self.archetype_ref(id) else {
            return;
        };
        if !archetype.is_empty() {
            return;
        }
        let signature = archetype.signature();
        if let Some(slot) = self.archetypes.get_mut(id.0 as usize) {
            *slot = None;
        }
        self.by_signature.remove(&signature);
        self.archetype_epoch += 1;
    }

    #[allow(clippy::too_many_arguments)]
    fn migrate_add<T: Component>(
        &mut self,
        entity: Entity,
        old_location: Location,
        new_id: ArchetypeId,
        new_index: ComponentIndex,
        value: T,
        tick: ComponentTick,
    ) -> Result<(), EcsError> {
        let (old_archetype, new_archetype) =
            two_archetypes_mut(&mut self.archetypes, old_location.archetype, new_id)
                .ok_or(EcsError::InvalidLocation)?;

        let old_layout = old_archetype.layout().clone();
        let new_layout = new_archetype.layout().clone();
        let old_signature = old_archetype.signature();
        let slot = old_location.slot as usize;
        let (new_chunk_index, new_slot) = new_archetype.allocate_slot();

        let old_chunk = old_archetype
            .chunk_mut(old_location.chunk)
            .ok_or(EcsError::InvalidLocation)?;
        let new_chunk = new_archetype
            .chunk_mut(new_chunk_index)
            .ok_or(EcsError::InvalidLocation)?;

        for (position, info) in new_layout.components.iter().enumerate() {
            if info.index == new_index {
                continue;
            }
            let target = new_chunk
                .column_at_mut(position)
                .ok_or(EcsError::InvalidLocation)?;
            if old_signature.contains(info.index) {
                let old_position = old_layout
                    .column_position(info.index)
                    .ok_or(EcsError::InvalidLocation)?;
                let component_tick = old_chunk
                    .component_tick(old_position, slot)
                    .unwrap_or_default();
                let source = old_chunk
                    .column_at_mut(old_position)
                    .ok_or(EcsError::InvalidLocation)?;
                target.push_from_erased(source, slot);
                old_chunk.swap_remove_tick(old_position, slot);
                new_chunk.push_component_tick(position, component_tick);
            } else {
                target.push_default();
            }
        }

        let t_position = new_layout
            .column_position(new_index)
            .ok_or(EcsError::InvalidLocation)?;
        let t_column = new_chunk
            .column_at_mut(t_position)
            .ok_or(EcsError::InvalidLocation)?;
        let t_column = t_column
            .as_any_mut()
            .downcast_mut::<ColumnVec<T>>()
            .ok_or(EcsError::InvalidLocation)?;
        t_column.push(value);
        new_chunk.push_component_tick(t_position, tick);

        new_chunk.finish_slot(entity);
        let moved = old_chunk.remove_entity_record(slot);
        new_archetype.on_insert();
        old_archetype.on_remove();

        self.entities.set_location(
            entity,
            Some(Location {
                archetype: new_id,
                chunk: new_chunk_index,
                slot: new_slot,
            }),
        );
        if let Some(moved) = moved {
            self.entities.set_location(
                moved,
                Some(Location {
                    archetype: old_location.archetype,
                    chunk: old_location.chunk,
                    slot: old_location.slot,
                }),
            );
        }
        self.drop_if_empty(old_location.archetype);
        Ok(())
    }

    fn migrate_remove<T: Component>(
        &mut self,
        entity: Entity,
        old_location: Location,
        new_id: ArchetypeId,
        removed_index: ComponentIndex,
        old_layout: &ArchetypeLayout,
    ) -> Result<Option<T>, EcsError> {
        let (old_archetype, new_archetype) =
            two_archetypes_mut(&mut self.archetypes, old_location.archetype, new_id)
                .ok_or(EcsError::InvalidLocation)?;

        let new_layout = new_archetype.layout().clone();
        let slot = old_location.slot as usize;
        let (new_chunk_index, new_slot) = new_archetype.allocate_slot();

        let old_chunk = old_archetype
            .chunk_mut(old_location.chunk)
            .ok_or(EcsError::InvalidLocation)?;
        let new_chunk = new_archetype
            .chunk_mut(new_chunk_index)
            .ok_or(EcsError::InvalidLocation)?;

        let removed_position = old_layout
            .column_position(removed_index)
            .ok_or(EcsError::InvalidLocation)?;
        let removed = {
            let removed_column = old_chunk
                .column_at_mut(removed_position)
                .ok_or(EcsError::InvalidLocation)?;
            removed_column
                .take(slot)
                .downcast::<T>()
                .ok()
                .map(|value| *value)
        };
        old_chunk.swap_remove_tick(removed_position, slot);

        for (position, info) in new_layout.components.iter().enumerate() {
            let old_position = old_layout
                .column_position(info.index)
                .ok_or(EcsError::InvalidLocation)?;
            let component_tick = old_chunk
                .component_tick(old_position, slot)
                .unwrap_or_default();
            let target = new_chunk
                .column_at_mut(position)
                .ok_or(EcsError::InvalidLocation)?;
            let source = old_chunk
                .column_at_mut(old_position)
                .ok_or(EcsError::InvalidLocation)?;
            target.push_from_erased(source, slot);
            old_chunk.swap_remove_tick(old_position, slot);
            new_chunk.push_component_tick(position, component_tick);
        }

        new_chunk.finish_slot(entity);
        let moved = old_chunk.remove_entity_record(slot);
        new_archetype.on_insert();
        old_archetype.on_remove();

        self.entities.set_location(
            entity,
            Some(Location {
                archetype: new_id,
                chunk: new_chunk_index,
                slot: new_slot,
            }),
        );
        if let Some(moved) = moved {
            self.entities.set_location(
                moved,
                Some(Location {
                    archetype: old_location.archetype,
                    chunk: old_location.chunk,
                    slot: old_location.slot,
                }),
            );
        }
        self.drop_if_empty(old_location.archetype);
        Ok(removed)
    }
}

fn signature_of(index: ComponentIndex) -> Signature {
    let mut signature = Signature::EMPTY;
    signature.insert(index);
    signature
}

fn two_archetypes_mut(
    archetypes: &mut [Option<Archetype>],
    a: ArchetypeId,
    b: ArchetypeId,
) -> Option<(&mut Archetype, &mut Archetype)> {
    if a == b {
        return None;
    }
    let (ai, bi) = (a.0 as usize, b.0 as usize);
    if ai < bi {
        let (left, right) = archetypes.split_at_mut(bi);
        let a_ref = left.get_mut(ai)?.as_mut()?;
        let b_ref = right.first_mut()?.as_mut()?;
        Some((a_ref, b_ref))
    } else {
        let (left, right) = archetypes.split_at_mut(ai);
        let b_ref = left.get_mut(bi)?.as_mut()?;
        let a_ref = right.first_mut()?.as_mut()?;
        Some((a_ref, b_ref))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::Commands;
    use crate::query::QuerySpec;
    use flux_core::schema::TypeId;
    use uuid::Uuid;

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct Position {
        x: f32,
        y: f32,
    }

    impl Component for Position {
        const TYPE_ID: TypeId =
            TypeId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0010));
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct Velocity {
        x: f32,
        y: f32,
    }

    impl Component for Velocity {
        const TYPE_ID: TypeId =
            TypeId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0011));
    }

    #[derive(Clone, Debug, Default, PartialEq)]
    struct Name(String);

    impl Component for Name {
        const TYPE_ID: TypeId =
            TypeId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0012));
    }

    #[test]
    fn spawn_and_despawn_invalidate_entity() {
        let mut world = World::new();
        let entity = world.spawn();
        assert!(world.is_alive(entity));
        assert_eq!(world.entity_count(), 1);

        assert!(world.despawn(entity));
        assert!(!world.is_alive(entity));
        assert_eq!(world.entity_count(), 0);
        assert_eq!(world.archetype_count(), 0);
    }

    #[test]
    fn insert_and_read_components() {
        let mut world = World::new();
        let entity = world.spawn();

        world.insert(entity, Position { x: 1.0, y: 2.0 }).unwrap();
        world.insert(entity, Velocity { x: 3.0, y: 4.0 }).unwrap();

        assert_eq!(
            world.get::<Position>(entity),
            Some(&Position { x: 1.0, y: 2.0 })
        );
        assert_eq!(
            world.get::<Velocity>(entity),
            Some(&Velocity { x: 3.0, y: 4.0 })
        );
        assert!(world.contains::<Position>(entity));
        world.validate();
    }

    #[test]
    fn insert_replaces_existing_component() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Position { x: 1.0, y: 1.0 }).unwrap();
        world.insert(entity, Position { x: 9.0, y: 9.0 }).unwrap();

        assert_eq!(
            world.get::<Position>(entity),
            Some(&Position { x: 9.0, y: 9.0 })
        );
        assert_eq!(world.entity_count(), 1);
        world.validate();
    }

    #[test]
    fn migration_preserves_other_components() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Position { x: 5.0, y: 6.0 }).unwrap();
        world.insert(entity, Velocity { x: 7.0, y: 8.0 }).unwrap();
        world.insert(entity, Name("hero".to_owned())).unwrap();

        assert_eq!(
            world.get::<Position>(entity),
            Some(&Position { x: 5.0, y: 6.0 })
        );
        assert_eq!(
            world.get::<Velocity>(entity),
            Some(&Velocity { x: 7.0, y: 8.0 })
        );
        assert_eq!(world.get::<Name>(entity), Some(&Name("hero".to_owned())));
        world.validate();
    }

    #[test]
    fn remove_returns_value_and_keeps_rest() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Position { x: 1.0, y: 1.0 }).unwrap();
        world.insert(entity, Velocity { x: 2.0, y: 2.0 }).unwrap();

        let removed = world.remove::<Velocity>(entity).unwrap();
        assert_eq!(removed, Some(Velocity { x: 2.0, y: 2.0 }));
        assert!(!world.contains::<Velocity>(entity));
        assert_eq!(
            world.get::<Position>(entity),
            Some(&Position { x: 1.0, y: 1.0 })
        );
        assert_eq!(world.remove::<Velocity>(entity).unwrap(), None);
        world.validate();
    }

    #[test]
    fn swap_remove_keeps_other_entities_intact() {
        let mut world = World::new();
        let entities: Vec<Entity> = (0..3)
            .map(|index| {
                let entity = world.spawn();
                world
                    .insert(
                        entity,
                        Position {
                            x: index as f32,
                            y: 0.0,
                        },
                    )
                    .unwrap();
                world
                    .insert(
                        entity,
                        Velocity {
                            x: 0.0,
                            y: index as f32,
                        },
                    )
                    .unwrap();
                entity
            })
            .collect();

        assert!(world.despawn(entities[1]));

        assert_eq!(
            world.get::<Position>(entities[0]),
            Some(&Position { x: 0.0, y: 0.0 })
        );
        assert_eq!(
            world.get::<Position>(entities[2]),
            Some(&Position { x: 2.0, y: 0.0 })
        );
        assert_eq!(
            world.get::<Velocity>(entities[2]),
            Some(&Velocity { x: 0.0, y: 2.0 })
        );
        assert_eq!(world.entity_count(), 2);
        world.validate();
    }

    #[test]
    fn empty_archetype_is_dropped() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Position::default()).unwrap();
        assert_eq!(world.archetype_count(), 1);

        world.remove::<Position>(entity).unwrap();
        assert_eq!(world.archetype_count(), 1);

        world.despawn(entity);
        assert_eq!(world.archetype_count(), 0);
    }

    #[test]
    fn uuid_is_stable_across_migrations() {
        let mut world = World::new();
        let uuid = EntityUuid::new();
        let entity = world.spawn_with_uuid(uuid);

        world.insert(entity, Position::default()).unwrap();
        world.insert(entity, Velocity::default()).unwrap();
        world.remove::<Position>(entity).unwrap();

        assert_eq!(world.uuid(entity), Some(uuid));
        assert_eq!(world.entity_by_uuid(uuid), Some(entity));
    }

    #[test]
    fn dead_entity_operations_fail() {
        let mut world = World::new();
        let entity = world.spawn();
        assert!(world.despawn(entity));

        assert_eq!(
            world.insert(entity, Position::default()),
            Err(EcsError::DeadEntity)
        );
        assert_eq!(world.remove::<Position>(entity), Err(EcsError::DeadEntity));
        assert!(world.get::<Position>(entity).is_none());
    }

    #[test]
    fn query_returns_only_matching_entities() {
        let mut world = World::new();
        let first = world.spawn();
        let second = world.spawn();
        let third = world.spawn();
        world.insert(first, Position { x: 1.0, y: 0.0 }).unwrap();
        world.insert(second, Position { x: 2.0, y: 0.0 }).unwrap();
        world.insert(second, Velocity { x: 0.0, y: 1.0 }).unwrap();
        world.insert(third, Velocity { x: 0.0, y: 2.0 }).unwrap();

        let positions: Vec<(Entity, f32)> = world
            .query::<Position>()
            .map(|(entity, position)| (entity, position.x))
            .collect();
        assert_eq!(positions.len(), 2);
        assert!(positions.contains(&(first, 1.0)));
        assert!(positions.contains(&(second, 2.0)));

        let without_velocity: Vec<Entity> = world
            .query_filtered::<Position>(&QuerySpec::new().without::<Velocity>())
            .map(|(entity, _)| entity)
            .collect();
        assert_eq!(without_velocity, vec![first]);
    }

    #[test]
    fn query_cache_is_isolated_by_key() {
        let mut world = World::new();
        let entity = world.spawn();
        world.insert(entity, Position::default()).unwrap();

        assert_eq!(world.query_cache().len(), 0);
        assert_eq!(world.query::<Position>().count(), 1);
        assert_eq!(world.query_cache().len(), 1);
        assert_eq!(world.query::<Velocity>().count(), 0);
        assert_eq!(world.query_cache().len(), 2);
    }

    #[test]
    fn added_and_changed_are_tick_scoped() {
        let mut world = World::new();
        world.advance_tick();
        let entity = world.spawn();
        world.insert(entity, Position::default()).unwrap();

        let after_insert = world.tick();
        assert_eq!(
            world.query::<Position>().added_since(SimTick::ZERO).count(),
            1
        );
        assert_eq!(
            world
                .query::<Position>()
                .changed_since(SimTick::ZERO)
                .count(),
            1
        );

        world.advance_tick();
        world.insert(entity, Position { x: 5.0, y: 0.0 }).unwrap();
        let after_replace = world.tick();
        assert_eq!(
            world.query::<Position>().added_since(after_insert).count(),
            0
        );
        assert_eq!(
            world
                .query::<Position>()
                .changed_since(after_insert)
                .count(),
            1
        );
        assert_eq!(
            world.query::<Position>().added_since(after_replace).count(),
            0
        );

        let tick = world.component_tick::<Position>(entity).unwrap();
        assert_eq!(tick.added, after_insert);
        assert_eq!(tick.changed, after_replace);
    }

    #[test]
    fn query_mut_batches_cover_all_entities() {
        let mut world = World::new();
        let entities = world.spawn_batch(10);
        for (index, entity) in entities.iter().enumerate() {
            world
                .insert(
                    *entity,
                    Position {
                        x: index as f32,
                        y: 0.0,
                    },
                )
                .unwrap();
        }

        let mut visited = 0;
        let mut batches = 0;
        world
            .query_mut::<Position>()
            .for_each_batch(3, |batch, values| {
                batches += 1;
                visited += batch.len();
                assert_eq!(batch.len(), values.len());
                for value in values {
                    value.x += 1.0;
                }
            });
        assert_eq!(visited, 10);
        assert_eq!(batches, 4);
        assert_eq!(
            world.get::<Position>(entities[0]),
            Some(&Position { x: 1.0, y: 0.0 })
        );
    }

    #[test]
    fn resources_are_accessible_from_world() {
        let mut world = World::new();
        assert!(world.resource::<u32>().is_none());
        world.insert_resource(41_u32);
        assert_eq!(world.resource::<u32>(), Some(&41));
        if let Some(value) = world.resource_mut::<u32>() {
            *value += 1;
        }
        assert_eq!(world.remove_resource::<u32>(), Some(42));
    }

    #[test]
    fn commands_are_applied_in_order() {
        let mut world = World::new();
        world.advance_tick();

        let mut commands = Commands::new();
        let uuid = EntityUuid::new();
        let entity = commands.spawn_with_uuid(uuid);
        commands.insert(entity, Position { x: 1.0, y: 2.0 });
        commands.insert(entity, Velocity { x: 3.0, y: 4.0 });

        assert_eq!(world.entity_count(), 0);
        let report = commands.apply(&mut world);
        assert_eq!(report.applied, 3);
        assert!(report.errors.is_empty());
        assert_eq!(world.entity_count(), 1);

        let entity = world.entity_by_uuid(uuid).unwrap();
        assert_eq!(
            world.get::<Position>(entity),
            Some(&Position { x: 1.0, y: 2.0 })
        );
        assert_eq!(
            world.get::<Velocity>(entity),
            Some(&Velocity { x: 3.0, y: 4.0 })
        );
    }

    #[test]
    fn hundred_thousand_spawn_despawn() {
        let mut world = World::new();
        for _ in 0..100_000 {
            let entity = world.spawn();
            world.insert(entity, Position::default()).unwrap();
            assert!(world.despawn(entity));
        }
        assert_eq!(world.entity_count(), 0);
        assert_eq!(world.archetype_count(), 0);
    }

    #[test]
    #[ignore = "release-only: 1M spawn/despawn"]
    fn million_spawn_despawn() {
        let mut world = World::new();
        for _ in 0..1_000_000 {
            let entity = world.spawn();
            world.insert(entity, Position::default()).unwrap();
            assert!(world.despawn(entity));
        }
        assert_eq!(world.entity_count(), 0);
    }
}
