//! Generational-контейнер: index + generation.

/// Индекс с поколением.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GenerationalIndex {
    index: u32,
    generation: u32,
}

impl GenerationalIndex {
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
}

#[derive(Clone, Debug)]
struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

/// Контейнер с generational-индексами.
#[derive(Clone, Debug)]
pub struct SlotMap<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    len: usize,
}

impl<T> SlotMap<T> {
    /// Создаёт пустой контейнер.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            len: 0,
        }
    }

    /// Вставляет значение и возвращает индекс.
    pub fn insert(&mut self, value: T) -> GenerationalIndex {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.value = Some(value);
            self.len += 1;
            GenerationalIndex {
                index,
                generation: slot.generation,
            }
        } else {
            let index = u32::try_from(self.slots.len()).unwrap_or(u32::MAX);
            self.slots.push(Slot {
                generation: 0,
                value: Some(value),
            });
            self.len += 1;
            GenerationalIndex {
                index,
                generation: 0,
            }
        }
    }

    /// Удаляет значение; устаревший индекс возвращает `None`.
    pub fn remove(&mut self, index: GenerationalIndex) -> Option<T> {
        let slot = self.slots.get_mut(index.index as usize)?;
        if slot.generation != index.generation {
            return None;
        }
        let value = slot.value.take()?;
        slot.generation = slot.generation.wrapping_add(1);
        self.free.push(index.index);
        self.len = self.len.saturating_sub(1);
        Some(value)
    }

    /// Ссылка по индексу.
    #[must_use]
    pub fn get(&self, index: GenerationalIndex) -> Option<&T> {
        let slot = self.slots.get(index.index as usize)?;
        if slot.generation != index.generation {
            return None;
        }
        slot.value.as_ref()
    }

    /// Мутабельная ссылка по индексу.
    pub fn get_mut(&mut self, index: GenerationalIndex) -> Option<&mut T> {
        let slot = self.slots.get_mut(index.index as usize)?;
        if slot.generation != index.generation {
            return None;
        }
        slot.value.as_mut()
    }

    /// Есть ли живое значение.
    #[must_use]
    pub fn contains(&self, index: GenerationalIndex) -> bool {
        self.get(index).is_some()
    }

    /// Число живых значений.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Пуст ли контейнер.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Итератор по живым значениям.
    pub fn iter(&self) -> impl Iterator<Item = (GenerationalIndex, &T)> {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            slot.value.as_ref().map(|value| {
                (
                    GenerationalIndex {
                        index: index as u32,
                        generation: slot.generation,
                    },
                    value,
                )
            })
        })
    }
}

impl<T> Default for SlotMap<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove() {
        let mut map = SlotMap::new();
        let index = map.insert(42);
        assert_eq!(map.get(index), Some(&42));
        assert_eq!(map.remove(index), Some(42));
        assert!(map.get(index).is_none());
    }

    #[test]
    fn stale_index_is_rejected_after_reuse() {
        let mut map = SlotMap::new();
        let first = map.insert("first");
        map.remove(first);
        let second = map.insert("second");

        assert_eq!(first.index(), second.index());
        assert_ne!(first.generation(), second.generation());
        assert!(map.get(first).is_none());
        assert_eq!(map.get(second), Some(&"second"));
    }

    #[test]
    fn iter_skips_removed() {
        let mut map = SlotMap::new();
        let a = map.insert(1);
        let _b = map.insert(2);
        map.remove(a);
        let values: Vec<i32> = map.iter().map(|(_, value)| *value).collect();
        assert_eq!(values, vec![2]);
    }
}
