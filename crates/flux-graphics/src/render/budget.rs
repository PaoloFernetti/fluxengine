//! Бюджет VRAM: резерв, занятость и pressure-лестница с гистерезисом.
//!
//! Render Graph резервирует transient-таргеты, TransferSystem — загрузки,
//! а `VramAllocator` сообщает фактическую занятость. Гистерезис не даёт
//! давлению «дребезжать» на границе уровня.

use super::super::rhi::{BudgetSnapshot, Pressure, RhiError};

/// Ширина гистерезиса по умолчанию (5 процентных пунктов).
pub const DEFAULT_HYSTERESIS: f32 = 0.05;

/// Бюджет VRAM с pressure-лестницей.
#[derive(Clone, Debug)]
pub struct VramBudget {
    total: u64,
    reserved: u64,
    used: u64,
    pressure: Pressure,
    hysteresis: f32,
}

impl VramBudget {
    /// Бюджет с гистерезисом по умолчанию.
    #[must_use]
    pub fn new(total: u64) -> Self {
        Self::with_hysteresis(total, DEFAULT_HYSTERESIS)
    }

    /// Бюджет с явным гистерезисом (`0.0..1.0`; вне диапазона — значение по умолчанию).
    #[must_use]
    pub fn with_hysteresis(total: u64, hysteresis: f32) -> Self {
        let hysteresis = if (0.0..1.0).contains(&hysteresis) {
            hysteresis
        } else {
            DEFAULT_HYSTERESIS
        };
        Self {
            total,
            reserved: 0,
            used: 0,
            pressure: Pressure::Normal,
            hysteresis,
        }
    }

    /// Всего байт бюджета.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Зарезервировано transient-таргетов, байт.
    #[must_use]
    pub fn reserved(&self) -> u64 {
        self.reserved
    }

    /// Занято лизингами/ресурсами, байт.
    #[must_use]
    pub fn used(&self) -> u64 {
        self.used
    }

    /// Текущее давление.
    #[must_use]
    pub fn pressure(&self) -> Pressure {
        self.pressure
    }

    /// Снимок для логов и GraphicsSettings.
    #[must_use]
    pub fn snapshot(&self) -> BudgetSnapshot {
        BudgetSnapshot {
            total: self.total,
            reserved: self.reserved,
            used: self.used,
            pressure: self.pressure,
        }
    }

    /// Резервирует память под transient-таргет.
    ///
    /// # Errors
    ///
    /// [`RhiError::Validation`] — нулевой резерв;
    /// [`RhiError::OutOfMemory`] — бюджет исчерпан.
    pub fn reserve(&mut self, bytes: u64) -> Result<(), RhiError> {
        if bytes == 0 {
            return Err(RhiError::validation("zero-sized VRAM reservation"));
        }
        let occupied = self
            .used
            .saturating_add(self.reserved)
            .saturating_add(bytes);
        if occupied > self.total {
            return Err(RhiError::OutOfMemory { requested: bytes });
        }
        self.reserved = self.reserved.saturating_add(bytes);
        self.recompute();
        Ok(())
    }

    /// Освобождает резерв.
    pub fn release_reserved(&mut self, bytes: u64) {
        self.reserved = self.reserved.saturating_sub(bytes);
        self.recompute();
    }

    /// Обновляет фактическую занятость из `VramAllocator`.
    pub fn record_used(&mut self, used: u64) {
        self.used = used;
        self.recompute();
    }

    /// Хватает ли бюджета на `bytes` с учётом резерва.
    #[must_use]
    pub fn can_allocate(&self, bytes: u64) -> bool {
        self.used
            .saturating_add(self.reserved)
            .saturating_add(bytes)
            <= self.total
    }

    /// Пора ли вытеснять ресурсы (High/Critical).
    #[must_use]
    pub fn should_evict(&self) -> bool {
        matches!(self.pressure, Pressure::High | Pressure::Critical)
    }

    fn ratio(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        let occupied = self.used.saturating_add(self.reserved);
        (occupied as f64 / self.total as f64).clamp(0.0, 1.0) as f32
    }

    fn recompute(&mut self) {
        let ratio = self.ratio();
        let desired = Pressure::from_ratio(ratio);
        if severity(desired) > severity(self.pressure) {
            self.pressure = desired;
        } else if severity(desired) < severity(self.pressure) {
            let floor = match self.pressure {
                Pressure::Critical => 0.95 - self.hysteresis,
                Pressure::High => 0.85 - self.hysteresis,
                Pressure::Elevated => 0.70 - self.hysteresis,
                Pressure::Normal => 0.0,
            };
            if ratio <= floor {
                self.pressure = desired;
            }
        }
    }
}

const fn severity(pressure: Pressure) -> u8 {
    match pressure {
        Pressure::Normal => 0,
        Pressure::Elevated => 1,
        Pressure::High => 2,
        Pressure::Critical => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservation_respects_total() {
        let mut budget = VramBudget::new(100);
        budget.reserve(60).unwrap();
        assert_eq!(budget.reserved(), 60);
        assert!(budget.reserve(41).is_err());
        budget.record_used(40);
        assert!(budget.reserve(1).is_err());
        budget.release_reserved(60);
        assert_eq!(budget.reserved(), 0);
    }

    #[test]
    fn pressure_upgrades_immediately() {
        let mut budget = VramBudget::new(1000);
        budget.record_used(700);
        assert_eq!(budget.pressure(), Pressure::Elevated);
        budget.record_used(850);
        assert_eq!(budget.pressure(), Pressure::High);
        budget.record_used(950);
        assert_eq!(budget.pressure(), Pressure::Critical);
        assert!(budget.should_evict());
    }

    #[test]
    fn pressure_downgrades_only_after_hysteresis() {
        let mut budget = VramBudget::new(1000);
        budget.record_used(950);
        assert_eq!(budget.pressure(), Pressure::Critical);
        budget.record_used(910);
        assert_eq!(budget.pressure(), Pressure::Critical);
        budget.record_used(900);
        assert_eq!(budget.pressure(), Pressure::High);
        budget.record_used(810);
        assert_eq!(budget.pressure(), Pressure::High);
        budget.record_used(800);
        assert_eq!(budget.pressure(), Pressure::Elevated);
        budget.record_used(660);
        assert_eq!(budget.pressure(), Pressure::Elevated);
        budget.record_used(650);
        assert_eq!(budget.pressure(), Pressure::Normal);
    }

    #[test]
    fn snapshot_matches_state() {
        let mut budget = VramBudget::new(200);
        budget.reserve(50).unwrap();
        budget.record_used(100);
        let snapshot = budget.snapshot();
        assert_eq!(snapshot.total, 200);
        assert_eq!(snapshot.reserved, 50);
        assert_eq!(snapshot.used, 100);
        assert_eq!(snapshot.pressure, Pressure::Elevated);
        assert!(!budget.can_allocate(51));
        assert!(budget.can_allocate(50));
    }
}
