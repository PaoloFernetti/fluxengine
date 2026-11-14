//! Питание и тепловое давление: подсказки для троттлинга.

use serde::{Deserialize, Serialize};

/// Тепловое давление ОС.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThermalPressure {
    /// Норма.
    #[default]
    Nominal,
    /// Небольшое.
    Fair,
    /// Серьёзное.
    Serious,
    /// Критическое.
    Critical,
}

/// Состояние питания.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PowerInfo {
    /// Работа от батареи.
    pub on_battery: bool,
    /// Заряд в процентах (если известен).
    pub battery_percent: Option<u8>,
    /// Тепловое давление.
    pub thermal: ThermalPressure,
}

impl PowerInfo {
    /// Нужно ли снижать нагрузку (батарея/тепло).
    #[must_use]
    pub fn should_throttle(self) -> bool {
        let low_battery =
            self.on_battery && self.battery_percent.is_some_and(|percent| percent <= 20);
        let hot = matches!(
            self.thermal,
            ThermalPressure::Serious | ThermalPressure::Critical
        );
        low_battery || hot
    }
}

/// Событие питания.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerEvent {
    /// Система уходит в сон.
    Suspending,
    /// Система проснулась.
    Resumed,
    /// Низкий заряд.
    BatteryLow,
    /// Смена теплового давления.
    ThermalChanged(ThermalPressure),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttling_follows_battery_and_thermal() {
        assert!(!PowerInfo::default().should_throttle());

        let low = PowerInfo {
            on_battery: true,
            battery_percent: Some(10),
            thermal: ThermalPressure::Nominal,
        };
        assert!(low.should_throttle());

        let plugged = PowerInfo {
            on_battery: false,
            battery_percent: Some(10),
            thermal: ThermalPressure::Nominal,
        };
        assert!(!plugged.should_throttle());

        let hot = PowerInfo {
            thermal: ThermalPressure::Critical,
            ..PowerInfo::default()
        };
        assert!(hot.should_throttle());
    }
}
