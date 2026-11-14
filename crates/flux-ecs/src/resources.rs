//! Типизированные ресурсы мира: синглтоны по `TypeId`.
//!
//! Ресурсы не привязаны к сущностям и живут в [`World`](crate::world::World).
//! Хранение — `HashMap<TypeId, Box<dyn Any>>`; итерации по ресурсам нет,
//! поэтому `HashMap` не влияет на детерминизм (ADR-0030).

use std::any::{Any, TypeId};
use std::collections::HashMap;

/// Контейнер ресурсов.
#[derive(Default)]
pub struct Resources {
    values: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Resources {
    /// Создаёт пустой контейнер.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Вставляет ресурс, возвращая предыдущее значение того же типа.
    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) -> Option<T> {
        self.values
            .insert(TypeId::of::<T>(), Box::new(value))
            .and_then(|previous| previous.downcast::<T>().ok())
            .map(|previous| *previous)
    }

    /// Ресурс по типу.
    #[must_use]
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.values.get(&TypeId::of::<T>())?.downcast_ref::<T>()
    }

    /// Мутабельный ресурс по типу.
    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.values.get_mut(&TypeId::of::<T>())?.downcast_mut::<T>()
    }

    /// Удаляет ресурс и возвращает его.
    pub fn remove<T: Send + Sync + 'static>(&mut self) -> Option<T> {
        self.values
            .remove(&TypeId::of::<T>())?
            .downcast::<T>()
            .ok()
            .map(|value| *value)
    }

    /// Есть ли ресурс.
    #[must_use]
    pub fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.values.contains_key(&TypeId::of::<T>())
    }

    /// Число ресурсов.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Нет ли ресурсов.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Очищает контейнер.
    pub fn clear(&mut self) {
        self.values.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default, PartialEq)]
    struct Counter(u32);

    #[derive(Debug, Default, PartialEq)]
    struct Frame(u32);

    #[test]
    fn resources_are_typed_and_isolated() {
        let mut resources = Resources::new();
        assert!(resources.insert(Counter(3)).is_none());
        assert!(resources.insert(Frame(7)).is_none());

        assert_eq!(resources.get::<Counter>(), Some(&Counter(3)));
        assert_eq!(resources.get::<Frame>(), Some(&Frame(7)));
        assert_eq!(resources.len(), 2);

        if let Some(counter) = resources.get_mut::<Counter>() {
            counter.0 += 1;
        }
        assert_eq!(resources.get::<Counter>(), Some(&Counter(4)));
    }

    #[test]
    fn insert_replaces_and_remove_returns_value() {
        let mut resources = Resources::new();
        resources.insert(Counter(1));
        assert_eq!(resources.insert(Counter(2)), Some(Counter(1)));
        assert_eq!(resources.remove::<Counter>(), Some(Counter(2)));
        assert!(resources.get::<Counter>().is_none());
        assert!(resources.is_empty());
    }
}
