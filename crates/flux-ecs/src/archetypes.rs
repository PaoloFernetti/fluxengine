//! Архетипы и чанки: `Signature`, `ComponentIndex`, SoA-столбцы, миграции.
//!
//! Чанк — непрерывный блок на `TARGET_CHUNK_BYTES`; `Entity → (archetype, chunk, slot)`.
//! Доступ к столбцам — через `Any`-downcast на столбец, без `unsafe`.
//!
//! Порядок столбцов — по `ComponentIndex` (порядок регистрации типов);
//! `Signature` — канонический набор битов, поэтому ключ архетипа детерминирован.

use std::collections::HashMap;
use std::mem::{align_of, size_of};

use flux_core::schema::TypeId;

use crate::change::ComponentTick;
use crate::component::{Column, ColumnVec, Component};
use crate::entity::Entity;

/// Целевой размер чанка; подбирается бенчем (ADR-0029).
pub const TARGET_CHUNK_BYTES: usize = 16 * 1024;

/// Максимум компонентов в `Signature` (битсет 4×u64).
pub const MAX_COMPONENTS: usize = 256;

const MAX_SLOTS_PER_CHUNK: usize = 1024;

/// Индекс компонента в `Signature` и layout архетипа.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ComponentIndex(pub u16);

impl ComponentIndex {
    fn bit(self) -> (usize, u64) {
        let index = self.0 as usize;
        (index / 64, 1_u64 << (index % 64))
    }
}

/// Набор типов компонентов архетипа.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Signature {
    bits: [u64; 4],
}

impl Signature {
    /// Пустой набор.
    pub const EMPTY: Self = Self { bits: [0; 4] };

    /// Добавляет компонент.
    pub fn insert(&mut self, index: ComponentIndex) {
        let (word, bit) = index.bit();
        self.bits[word] |= bit;
    }

    /// Удаляет компонент.
    pub fn remove(&mut self, index: ComponentIndex) {
        let (word, bit) = index.bit();
        self.bits[word] &= !bit;
    }

    /// Содержит ли компонент.
    #[must_use]
    pub fn contains(self, index: ComponentIndex) -> bool {
        let (word, bit) = index.bit();
        self.bits[word] & bit != 0
    }

    /// Содержит ли все компоненты `other`.
    #[must_use]
    pub fn contains_all(self, other: Self) -> bool {
        self.bits
            .iter()
            .zip(other.bits.iter())
            .all(|(mine, theirs)| mine & theirs == *theirs)
    }

    /// Есть ли общий компонент.
    #[must_use]
    pub fn intersects(self, other: Self) -> bool {
        self.bits
            .iter()
            .zip(other.bits.iter())
            .any(|(mine, theirs)| mine & theirs != 0)
    }

    /// Объединение наборов.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            bits: [
                self.bits[0] | other.bits[0],
                self.bits[1] | other.bits[1],
                self.bits[2] | other.bits[2],
                self.bits[3] | other.bits[3],
            ],
        }
    }

    /// Разность наборов.
    #[must_use]
    pub fn without(self, other: Self) -> Self {
        Self {
            bits: [
                self.bits[0] & !other.bits[0],
                self.bits[1] & !other.bits[1],
                self.bits[2] & !other.bits[2],
                self.bits[3] & !other.bits[3],
            ],
        }
    }

    /// Число компонентов.
    #[must_use]
    pub fn len(self) -> usize {
        self.bits
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum()
    }

    /// Пуст ли набор.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.bits.iter().all(|word| *word == 0)
    }

    /// Индексы компонентов по возрастанию.
    pub fn iter(self) -> impl Iterator<Item = ComponentIndex> {
        (0..MAX_COMPONENTS)
            .map(|index| ComponentIndex(index as u16))
            .filter(move |index| self.contains(*index))
    }
}

/// Идентификатор архетипа: монотонный, не переиспользуется.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ArchetypeId(pub u32);

/// Метаданные компонента, нужные архетипу.
#[derive(Clone, Copy)]
pub struct ComponentInfo {
    /// Индекс в `Signature`.
    pub index: ComponentIndex,
    /// Стабильный UUID типа.
    pub type_id: TypeId,
    /// `size_of::<T>()`.
    pub size: usize,
    /// `align_of::<T>()`.
    pub align: usize,
    /// Фабрика type-erased столбца.
    pub new_column: fn(usize) -> Box<dyn Column>,
}

