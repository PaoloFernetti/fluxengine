//! Трекинг памяти по тегам (диагностика, dev-overlay).

use std::sync::atomic::{AtomicUsize, Ordering};

/// Тег назначения памяти.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemoryTag {
    /// Frame-арена.
    Frame,
    /// Ассеты.
    Assets,
    /// Рендер.
    Rendering,
    /// Физика.
    Physics,
    /// Аудио.
    Audio,
    /// Сеть.
    Network,
    /// Редактор.
    Editor,
    /// Прочее.
    Other,
}

impl MemoryTag {
    /// Все теги.
    pub const ALL: [Self; 8] = [
        Self::Frame,
        Self::Assets,
        Self::Rendering,
        Self::Physics,
        Self::Audio,
        Self::Network,
        Self::Editor,
        Self::Other,
    ];

    const fn index(self) -> usize {
        match self {
            Self::Frame => 0,
            Self::Assets => 1,
            Self::Rendering => 2,
            Self::Physics => 3,
            Self::Audio => 4,
            Self::Network => 5,
            Self::Editor => 6,
            Self::Other => 7,
        }
    }
}

static COUNTERS: [AtomicUsize; 8] = [const { AtomicUsize::new(0) }; 8];

/// Учитывает выделение.
pub fn track_alloc(tag: MemoryTag, bytes: usize) {
    COUNTERS[tag.index()].fetch_add(bytes, Ordering::Relaxed);
}

/// Учитывает освобождение (насыщающе).
pub fn track_free(tag: MemoryTag, bytes: usize) {
    let counter = &COUNTERS[tag.index()];
    let mut current = counter.load(Ordering::Relaxed);
    loop {
        let next = current.saturating_sub(bytes);
        match counter.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

/// Сколько байт учитывается по тегу.
#[must_use]
pub fn tracked(tag: MemoryTag) -> usize {
    COUNTERS[tag.index()].load(Ordering::Relaxed)
}

/// Сбрасывает все счётчики.
pub fn reset_tracking() {
    for counter in &COUNTERS {
        counter.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracking_accumulates_and_saturates() {
        reset_tracking();
        track_alloc(MemoryTag::Assets, 100);
        track_alloc(MemoryTag::Assets, 50);
        assert_eq!(tracked(MemoryTag::Assets), 150);

        track_free(MemoryTag::Assets, 500);
        assert_eq!(tracked(MemoryTag::Assets), 0);
        assert_eq!(tracked(MemoryTag::Rendering), 0);
    }
}
