//! Пул объектов фиксированной ёмкости.

/// Пул с повторным использованием слотов.
#[derive(Clone, Debug)]
pub struct PoolAllocator<T> {
    slots: Vec<Option<T>>,
    free: Vec<usize>,
    allocated: usize,
}

impl<T> PoolAllocator<T> {
    /// Создаёт пул на `capacity` слотов.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: (0..capacity).map(|_| None).collect(),
            free: (0..capacity).rev().collect(),
            allocated: 0,
        }
    }

    /// Занимает слот.
    pub fn acquire(&mut self, value: T) -> Option<usize> {
        let index = self.free.pop()?;
        self.slots[index] = Some(value);
        self.allocated += 1;
        Some(index)
    }

    /// Освобождает слот.
    pub fn release(&mut self, index: usize) -> Option<T> {
        let value = self.slots.get_mut(index)?.take()?;
        self.free.push(index);
        self.allocated = self.allocated.saturating_sub(1);
        Some(value)
    }

    /// Ссылка на значение.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&T> {
        self.slots.get(index)?.as_ref()
    }

    /// Мутабельная ссылка.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.slots.get_mut(index)?.as_mut()
    }

    /// Ёмкость.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// Занято слотов.
    #[must_use]
    pub fn allocated(&self) -> usize {
        self.allocated
    }

    /// Свободно слотов.
    #[must_use]
    pub fn free(&self) -> usize {
        self.free.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_release_reuses_slot() {
        let mut pool = PoolAllocator::with_capacity(2);
        let a = pool.acquire(1).unwrap();
        let b = pool.acquire(2).unwrap();
        assert!(pool.acquire(3).is_none());
        assert_eq!(pool.release(a), Some(1));
        assert_eq!(pool.acquire(3), Some(a));
        assert_eq!(pool.get(b), Some(&2));
        assert_eq!(pool.allocated(), 2);
    }
}