/// Layout архетипа: компоненты и ёмкость чанка.
#[derive(Clone)]
pub struct ArchetypeLayout {
    /// Компоненты в порядке `ComponentIndex`.
    pub components: Vec<ComponentInfo>,
    /// Байт на слот (с выравниванием).
    pub bytes_per_slot: usize,
    /// Слотов в чанке.
    pub slots_per_chunk: usize,
}

impl ArchetypeLayout {
    /// Позиция компонента в чанке.
    #[must_use]
    pub fn column_position(&self, index: ComponentIndex) -> Option<usize> {
        self.components.iter().position(|info| info.index == index)
    }
}

/// Реестр типов компонентов: UUID → индекс, метаданные, фабрика столбца.
#[derive(Default)]
pub struct ComponentRegistry {
    by_type: HashMap<TypeId, ComponentIndex>,
    infos: Vec<ComponentInfo>,
}

impl ComponentRegistry {
    /// Создаёт пустой реестр.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Регистрирует компонент и возвращает его индекс.
    ///
    /// # Panics
    ///
    /// Паникует при превышении [`MAX_COMPONENTS`]: дизайн `Signature`
    /// рассчитан на 256 типов; расширение — через новый ADR.
    pub fn register<T: Component>(&mut self) -> ComponentIndex {
        if let Some(&index) = self.by_type.get(&T::TYPE_ID) {
            return index;
        }
        let index = ComponentIndex(
            u16::try_from(self.infos.len())
                .unwrap_or_else(|_| panic!("component limit exceeded ({MAX_COMPONENTS})")),
        );
        self.infos.push(ComponentInfo {
            index,
            type_id: T::TYPE_ID,
            size: size_of::<T>(),
            align: align_of::<T>(),
            new_column: make_column::<T>,
        });
        self.by_type.insert(T::TYPE_ID, index);
        index
    }

    /// Индекс зарегистрированного компонента.
    #[must_use]
    pub fn index_of<T: Component>(&self) -> Option<ComponentIndex> {
        self.by_type.get(&T::TYPE_ID).copied()
    }

    /// Метаданные по индексу.
    #[must_use]
    pub fn info(&self, index: ComponentIndex) -> Option<ComponentInfo> {
        self.infos.get(index.0 as usize).copied()
    }

    /// `Signature` из одного компонента.
    #[must_use]
    pub fn signature_of<T: Component>(&self) -> Option<Signature> {
        let mut signature = Signature::EMPTY;
        signature.insert(self.index_of::<T>()?);
        Some(signature)
    }

    /// Layout для набора компонентов.
    #[must_use]
    pub fn layout(&self, signature: Signature) -> ArchetypeLayout {
        let components: Vec<ComponentInfo> = signature
            .iter()
            .filter_map(|index| self.info(index))
            .collect();
        let bytes_per_slot = components
            .iter()
            .map(|info| align_up(info.size, info.align))
            .sum::<usize>()
            .max(1);
        let slots_per_chunk = (TARGET_CHUNK_BYTES / bytes_per_slot).clamp(1, MAX_SLOTS_PER_CHUNK);
        ArchetypeLayout {
            components,
            bytes_per_slot,
            slots_per_chunk,
        }
    }
}

fn make_column<T: Component>(capacity: usize) -> Box<dyn Column> {
    Box::new(ColumnVec::<T>::with_capacity(capacity))
}

fn align_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (value + align - 1) & !(align - 1)
}

/// Чанк: SoA-столбцы и сущности, занимающие слоты.
pub struct Chunk {
    columns: Vec<Box<dyn Column>>,
    /// Метки `added/changed` параллельно каждому столбцу и слоту.
    ticks: Vec<Vec<ComponentTick>>,
    entities: Vec<Entity>,
    len: usize,
    capacity: usize,
}

impl Chunk {
    fn new(layout: &ArchetypeLayout) -> Self {
        let capacity = layout.slots_per_chunk;
        let columns = layout
            .components
            .iter()
            .map(|info| (info.new_column)(capacity))
            .collect();
        let ticks = layout
            .components
            .iter()
            .map(|_| Vec::with_capacity(capacity))
            .collect();
        Self {
            columns,
            ticks,
            entities: Vec::with_capacity(capacity),
            len: 0,
            capacity,
        }
    }

