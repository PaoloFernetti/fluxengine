//! Запросы: кэш архетипов по `QueryKey`, итерация по чанкам, батчи.
//!
//! Кэш инвалидируется только при создании/удалении архетипа (`epoch`),
//! а не при миграции сущностей между существующими архетипами. Ключ —
//! `with`/`without`-сигнатуры; порядок архетипов — по возрастанию
//! `ArchetypeId`, поэтому обход детерминирован (ADR-0030).
//!
//! Внутренний цикл идёт по плотным срезам `&[T]` после одного downcast
//! на столбец; батч — диапазон слотов чанка (по умолчанию 256) для
//! будущего параллельного исполнения.

use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::Arc;

use flux_core::time::SimTick;

use crate::archetypes::{
    Archetype, ArchetypeId, Chunk, ComponentIndex, ComponentRegistry, Signature,
};
use crate::component::{ColumnVec, Component};
use crate::entity::Entity;
use crate::world::World;

/// Размер батча по умолчанию (границы для будущего параллелизма).
pub const DEFAULT_BATCH_SIZE: usize = 256;

/// Ключ кэша запросов.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueryKey {
    /// Обязательные компоненты.
    pub with: Signature,
    /// Запрещённые компоненты.
    pub without: Signature,
}

type RegisterFn = fn(&mut ComponentRegistry) -> ComponentIndex;

fn register<T: Component>(registry: &mut ComponentRegistry) -> ComponentIndex {
    registry.register::<T>()
}

/// Описание фильтра `With`/`Without` для [`World::query_filtered`](crate::world::World::query_filtered).
#[derive(Clone, Default)]
pub struct QuerySpec {
    with: Vec<RegisterFn>,
    without: Vec<RegisterFn>,
}

impl QuerySpec {
    /// Пустой фильтр.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Требует наличие `T`.
    #[must_use]
    pub fn with<T: Component>(mut self) -> Self {
        self.with.push(register::<T>);
        self
    }

    /// Требует отсутствие `T`.
    #[must_use]
    pub fn without<T: Component>(mut self) -> Self {
        self.without.push(register::<T>);
        self
    }

    /// Регистрирует компоненты и строит сигнатуры.
    pub(crate) fn resolve(&self, registry: &mut ComponentRegistry) -> (Signature, Signature) {
        let mut with = Signature::EMPTY;
        for register in &self.with {
            with.insert(register(registry));
        }
        let mut without = Signature::EMPTY;
        for register in &self.without {
            without.insert(register(registry));
        }
        (with, without)
    }
}

/// Кэш списков архетипов по ключу запроса.
#[derive(Default)]
pub struct QueryCache {
    by_key: BTreeMap<QueryKey, Arc<[ArchetypeId]>>,
    epoch: u64,
}

impl QueryCache {
    /// Создаёт пустой кэш.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Число закэшированных ключей.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    /// Пуст ли кэш.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    /// Текущая эпоха архетипов.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Очищает кэш (например, при загрузке мира).
    pub fn clear(&mut self) {
        self.by_key.clear();
    }

    /// Список архетипов под ключ; кэш сбрасывается при смене `epoch`.
    pub(crate) fn resolve(
        &mut self,
        key: QueryKey,
        epoch: u64,
        archetypes: &[Option<Archetype>],
    ) -> Arc<[ArchetypeId]> {
        if self.epoch != epoch {
            self.by_key.clear();
            self.epoch = epoch;
        }
        if let Some(ids) = self.by_key.get(&key) {
            return Arc::clone(ids);
        }

        let ids: Vec<ArchetypeId> = archetypes
            .iter()
            .enumerate()
            .filter_map(|(index, archetype)| {
                let archetype = archetype.as_ref()?;
                let signature = archetype.signature();
                (signature.contains_all(key.with) && !signature.intersects(key.without))
                    .then_some(ArchetypeId(index as u32))
            })
            .collect();
        let ids: Arc<[ArchetypeId]> = ids.into();
        self.by_key.insert(key, Arc::clone(&ids));
        ids
    }
}

/// Чтение компонента `T` по сущностям, попадающим в фильтр.
pub struct Query<'w, T: Component> {
    world: &'w World,
    archetypes: Arc<[ArchetypeId]>,
    index: ComponentIndex,
    archetype_cursor: usize,
    chunk_cursor: usize,
    slot_cursor: usize,
    added_after: Option<SimTick>,
    changed_after: Option<SimTick>,
    _marker: PhantomData<&'w T>,
}

