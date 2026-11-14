//! Компоненты: trait `Component`, type-erased столбцы, drop-политика.
//!
//! Type-erasure — через `Any`-downcast на *столбец*: один downcast на чанк,
//! дальше плотная итерация `&[T]`. `unsafe` в MVP не используется.
//!
//! `Component: Default` — требование миграции `add`: новый компонент
//! инициализируется `T::default()`; явное значение задаёт `World::insert`.

use std::any::Any;

use flux_core::schema::{SchemaRef, TypeId};

use crate::schema::ComponentSchema;

/// Компонент — данные, привязанные к сущности.
pub trait Component: Default + Send + Sync + 'static {
    /// Стабильный идентификатор типа (UUID, ADR-0030).
    const TYPE_ID: TypeId;

    /// Схема компонента для инспектора и сериализации.
    ///
    /// Дерайв `Reflect` переопределяет метод полным списком полей; ручные
    /// реализации по умолчанию объявляют тип без полей (только `SchemaRef`).
    fn schema() -> ComponentSchema {
        ComponentSchema::new(
            SchemaRef::new(Self::TYPE_ID, 1),
            std::any::type_name::<Self>(),
        )
    }
}

/// Type-erased столбец компонента.
///
/// Все методы, работающие с `T`, реализуются в [`ColumnVec`]; трейт остаётся
/// object-safe, чтобы чанк хранил `Box<dyn Column>`.
pub trait Column: Any + Send + Sync {
    /// Число значений.
    fn len(&self) -> usize;

    /// Пусто ли.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Резервирует место.
    fn reserve(&mut self, additional: usize);

    /// Удаляет значение по индексу, перемещая последнее на его место.
    fn swap_remove(&mut self, index: usize);

    /// Очищает столбец.
    fn clear(&mut self);

    /// Забирает значение по индексу как `Any` (для `remove`).
    fn take(&mut self, index: usize) -> Box<dyn Any>;

    /// Добавляет `T::default()` (для миграции `add`).
    fn push_default(&mut self);

    /// Перемещает значение из другого столбца того же типа (миграция).
    fn push_from_erased(&mut self, source: &mut dyn Column, index: usize);

    /// Downcast только для чтения.
    fn as_any(&self) -> &dyn Any;

    /// Downcast с изменением.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Плотный столбец значений `T` (SoA-колонка внутри чанка).
pub struct ColumnVec<T: Component> {
    data: Vec<T>,
}

impl<T: Component> ColumnVec<T> {
    /// Создаёт столбец с резервом.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
        }
    }

    /// Создаёт пустой столбец.
    #[must_use]
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    /// Срез значений (плотная итерация запроса).
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.data
    }

    /// Мутабельный срез значений.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.data
    }

    /// Значение по индексу.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&T> {
        self.data.get(index)
    }

    /// Мутабельное значение по индексу.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.data.get_mut(index)
    }

    /// Добавляет значение.
    pub fn push(&mut self, value: T) {
        self.data.push(value);
    }
}

impl<T: Component> Default for ColumnVec<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Component> std::fmt::Debug for ColumnVec<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColumnVec")
            .field("len", &self.data.len())
            .field("type", &T::TYPE_ID)
            .finish()
    }
}

impl<T: Component> Column for ColumnVec<T> {
    fn len(&self) -> usize {
        self.data.len()
    }

    fn reserve(&mut self, additional: usize) {
        self.data.reserve(additional);
    }

    fn swap_remove(&mut self, index: usize) {
        let _ = self.data.swap_remove(index);
    }

    fn clear(&mut self) {
        self.data.clear();
    }

    fn take(&mut self, index: usize) -> Box<dyn Any> {
        Box::new(self.data.swap_remove(index))
    }

    fn push_default(&mut self) {
        self.data.push(T::default());
    }

    fn push_from_erased(&mut self, source: &mut dyn Column, index: usize) {
        if let Some(source) = source.as_any_mut().downcast_mut::<Self>() {
            let value = source.data.swap_remove(index);
            self.data.push(value);
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_core::schema::TypeId;
    use uuid::Uuid;

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct Health {
        value: f32,
    }

    impl Component for Health {
        const TYPE_ID: TypeId =
            TypeId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0001));
    }

    #[test]
    fn column_swap_remove_moves_last() {
        let mut column = ColumnVec::<Health>::new();
        column.push(Health { value: 1.0 });
        column.push(Health { value: 2.0 });
        column.push(Health { value: 3.0 });

        column.swap_remove(0);

        assert_eq!(column.len(), 2);
        assert_eq!(column.get(0), Some(&Health { value: 3.0 }));
        assert_eq!(column.get(1), Some(&Health { value: 2.0 }));
    }

    #[test]
    fn push_from_erased_moves_value_without_clone() {
        let mut source = ColumnVec::<Health>::new();
        source.push(Health { value: 7.0 });

        let mut target = ColumnVec::<Health>::new();
        target.push_from_erased(&mut source, 0);

        assert_eq!(source.len(), 0);
        assert_eq!(target.get(0), Some(&Health { value: 7.0 }));
    }

    #[test]
    fn take_returns_typed_value() {
        let mut column = ColumnVec::<Health>::new();
        column.push(Health { value: 4.0 });

        let value = column.take(0);
        assert_eq!(
            value.downcast::<Health>().ok().map(|v| *v),
            Some(Health { value: 4.0 })
        );
        assert!(column.is_empty());
    }
}