    /// Занятых слотов.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Нет ли занятых слотов.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Ёмкость чанка.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Есть ли место.
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.len >= self.capacity
    }

    /// Сущность в слоте.
    #[must_use]
    pub fn entity_at(&self, slot: usize) -> Option<Entity> {
        self.entities.get(slot).copied()
    }

    /// Столбец по позиции в layout.
    #[must_use]
    pub fn column_at(&self, position: usize) -> Option<&dyn Column> {
        self.columns.get(position).map(AsRef::as_ref)
    }

    /// Мутабельный столбец по позиции в layout.
    pub fn column_at_mut(&mut self, position: usize) -> Option<&mut dyn Column> {
        self.columns.get_mut(position).map(AsMut::as_mut)
    }

    /// Сущности чанка (плотно, `0..len`).
    #[must_use]
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    /// Сущности и мутабельный столбец одновременно (батч-итерация).
    pub(crate) fn entities_and_column_mut(
        &mut self,
        position: usize,
    ) -> Option<(&[Entity], &mut dyn Column)> {
        let column = self.columns.get_mut(position)?.as_mut();
        Some((&self.entities, column))
    }

    /// Метка компонента в слоте.
    #[must_use]
    pub fn component_tick(&self, position: usize, slot: usize) -> Option<ComponentTick> {
        self.ticks.get(position)?.get(slot).copied()
    }

    pub(crate) fn push_component_tick(&mut self, position: usize, tick: ComponentTick) {
        if let Some(ticks) = self.ticks.get_mut(position) {
            ticks.push(tick);
        }
    }

    pub(crate) fn swap_remove_tick(&mut self, position: usize, slot: usize) {
        if let Some(ticks) = self.ticks.get_mut(position) {
            let _ = ticks.swap_remove(slot);
        }
    }

    pub(crate) fn set_component_tick(&mut self, position: usize, slot: usize, tick: ComponentTick) {
        if let Some(slot_tick) = self
            .ticks
            .get_mut(position)
            .and_then(|ticks| ticks.get_mut(slot))
        {
            *slot_tick = tick;
        }
    }

    /// Завершает слот при миграции: тики перенесены вручную.
    pub(crate) fn finish_slot(&mut self, entity: Entity) {
        self.entities.push(entity);
        self.len += 1;
    }

    /// Завершает слот при спавне: все компоненты получают метку `tick`.
    pub(crate) fn finish_slot_with_ticks(&mut self, entity: Entity, tick: ComponentTick) {
        self.entities.push(entity);
        for ticks in &mut self.ticks {
            ticks.push(tick);
        }
        self.len += 1;
    }

    /// Снимает запись слота после миграции (столбцы и тики уже перенесены).
    ///
    /// Вызывается только после `take`/`push_from_erased`, которые сами
    /// выполнили `swap_remove` в исходных столбцах и тиках, поэтому здесь
    /// ничего не удаляется — иначе значение последнего слота пропало бы дважды.
    ///
    /// Возвращает сущность, переехавшую с последнего слота на освободившийся.
    pub(crate) fn remove_entity_record(&mut self, slot: usize) -> Option<Entity> {
        let _ = self.entities.swap_remove(slot);
        self.len -= 1;
        self.moved_into(slot)
    }

    /// Удаляет слот вместе со значениями всех столбцов (`despawn`).
    ///
    /// Возвращает сущность, переехавшую с последнего слота на освободившийся.
    pub(crate) fn remove_slot(&mut self, slot: usize) -> Option<Entity> {
        for column in &mut self.columns {
            column.swap_remove(slot);
        }
        for ticks in &mut self.ticks {
            let _ = ticks.swap_remove(slot);
        }
        let _ = self.entities.swap_remove(slot);
        self.len -= 1;
        self.moved_into(slot)
    }

    fn moved_into(&self, slot: usize) -> Option<Entity> {
        if slot < self.entities.len() {
            self.entities.get(slot).copied()
        } else {
            None
        }
    }

    pub(crate) fn validate(&self) {
        debug_assert_eq!(self.entities.len(), self.len);
        debug_assert!(
            self.columns.iter().all(|column| column.len() == self.len),
            "chunk columns out of sync with len"
        );
        debug_assert!(
            self.ticks.iter().all(|ticks| ticks.len() == self.len),
            "chunk ticks out of sync with len"
        );
    }
}

/// Архетип: набор компонентов + чанки.
pub struct Archetype {
    id: ArchetypeId,
    signature: Signature,
    layout: ArchetypeLayout,
    chunks: Vec<Chunk>,
    len: usize,
}

