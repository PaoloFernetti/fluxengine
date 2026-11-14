//! Память: frame-арена, generational-контейнеры, пул, трекинг по тегам.

pub mod arena;
pub mod pool;
pub mod slotmap;
pub mod tracking;

pub use arena::FrameArena;
pub use pool::PoolAllocator;
pub use slotmap::{GenerationalIndex, SlotMap};
pub use tracking::{reset_tracking, track_alloc, track_free, tracked, MemoryTag};

/// Политика при исчерпании памяти.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OomPolicy {
    /// Паника: нарушение инварианта.
    Panic,
    /// Вернуть `None`/`null`: ожидаемый отказ.
    #[default]
    ReturnNull,
}
