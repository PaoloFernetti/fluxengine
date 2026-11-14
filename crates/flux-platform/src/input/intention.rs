//! Intention-снапшот: единственный вход симуляции (ADR-0031).
//!
//! Конвертер не хранит состояния: `InputState` + `InputMap` → [`Intention`].
//! Бот детерминизм-харнесса создаёт `Intention` напрямую, минуя raw-слой.

use std::collections::BTreeMap;

use uuid::Uuid;

use flux_core::input::Intention;

use super::curves::{AxisSettings, Deadzone};
use super::mapping::{ActionValue, InputMap, InputState, PlayerId};

/// UUID действий, из которых собирается `Intention`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentionActions {
    /// Действие движения (`Axis2`: `[strafe, forward]`).
    pub movement: Uuid,
    /// Действие обзора (`Axis2`); `None` — обзор берётся из дельты мыши.
    pub look: Option<Uuid>,
    /// Действие прыжка.
    pub jump: Option<Uuid>,
    /// Действие приседания.
    pub crouch: Option<Uuid>,
}

impl IntentionActions {
    /// Только движение (минимальный набор).
    #[must_use]
    pub const fn movement_only(movement: Uuid) -> Self {
        Self {
            movement,
            look: None,
            jump: None,
            crouch: None,
        }
    }
}

/// Настройки конвертера: кривые осей и чувствительность мыши.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IntentionConfig {
    /// Кривая движения (deadzone + rescale + sensitivity).
    pub movement: AxisSettings,
    /// Кривая обзора для действий (для мыши deadzone не применяется).
    pub look: AxisSettings,
    /// Чувствительность мыши: пиксели → `-1..1`.
    pub mouse_sensitivity: [f64; 2],
}

impl Default for IntentionConfig {
    fn default() -> Self {
        Self {
            movement: AxisSettings::default(),
            look: AxisSettings {
                deadzone: Deadzone::new(0.0, 1.0),
                ..AxisSettings::default()
            },
            mouse_sensitivity: [0.1, 0.1],
        }
    }
}

/// Собирает `Intention` из уже разрешённых действий.
#[must_use]
pub fn intention_from_values(
    values: &BTreeMap<Uuid, ActionValue>,
    actions: IntentionActions,
    config: IntentionConfig,
) -> Intention {
    let movement = values
        .get(&actions.movement)
        .map_or([0.0; 2], |value| config.movement.apply(value.as_axis2()));
    let look = actions
        .look
        .and_then(|id| values.get(&id))
        .map_or([0.0; 2], |value| config.look.apply(value.as_axis2()));

    Intention {
        move_axis: movement,
        look,
        jump: is_active(values, actions.jump),
        crouch: is_active(values, actions.crouch),
    }
    .clamped()
}

/// Разрешает действия игрока и собирает `Intention`.
///
/// Если действие обзора не объявлено, `look` заполняется дельтой мыши
/// за кадр, умноженной на [`IntentionConfig::mouse_sensitivity`].
#[must_use]
pub fn intention_from_input(
    map: &InputMap,
    state: &InputState,
    player: PlayerId,
    actions: IntentionActions,
    config: IntentionConfig,
) -> Intention {
    let values = map.resolve(state, player);
    let mut intention = intention_from_values(&values, actions, config);
    if actions.look.is_none() {
        let delta = state.mouse_delta();
        intention.look = [
            (delta[0] * config.mouse_sensitivity[0]).clamp(-1.0, 1.0),
            (delta[1] * config.mouse_sensitivity[1]).clamp(-1.0, 1.0),
        ];
    }
    intention.clamped()
}

fn is_active(values: &BTreeMap<Uuid, ActionValue>, action: Option<Uuid>) -> bool {
    action
        .and_then(|id| values.get(&id))
        .is_some_and(|value| value.as_bool())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::input::mapping::{ActionKind, AxisComponent, Binding, BindingSource, InputAction};
    use crate::input::raw::{RawEvent, RawEventKind, Scancode};

    const MOVE: InputAction = InputAction::new(
        "move",
        Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0500),
        ActionKind::Axis2,
    );
    const JUMP: InputAction = InputAction::new(
        "jump",
        Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0501),
        ActionKind::Button,
    );

    fn wasd_map() -> InputMap {
        let mut map = InputMap::new();
        for (scancode, axis, sign) in [
            (Scancode::W, 1, 1.0),
            (Scancode::S, 1, -1.0),
            (Scancode::D, 0, 1.0),
            (Scancode::A, 0, -1.0),
        ] {
            map.bind(
                Binding::new(MOVE, BindingSource::key(scancode), PlayerId(0))
                    .with_component(AxisComponent::axis(axis, sign)),
            );
        }
        map.bind(Binding::new(
            JUMP,
            BindingSource::key(Scancode::SPACE),
            PlayerId(0),
        ));
        map
    }

    fn press(state: &mut InputState, scancode: Scancode) {
        state.apply(&RawEvent::new(
            Duration::ZERO,
            RawEventKind::Key {
                scancode,
                pressed: true,
                repeat: false,
            },
        ));
    }

    #[test]
    fn wasd_becomes_intention_with_normalized_diagonal() {
        let mut state = InputState::new();
        press(&mut state, Scancode::W);
        press(&mut state, Scancode::D);

        let intention = intention_from_input(
            &wasd_map(),
            &state,
            PlayerId(0),
            IntentionActions {
                movement: MOVE.id,
                look: None,
                jump: Some(JUMP.id),
                crouch: None,
            },
            IntentionConfig::default(),
        );

        let expected = std::f64::consts::FRAC_1_SQRT_2;
        assert!((intention.move_axis[0] - expected).abs() < 1e-12);
        assert!((intention.move_axis[1] - expected).abs() < 1e-12);
        assert!(!intention.jump);
    }

    #[test]
    fn mouse_delta_feeds_look_when_no_action() {
        let mut state = InputState::new();
        state.apply(&RawEvent::new(
            Duration::ZERO,
            RawEventKind::MouseMotion { dx: 5.0, dy: -2.0 },
        ));

        let intention = intention_from_input(
            &InputMap::new(),
            &state,
            PlayerId(0),
            IntentionActions::movement_only(MOVE.id),
            IntentionConfig::default(),
        );
        assert!((intention.look[0] - 0.5).abs() < 1e-12);
        assert!((intention.look[1] + 0.2).abs() < 1e-12);
    }

    #[test]
    fn jump_action_sets_flag() {
        let mut state = InputState::new();
        press(&mut state, Scancode::SPACE);

        let intention = intention_from_input(
            &wasd_map(),
            &state,
            PlayerId(0),
            IntentionActions {
                movement: MOVE.id,
                look: None,
                jump: Some(JUMP.id),
                crouch: None,
            },
            IntentionConfig::default(),
        );
        assert!(intention.jump);
        assert!(!intention.crouch);
    }
}