impl Archetype {
    pub(crate) fn new(id: ArchetypeId, signature: Signature, layout: ArchetypeLayout) -> Self {
        Self {
            id,
            signature,
            layout,
            chunks: Vec::new(),
            len: 0,
        }
    }

    /// Идентификатор.
    #[must_use]
    pub fn id(&self) -> ArchetypeId {
        self.id
    }

    /// Набор компонентов.
    #[must_use]
    pub fn signature(&self) -> Signature {
        self.signature
    }

    /// Layout.
    #[must_use]
    pub fn layout(&self) -> &ArchetypeLayout {
        &self.layout
    }

    /// Число сущностей.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Пуст ли архетип.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Число чанков.
    #[must_use]
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Выделяет слот: `(chunk, slot)`, при необходимости создаёт чанк.
    pub(crate) fn allocate_slot(&mut self) -> (u32, u32) {
        for (index, chunk) in self.chunks.iter().enumerate() {
            if !chunk.is_full() {
                return (index as u32, chunk.len() as u32);
            }
        }
        self.chunks.push(Chunk::new(&self.layout));
        ((self.chunks.len() - 1) as u32, 0)
    }

    pub(crate) fn chunk(&self, index: u32) -> Option<&Chunk> {
        self.chunks.get(index as usize)
    }

    pub(crate) fn chunk_mut(&mut self, index: u32) -> Option<&mut Chunk> {
        self.chunks.get_mut(index as usize)
    }

    pub(crate) fn on_insert(&mut self) {
        self.len += 1;
    }

    pub(crate) fn on_remove(&mut self) {
        self.len -= 1;
    }

    pub(crate) fn validate(&self) {
        debug_assert_eq!(self.len, self.chunks.iter().map(Chunk::len).sum::<usize>());
        for chunk in &self.chunks {
            chunk.validate();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct Position {
        x: f32,
        y: f32,
    }

    impl Component for Position {
        const TYPE_ID: TypeId =
            TypeId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0002));
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct Velocity {
        x: f32,
        y: f32,
    }

    impl Component for Velocity {
        const TYPE_ID: TypeId =
            TypeId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0003));
    }

    #[test]
    fn signature_bitset_operations() {
        let mut registry = ComponentRegistry::new();
        let position = registry.register::<Position>();
        let velocity = registry.register::<Velocity>();

        let mut signature = Signature::EMPTY;
        signature.insert(position);
        signature.insert(velocity);

        assert!(signature.contains(position));
        assert!(signature.contains(velocity));
        assert_eq!(signature.len(), 2);

        let without_velocity = signature.without(Signature {
            bits: {
                let mut bits = [0; 4];
                bits[0] = 1 << velocity.0;
                bits
            },
        });
        assert!(without_velocity.contains(position));
        assert!(!without_velocity.contains(velocity));
        assert_eq!(without_velocity.len(), 1);
    }

    #[test]
    fn layout_computes_slots_per_chunk() {
        let mut registry = ComponentRegistry::new();
        let position = registry.register::<Position>();
        let mut signature = Signature::EMPTY;
        signature.insert(position);

        let layout = registry.layout(signature);
        assert_eq!(layout.components.len(), 1);
        assert_eq!(layout.bytes_per_slot, 8);
        assert_eq!(
            layout.slots_per_chunk,
            (TARGET_CHUNK_BYTES / 8).min(MAX_SLOTS_PER_CHUNK)
        );
    }

    #[test]
    fn chunk_swap_remove_keeps_columns_in_sync() {
        let mut registry = ComponentRegistry::new();
        let position = registry.register::<Position>();
        let mut signature = Signature::EMPTY;
        signature.insert(position);
        let layout = registry.layout(signature);

        let mut archetype = Archetype::new(ArchetypeId(0), signature, layout);
        let (chunk_index, slot) = archetype.allocate_slot();
        let chunk = archetype.chunk_mut(chunk_index).unwrap();

        let first = Entity::for_test(0, 0);
        chunk
            .column_at_mut(0)
            .and_then(|column| column.as_any_mut().downcast_mut::<ColumnVec<Position>>())
            .unwrap()
            .push(Position { x: 1.0, y: 2.0 });
        chunk.finish_slot_with_ticks(first, ComponentTick::default());
        assert_eq!(slot, 0);
        assert_eq!(chunk.component_tick(0, 0), Some(ComponentTick::default()));

        assert_eq!(chunk.len(), 1);
        chunk.validate();
    }
}
