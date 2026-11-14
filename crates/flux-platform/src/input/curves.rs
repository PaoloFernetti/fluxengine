//! Кривые отклика: радиальный deadzone + rescale, response curve, sensitivity.
//!
//! Радиальный deadzone не теряет диапазон: вход за мёртвой зоной
//! перенормируется в `0..1` (rescale), а не обрезается. Это требование G3
//! (ADR-0031).

use serde::{Deserialize, Serialize};

/// Радиальная мёртвая зона стика.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Deadzone {
    /// Внутренний радиус: меньше — ноль.
    pub inner: f64,
    /// Внешний радиус: больше — полная величина.
    pub outer: f64,
}

impl Default for Deadzone {
    fn default() -> Self {
        Self {
            inner: 0.1,
            outer: 1.0,
        }
    }
}

impl Deadzone {
    /// Мёртвая зона с проверкой порядка.
    #[must_use]
    pub fn new(inner: f64, outer: f64) -> Self {
        let inner = inner.clamp(0.0, 1.0);
        let outer = outer.clamp(inner.max(1e-6), 1.0);
        Self { inner, outer }
    }

    /// Применяет радиальный deadzone с rescale к вектору.
    #[must_use]
    pub fn apply(&self, value: [f64; 2]) -> [f64; 2] {
        let length = value[0].hypot(value[1]);
        if length <= self.inner || length == 0.0 {
            return [0.0, 0.0];
        }
        let span = self.outer - self.inner;
        let normalized = if span <= f64::EPSILON {
            1.0
        } else {
            ((length - self.inner) / span).clamp(0.0, 1.0)
        };
        let scale = normalized / length;
        [value[0] * scale, value[1] * scale]
    }
}

/// Кривая отклика оси.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum ResponseCurve {
    /// Линейная.
    #[default]
    Linear,
    /// Степенная: `sign(x) * |x|^exponent` (exponent > 1 — мягче у центра).
    Exponential(f64),
}

impl ResponseCurve {
    /// Применяет кривую к значению `-1..1`.
    #[must_use]
    pub fn apply(self, value: f64) -> f64 {
        match self {
            Self::Linear => value,
            Self::Exponential(exponent) => {
                let exponent = exponent.max(1e-6);
                value.signum() * value.abs().powf(exponent)
            }
        }
    }
}

/// Настройки оси действия.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AxisSettings {
    /// Мёртвая зона.
    pub deadzone: Deadzone,
    /// Кривая отклика.
    pub response: ResponseCurve,
    /// Чувствительность по осям.
    pub sensitivity: [f64; 2],
    /// Инверсия по осям.
    pub invert: [bool; 2],
}

impl Default for AxisSettings {
    fn default() -> Self {
        Self {
            deadzone: Deadzone::default(),
            response: ResponseCurve::Linear,
            sensitivity: [1.0, 1.0],
            invert: [false, false],
        }
    }
}

impl AxisSettings {
    /// Полный конвейер: deadzone → response → sensitivity → invert.
    #[must_use]
    pub fn apply(&self, value: [f64; 2]) -> [f64; 2] {
        let deadzoned = self.deadzone.apply(value);
        let mut result = [0.0; 2];
        for axis in 0..2 {
            let mut axis_value = self.response.apply(deadzoned[axis]) * self.sensitivity[axis];
            if self.invert[axis] {
                axis_value = -axis_value;
            }
            result[axis] = axis_value.clamp(-1.0, 1.0);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadzone_rescales_without_losing_range() {
        let deadzone = Deadzone::new(0.2, 1.0);
        assert_eq!(deadzone.apply([0.1, 0.0]), [0.0, 0.0]);

        let rescaled = deadzone.apply([0.6, 0.0]);
        // (0.6 - 0.2) / 0.8 = 0.5 — половина полного диапазона, не 0.4.
        assert!((rescaled[0] - 0.5).abs() < 1e-12);

        let full = deadzone.apply([1.0, 0.0]);
        assert!((full[0] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn deadzone_preserves_direction() {
        let deadzone = Deadzone::new(0.1, 0.9);
        let out = deadzone.apply([0.3, 0.3]);
        let ratio = out[0] / out[1];
        assert!((ratio - 1.0).abs() < 1e-12);
    }

    #[test]
    fn response_curve_keeps_sign() {
        let curve = ResponseCurve::Exponential(2.0);
        assert!((curve.apply(0.5) - 0.25).abs() < 1e-12);
        assert!((curve.apply(-0.5) + 0.25).abs() < 1e-12);
        assert_eq!(ResponseCurve::Linear.apply(-0.3), -0.3);
    }

    #[test]
    fn axis_settings_pipeline() {
        let settings = AxisSettings {
            deadzone: Deadzone::new(0.0, 1.0),
            response: ResponseCurve::Linear,
            sensitivity: [2.0, 1.0],
            invert: [false, true],
        };
        let out = settings.apply([0.25, 0.5]);
        assert!((out[0] - 0.5).abs() < 1e-12);
        assert!((out[1] + 0.5).abs() < 1e-12);
    }
}
