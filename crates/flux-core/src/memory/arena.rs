//! Frame-арена: линейный аллокатор, очищаемый раз в кадр.
//!
//! После прогрева цикл `alloc`/`reset` не выполняет системных аллокаций:
//! буфер выделяется один раз в [`FrameArena::new`].

/// Линейный frame-аллокатор.
#[derive(Debug)]
pub struct FrameArena {
    buffer: Vec<u8>,
    offset: usize,
    allocations: u64,
    bytes_allocated: u64,
}

impl FrameArena {
    /// Создаёт арену с фиксированной ёмкостью.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![0; capacity],
            offset: 0,
            allocations: 0,
            bytes_allocated: 0,
        }
    }

    /// Выделяет `size` байт с выравниванием `align`.
    ///
    /// Возвращает `None`, если места не хватает или выравнивание не степень двойки.
    pub fn alloc(&mut self, size: usize, align: usize) -> Option<&mut [u8]> {
        let start = align_up(self.offset, align)?;
        let end = start.checked_add(size)?;
        if end > self.buffer.len() {
            return None;
        }

        self.offset = end;
        self.allocations = self.allocations.saturating_add(1);
        self.bytes_allocated = self.bytes_allocated.saturating_add(size as u64);
        Some(&mut self.buffer[start..end])
    }

    /// Сбрасывает арену (раз в кадр).
    pub fn reset(&mut self) {
        self.offset = 0;
    }

    /// Занято байт.
    #[must_use]
    pub fn used(&self) -> usize {
        self.offset
    }

    /// Ёмкость.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }

    /// Число логических выделений (диагностика).
    #[must_use]
    pub fn allocations(&self) -> u64 {
        self.allocations
    }

    /// Суммарно выделено байт (диагностика).
    #[must_use]
    pub fn bytes_allocated(&self) -> u64 {
        self.bytes_allocated
    }
}

fn align_up(value: usize, align: usize) -> Option<usize> {
    if !align.is_power_of_two() {
        return None;
    }
    value.checked_add(align - 1).map(|v| v & !(align - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_is_aligned_and_bounded() {
        let mut arena = FrameArena::new(64);
        let block = arena.alloc(8, 16).unwrap();
        assert_eq!(block.len(), 8);
        assert_eq!(block.as_ptr() as usize % 16, 0);
        assert!(arena.alloc(100, 8).is_none());
    }

    #[test]
    fn reset_reuses_memory() {
        let mut arena = FrameArena::new(64);
        let first = arena.alloc(32, 8).unwrap().as_ptr() as usize;
        arena.reset();
        let second = arena.alloc(32, 8).unwrap().as_ptr() as usize;
        assert_eq!(first, second);
        assert_eq!(arena.used(), 32);
    }

    #[test]
    fn rejects_non_power_of_two_alignment() {
        let mut arena = FrameArena::new(64);
        assert!(arena.alloc(8, 3).is_none());
    }

    #[test]
    fn thousand_cycles_stay_within_capacity() {
        let mut arena = FrameArena::new(4096);
        for _ in 0..1_000 {
            for _ in 0..10 {
                assert!(arena.alloc(16, 16).is_some());
            }
            arena.reset();
        }
        assert_eq!(arena.used(), 0);
    }
}