impl<'w, T: Component> Query<'w, T> {
    pub(crate) fn new(
        world: &'w World,
        archetypes: Arc<[ArchetypeId]>,
        index: ComponentIndex,
    ) -> Self {
        Self {
            world,
            archetypes,
            index,
            archetype_cursor: 0,
            chunk_cursor: 0,
            slot_cursor: 0,
            added_after: None,
            changed_after: None,
            _marker: PhantomData,
        }
    }

    /// Только компоненты, добавленные после `tick` (`Added<T>`).
    #[must_use]
    pub fn added_since(mut self, tick: SimTick) -> Self {
        self.added_after = Some(tick);
        self
    }

    /// Только компоненты, изменённые после `tick` (`Changed<T>`).
    #[must_use]
    pub fn changed_since(mut self, tick: SimTick) -> Self {
        self.changed_after = Some(tick);
        self
    }

    fn matches_tick(&self, chunk: &Chunk, position: usize, slot: usize) -> bool {
        if self.added_after.is_none() && self.changed_after.is_none() {
            return true;
        }
        let Some(tick) = chunk.component_tick(position, slot) else {
            return true;
        };
        if let Some(last) = self.added_after {
            if !tick.is_added_since(last) {
                return false;
            }
        }
        if let Some(last) = self.changed_after {
            if !tick.is_changed_since(last) {
                return false;
            }
        }
        true
    }
}

impl<'w, T: Component> Iterator for Query<'w, T> {
    type Item = (Entity, &'w T);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let &id = self.archetypes.get(self.archetype_cursor)?;
            let Some(archetype) = self.world.archetype_ref(id) else {
                self.archetype_cursor += 1;
                self.chunk_cursor = 0;
                self.slot_cursor = 0;
                continue;
            };
            let Some(chunk) = archetype.chunk(self.chunk_cursor as u32) else {
                self.archetype_cursor += 1;
                self.chunk_cursor = 0;
                self.slot_cursor = 0;
                continue;
            };
            if self.slot_cursor >= chunk.len() {
                self.chunk_cursor += 1;
                self.slot_cursor = 0;
                continue;
            }

            let slot = self.slot_cursor;
            self.slot_cursor += 1;

            let Some(position) = archetype.layout().column_position(self.index) else {
                continue;
            };
            if !self.matches_tick(chunk, position, slot) {
                continue;
            }
            let Some(column) = chunk.column_at(position) else {
                continue;
            };
            let Some(column) = column.as_any().downcast_ref::<ColumnVec<T>>() else {
                continue;
            };
            let (Some(value), Some(entity)) = (column.get(slot), chunk.entity_at(slot)) else {
                continue;
            };
            return Some((entity, value));
        }
    }
}

/// Мутабельное чтение компонента `T` с батчами по слотам чанка.
pub struct QueryMut<'w, T: Component> {
    world: &'w mut World,
    archetypes: Arc<[ArchetypeId]>,
    index: ComponentIndex,
    _marker: PhantomData<&'w mut T>,
}

impl<'w, T: Component> QueryMut<'w, T> {
    pub(crate) fn new(
        world: &'w mut World,
        archetypes: Arc<[ArchetypeId]>,
        index: ComponentIndex,
    ) -> Self {
        Self {
            world,
            archetypes,
            index,
            _marker: PhantomData,
        }
    }

    /// Обходит все значения по одному.
    pub fn for_each(&mut self, mut f: impl FnMut(Entity, &mut T)) {
        self.for_each_batch(usize::MAX, |entities, values| {
            for (entity, value) in entities.iter().zip(values.iter_mut()) {
                f(*entity, value);
            }
        });
    }

    /// Обходит значения батчами по `batch_size` слотов.
    pub fn for_each_batch(&mut self, batch_size: usize, mut f: impl FnMut(&[Entity], &mut [T])) {
        let batch_size = batch_size.max(1);
        for &id in self.archetypes.iter() {
            let Some(archetype) = self.world.archetype_mut(id) else {
                continue;
            };
            let Some(position) = archetype.layout().column_position(self.index) else {
                continue;
            };
            let chunk_count = archetype.chunk_count();
            for chunk_index in 0..chunk_count {
                let Some(chunk) = archetype.chunk_mut(chunk_index as u32) else {
                    continue;
                };
                let Some((entities, column)) = chunk.entities_and_column_mut(position) else {
                    continue;
                };
                let Some(column) = column.as_any_mut().downcast_mut::<ColumnVec<T>>() else {
                    continue;
                };
                let values = column.as_mut_slice();
                for (entity_batch, value_batch) in entities
                    .chunks(batch_size)
                    .zip(values.chunks_mut(batch_size))
                {
                    f(entity_batch, value_batch);
                }
            }
        }
    }
}
