//! Сущности: `Entity { index, generation }`, аллокатор слотов, UUID-карта.
//!
//! Инварианты: generation для слота строго возрастает и не повторяется
//! (при достижении `u32::MAX` слот выводится из оборота); `entity_uuid`
//! стабилен при миграциях архетипа.

use uuid::Uuid;

use crate::archetypes::ArchetypeId;

/// Сущность: индекс слота + поколение.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Entity {
    index: u32,
    generation: u32,
}

impl Entity {
    /// Индекс слота.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Поколение слота.
    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }

    #[cfg(test)]
    pub(crate) const fn for_test(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }
}

/// Стабильный идентификатор сущности для сейвов и сети.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct EntityUuid(pub Uuid);

impl EntityUuid {
    /// Новый случайный UUID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EntityUuid {
    fn default() -> Self {
        Self::new()
    }
}

/// Позиция сущности в хранилище.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location {
    /// Архетип.
    pub archetype: ArchetypeId,
    /// Чанк внутри архетипа.
    pub chunk: u32,
    /// Слот внутри чанка.
    pub slot: u32,
}

struct EntityRecord {
    generation: u32,
    uuid: EntityUuid,
    alive: bool,
    location: Option<Location>,
}

/// Аллокатор слотов с монотонным поколением.
#[derive(Default)]
pub struct EntityAllocator {
    records: Vec<EntityRecord>,
    free: Vec<u32>,
}

impl EntityAllocator {
    /// Создаёт пустой аллокатор.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Выделяет слот под новую сущность.
    ///
    /// При переиспользовании слота поколение инкрементируется; слот с
    /// исчерпанным поколением (`u32::MAX`) выводится из оборота навсегда.
    pub fn allocate(&mut self, uuid: EntityUuid) -> Entity {
        while let Some(index) = self.free.pop() {
            let record = &mut self.records[index as usize];
            if record.generation == u32::MAX {
                continue;
            }
            record.generation += 1;
            record.uuid = uuid;
            record.alive = true;
            record.location = None;
            return Entity {
                index,
                generation: record.generation,
            };
        }

        let index = u32::try_from(self.records.len())
            .unwrap_or_else(|_| panic!("entity index space exhausted (u32::MAX slots)"));
        self.records.push(EntityRecord {
            generation: 0,
            uuid,
            alive: true,
            location: None,
        });
        Entity {
            index,
            generation: 0,
        }
    }

    /// Освобождает слот. Возвращает `false`, если сущность уже мертва.
    pub fn free(&mut self, entity: Entity) -> bool {
        let Some(record) = self.records.get_mut(entity.index as usize) else {
            return false;
        };
        if !record.alive || record.generation != entity.generation {
            return false;
        }
        record.alive = false;
        record.location = None;
        if record.generation != u32::MAX {
            self.free.push(entity.index);
        }
        true
    }

    /// Жива ли сущность (совпадает индекс и поколение).
    #[must_use]
    pub fn is_alive(&self, entity: Entity) -> bool {
        self.records
            .get(entity.index as usize)
            .is_some_and(|record| record.alive && record.generation == entity.generation)
    }

    /// Позиция сущности.
    #[must_use]
    pub fn location(&self, entity: Entity) -> Option<Location> {
        let record = self.records.get(entity.index as usize)?;
        if !record.alive || record.generation != entity.generation {
            return None;
        }
        record.location
    }

    /// Записывает позицию сущности.
    pub fn set_location(&mut self, entity: Entity, location: Option<Location>) {
        if let Some(record) = self.records.get_mut(entity.index as usize) {
            if record.generation == entity.generation {
                record.location = location;
            }
        }
    }

    /// UUID сущности.
    #[must_use]
    pub fn uuid(&self, entity: Entity) -> Option<EntityUuid> {
        let record = self.records.get(entity.index as usize)?;
        if !record.alive || record.generation != entity.generation {
            return None;
        }
        Some(record.uuid)
    }

    /// Число живых сущностей.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.iter().filter(|record| record.alive).count()
    }

    /// Нет ли живых сущностей.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Число выделенных слотов (включая свободные).
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.records.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_increases_on_reuse() {
        let mut allocator = EntityAllocator::new();
        let first = allocator.allocate(EntityUuid::new());
        assert!(allocator.free(first));

        let second = allocator.allocate(EntityUuid::new());
        assert_eq!(second.index(), first.index());
        assert!(second.generation() > first.generation());
        assert!(!allocator.is_alive(first));
        assert!(allocator.is_alive(second));
    }

    #[test]
    fn stale_entity_is_rejected() {
        let mut allocator = EntityAllocator::new();
        let entity = allocator.allocate(EntityUuid::new());
        assert!(allocator.free(entity));
        assert!(allocator.location(entity).is_none());
        assert!(!allocator.free(entity));
    }

    #[test]
    fn exhausted_generation_slot_is_retired() {
        let mut allocator = EntityAllocator::new();
        let entity = allocator.allocate(EntityUuid::new());
        // Симулируем исчерпание поколения, чтобы проверить вывод слота из оборота.
        allocator.records[entity.index() as usize].generation = u32::MAX;
        let exhausted = Entity::for_test(entity.index(), u32::MAX);
        assert!(allocator.free(exhausted));
        let next = allocator.allocate(EntityUuid::new());
        assert_ne!(next.index(), entity.index());
    }
}
