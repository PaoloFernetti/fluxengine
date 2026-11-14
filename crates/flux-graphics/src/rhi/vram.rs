//! VRAM: лизинг памяти и бюджет.

use flux_core::memory::MemoryTag;

use super::error::RhiError;

/// Лизинг VRAM: один захват памяти под тегом.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VramLease {
    /// Идентификатор лизинга.
    pub id: u64,
    /// Байт.
    pub bytes: u64,
    /// Тег назначения.
    pub tag: MemoryTag,
}

/// Давление на бюджет VRAM.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Pressure {
    /// Норма.
    #[default]
    Normal,
    /// Повышенное.
    Elevated,
    /// Высокое.
    High,
    /// Критическое.
    Critical,
}

impl Pressure {
    /// Уровень по доле занятой памяти (`0.0..=1.0`).
    #[must_use]
    pub fn from_ratio(ratio: f32) -> Self {
        if ratio >= 0.95 {
            Self::Critical
        } else if ratio >= 0.85 {
            Self::High
        } else if ratio >= 0.70 {
            Self::Elevated
        } else {
            Self::Normal
        }
    }
}

/// Снимок бюджета VRAM.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BudgetSnapshot {
    /// Всего байт.
    pub total: u64,
    /// Зарезервировано (transient targets).
    pub reserved: u64,
    /// Занято лизингами.
    pub used: u64,
    /// Давление.
    pub pressure: Pressure,
}

impl BudgetSnapshot {
    /// Свободно байт.
    #[must_use]
    pub fn available(&self) -> u64 {
        self.total
            .saturating_sub(self.reserved)
            .saturating_sub(self.used)
    }

    /// Доля занятой памяти `0.0..=1.0`.
    #[must_use]
    pub fn utilization(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        let occupied = self.reserved.saturating_add(self.used);
        (occupied as f64 / self.total as f64).clamp(0.0, 1.0) as f32
    }
}

/// Аллокатор VRAM.
pub trait VramAllocator: Send {
    /// Захватывает `bytes` под тегом.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — нулевой размер;
    /// [`RhiError::OutOfMemory`] — бюджет исчерпан.
    fn allocate(&mut self, bytes: u64, tag: MemoryTag) -> Result<VramLease, RhiError>;

    /// Освобождает лизинг. `false` — лизинг неизвестен.
    fn free(&mut self, lease: VramLease) -> bool;

    /// Текущий снимок бюджета.
    fn budget(&self) -> BudgetSnapshot;
}
