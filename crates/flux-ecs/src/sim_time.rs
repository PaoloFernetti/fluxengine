//! `SimTime` как компонент мира (WP 2.8).
//!
//! Тик — детерминированный счётчик; доля суток квантуется в 16 бит
//! (`day_fraction_q16`), чтобы одинаковая последовательность тиков давала
//! одинаковое значение в дельте сейва. Источник истины — `flux_core::time::SimTime`;
//! компонент хранит только сериализуемый снимок.

use flux_core::time::SimTick;

/// Тиков в игровых сутках при шаге 60 Гц.
pub const TICKS_PER_DAY: u64 = 86_400 * 60;

/// Компонент времени симуляции: тик и квантованная доля суток.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    flux_macros::Reflect,
)]
#[reflect(type_uuid = "6b1c2f0e-9a4d-4c8b-9f2e-0a1b2c3d4e5f")]
pub struct SimTimeComponent {
    /// Абсолютный тик симуляции.
    #[save]
    pub ticks: u64,
    /// Доля суток: `0..65535` (16 бит на сутки).
    #[save]
    pub day_fraction_q16: u32,
}

impl SimTimeComponent {
    /// Снимок времени по тику.
    #[must_use]
    pub fn from_tick(tick: SimTick) -> Self {
        let ticks = tick.get();
        let within_day = ticks % TICKS_PER_DAY;
        let fraction = (u128::from(within_day) * 65_536) / u128::from(TICKS_PER_DAY);
        Self {
            ticks,
            day_fraction_q16: u32::try_from(fraction).unwrap_or(0),
        }
    }

    /// Абсолютный тик.
    #[must_use]
    pub const fn tick(&self) -> u64 {
        self.ticks
    }

    /// Доля суток в 1/65536.
    #[must_use]
    pub const fn day_fraction_q16(&self) -> u32 {
        self.day_fraction_q16
    }

    /// Продвигает время на `steps` тиков.
    pub fn advance(&mut self, steps: u64) {
        let mut tick = SimTick::ZERO;
        tick.advance(self.ticks);
        tick.advance(steps);
        *self = Self::from_tick(tick);
    }

    /// Устанавливает время по тику.
    pub fn set_tick(&mut self, tick: SimTick) {
        *self = Self::from_tick(tick);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;

    #[test]
    fn component_is_deterministic_and_quantized() {
        let first = SimTimeComponent::from_tick(SimTick::ZERO);
        let second = SimTimeComponent::from_tick(SimTick::ZERO);
        assert_eq!(first, second);
        assert_eq!(first.ticks, 0);
        assert_eq!(first.day_fraction_q16, 0);

        let mut tick = SimTick::ZERO;
        tick.advance(TICKS_PER_DAY / 2);
        let half = SimTimeComponent::from_tick(tick);
        assert_eq!(half.day_fraction_q16, 32_768);

        tick.advance(TICKS_PER_DAY);
        let next_day = SimTimeComponent::from_tick(tick);
        assert_eq!(next_day.day_fraction_q16, 32_768);
    }

    #[test]
    fn advance_matches_from_tick() {
        let mut component = SimTimeComponent::default();
        component.advance(123);
        let mut tick = SimTick::ZERO;
        tick.advance(123);
        assert_eq!(component, SimTimeComponent::from_tick(tick));

        component.set_tick(tick);
        assert_eq!(component.tick(), 123);
    }

    #[test]
    fn schema_marks_fields_for_save() {
        let schema = SimTimeComponent::schema();
        assert_eq!(schema.fields.len(), 2);
        assert_eq!(schema.save_fields().count(), 2);
        assert_eq!(schema.replicated_fields().count(), 0);
        assert_eq!(schema.fields[0].ty, crate::schema::FieldType::U64);
        assert_eq!(schema.fields[1].ty, crate::schema::FieldType::U32);
    }
}
