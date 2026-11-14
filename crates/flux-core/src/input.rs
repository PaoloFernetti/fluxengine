//! Контракт ввода: `Intention` — единственный вход симуляции (ADR-0031).
//!
//! Платформа формирует снапшот на тик, физика (KCC) и AI читают его.
//! Raw-события сюда не попадают: частота кадров не влияет на симуляцию.

use serde::{Deserialize, Serialize};

/// Намерение игрока/бота/AI на один тик.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Intention {
    /// Направление движения в локальной плоскости (`-1..1` по осям).
    pub move_axis: [f64; 2],
    /// Дельта обзора (`-1..1` по осям), уже после мёртвой зоны.
    pub look: [f64; 2],
    /// Прыжок в этом тике.
    pub jump: bool,
    /// Приседание.
    pub crouch: bool,
}

impl Intention {
    /// Нейтральное намерение.
    pub const IDLE: Self = Self {
        move_axis: [0.0, 0.0],
        look: [0.0, 0.0],
        jump: false,
        crouch: false,
    };

    /// Ограничивает оси диапазоном `-1..1` (защита от ботов и сети).
    #[must_use]
    pub fn clamped(self) -> Self {
        Self {
            move_axis: [
                self.move_axis[0].clamp(-1.0, 1.0),
                self.move_axis[1].clamp(-1.0, 1.0),
            ],
            look: [self.look[0].clamp(-1.0, 1.0), self.look[1].clamp(-1.0, 1.0)],
            jump: self.jump,
            crouch: self.crouch,
        }
    }

    /// Нет ли активных действий.
    #[must_use]
    pub fn is_idle(self) -> bool {
        self.move_axis == [0.0, 0.0] && self.look == [0.0, 0.0] && !self.jump && !self.crouch
    }

    /// Длина вектора движения (для анимации/скорости).
    #[must_use]
    pub fn move_length(self) -> f64 {
        self.move_axis[0].hypot(self.move_axis[1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_is_neutral() {
        assert!(Intention::IDLE.is_idle());
        assert_eq!(Intention::default(), Intention::IDLE);
        assert_eq!(Intention::IDLE.move_length(), 0.0);
    }

    #[test]
    fn clamping_limits_axes() {
        let raw = Intention {
            move_axis: [2.0, -3.0],
            look: [-5.0, 0.5],
            jump: true,
            crouch: false,
        };
        let clamped = raw.clamped();
        assert_eq!(clamped.move_axis, [1.0, -1.0]);
        assert_eq!(clamped.look, [-1.0, 0.5]);
        assert!(clamped.jump);
        assert!(!clamped.is_idle());
    }

    #[test]
    fn move_length_is_euclidean() {
        let intention = Intention {
            move_axis: [0.6, 0.8],
            ..Intention::IDLE
        };
        assert!((intention.move_length() - 1.0).abs() < 1e-12);
    }
}
