//! I1: property-тесты ввода, AZERTY по scancodes и контракт `Intention`.
//!
//! Проверяются инварианты ADR-0031: deadzone не теряет диапазон, профиль
//! биндингов переживает RON-roundtrip, конфликты детерминированы, а
//! симуляция получает только `Intention`-снапшот.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use proptest::prelude::*;
use uuid::Uuid;

use flux_platform::input::curves::{AxisSettings, Deadzone, ResponseCurve};
use flux_platform::input::intention::{intention_from_input, IntentionActions, IntentionConfig};
use flux_platform::input::mapping::{
    ActionKind, AxisComponent, Binding, BindingSource, Chord, ChordBinding, InputAction,
    InputContext, InputMap, InputState, PlayerId,
};
use flux_platform::input::raw::{MouseButton, RawEvent, RawEventKind, RawQueue, Scancode};
use flux_platform::input::rebinding::{BindingProfile, ProfileBinding};
use flux_platform::Intention;

const MOVE: InputAction = InputAction::new(
    "move",
    Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0600),
    ActionKind::Axis2,
);
const SAVE: InputAction = InputAction::new(
    "save",
    Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0601),
    ActionKind::Button,
);

fn scancode() -> impl Strategy<Value = Scancode> {
    any::<u32>().prop_map(Scancode)
}

fn mouse_button() -> impl Strategy<Value = MouseButton> {
    prop_oneof![
        Just(MouseButton::Left),
        Just(MouseButton::Right),
        Just(MouseButton::Middle),
        any::<u16>().prop_map(MouseButton::Other),
    ]
}

fn binding_source() -> impl Strategy<Value = BindingSource> {
    prop_oneof![
        scancode().prop_map(BindingSource::Key),
        mouse_button().prop_map(BindingSource::Mouse),
        any::<u16>().prop_map(BindingSource::GamepadButton),
        (any::<u16>(), prop_oneof![Just(-1i8), Just(0), Just(1)])
            .prop_map(|(axis, sign)| BindingSource::GamepadAxis { axis, sign }),
    ]
}

fn action_kind() -> impl Strategy<Value = ActionKind> {
    prop_oneof![
        Just(ActionKind::Button),
        Just(ActionKind::Axis1),
        Just(ActionKind::Axis2),
        Just(ActionKind::Axis3),
    ]
}

fn profile_binding() -> impl Strategy<Value = ProfileBinding> {
    (
        any::<u128>(),
        "[a-zA-Z0-9_]{0,8}",
        action_kind(),
        binding_source(),
        any::<u8>(),
        "[a-zA-Z0-9_]{0,8}",
        prop_oneof![Just(-1.0f64), Just(0.0), Just(1.0)],
        any::<u8>(),
    )
        .prop_map(
            |(action, action_name, kind, source, player, context, sign, axis)| ProfileBinding {
                action: Uuid::from_u128(action),
                action_name,
                kind,
                source,
                player: PlayerId(player),
                context,
                component: AxisComponent { axis, sign },
            },
        )
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn deadzone_output_is_bounded(
        inner in 0.0f64..0.9,
        outer in 0.0f64..1.0,
        x in -2.0f64..2.0,
        y in -2.0f64..2.0,
    ) {
        let out = Deadzone::new(inner, outer).apply([x, y]);
        prop_assert!(out[0].hypot(out[1]) <= 1.0 + 1e-9);
    }

    #[test]
    fn deadzone_kills_inner_radius(
        inner in 0.0f64..0.9,
        x in -1.0f64..1.0,
        y in -1.0f64..1.0,
    ) {
        let deadzone = Deadzone::new(inner, 1.0);
        let length = x.hypot(y);
        prop_assume!(length <= deadzone.inner && length > 1e-6);
        prop_assert_eq!(deadzone.apply([x, y]), [0.0, 0.0]);
    }

    #[test]
    fn deadzone_preserves_direction(
        inner in 0.0f64..0.5,
        x in -1.0f64..1.0,
        y in -1.0f64..1.0,
    ) {
        let deadzone = Deadzone::new(inner, 1.0);
        let length = x.hypot(y);
        prop_assume!(length > inner + 1e-6 && length <= 1.0);
        let out = deadzone.apply([x, y]);
        let cross = out[0] * y - out[1] * x;
        prop_assert!(cross.abs() <= 1e-9 * (1.0 + length * length));
    }

    #[test]
    fn axis_settings_stay_in_unit_range(
        inner in 0.0f64..0.5,
        outer in 0.5f64..1.0,
        exponent in 1.0f64..4.0,
        sensitivity_x in -3.0f64..3.0,
        sensitivity_y in -3.0f64..3.0,
        invert_x in any::<bool>(),
        invert_y in any::<bool>(),
        x in -2.0f64..2.0,
        y in -2.0f64..2.0,
    ) {
        let settings = AxisSettings {
            deadzone: Deadzone::new(inner, outer),
            response: ResponseCurve::Exponential(exponent),
            sensitivity: [sensitivity_x, sensitivity_y],
            invert: [invert_x, invert_y],
        };
        let out = settings.apply([x, y]);
        prop_assert!((-1.0..=1.0).contains(&out[0]));
        prop_assert!((-1.0..=1.0).contains(&out[1]));
    }

    #[test]
    fn profile_roundtrips_through_ron(
        entries in prop::collection::vec(profile_binding(), 0..8),
    ) {
        let mut profile = BindingProfile::new();
        for entry in entries {
            profile.bind(entry);
        }
        let text = profile.to_ron().unwrap();
        let restored = BindingProfile::from_ron(&text).unwrap();
        prop_assert_eq!(profile, restored);
    }

    #[test]
    fn conflicts_are_sorted_and_minimal(
        entries in prop::collection::vec(profile_binding(), 0..12),
    ) {
        let mut profile = BindingProfile::new();
        for entry in entries {
            profile.bind(entry);
        }
        let conflicts = profile.detect_conflicts();
        for conflict in &conflicts {
            prop_assert!(conflict.actions.len() >= 2);
            let mut sorted = conflict.actions.clone();
            sorted.sort_unstable();
            sorted.dedup();
            prop_assert_eq!(sorted, conflict.actions.clone());
        }
        for pair in conflicts.windows(2) {
            let left = (pair[0].player, pair[0].context.clone(), pair[0].source);
            let right = (pair[1].player, pair[1].context.clone(), pair[1].source);
            prop_assert!(left < right);
        }
    }

    #[test]
    fn intention_axes_are_always_clamped(
        keys in prop::collection::vec(scancode(), 0..8),
        mouse_x in -1000.0f64..1000.0,
        mouse_y in -1000.0f64..1000.0,
    ) {
        let mut state = InputState::new();
        for key in keys {
            state.apply(&RawEvent::new(
                Duration::ZERO,
                RawEventKind::Key {
                    scancode: key,
                    pressed: true,
                    repeat: false,
                },
            ));
        }
        state.apply(&RawEvent::new(
            Duration::ZERO,
            RawEventKind::MouseMotion {
                dx: mouse_x,
                dy: mouse_y,
            },
        ));

        let mut map = InputMap::new();
        map.bind(Binding::new(
            MOVE,
            BindingSource::key(Scancode::W),
            PlayerId(0),
        ));
        map.bind(
            Binding::new(MOVE, BindingSource::key(Scancode::D), PlayerId(0))
                .with_component(AxisComponent::axis(0, 1.0)),
        );
        map.contexts_mut().push(InputContext::GAMEPLAY);

        let intention = intention_from_input(
            &map,
            &state,
            PlayerId(0),
            IntentionActions::movement_only(MOVE.id),
            IntentionConfig::default(),
        );
        prop_assert!(intention.move_axis.iter().all(|axis| (-1.0..=1.0).contains(axis)));
        prop_assert!(intention.look.iter().all(|axis| (-1.0..=1.0).contains(axis)));
    }
}

#[test]
fn azerty_physical_w_is_forward() {
    // На AZERTY клавиша, печатающая Z, находится на физической позиции W.
    // Mapping работает по scancode, поэтому «вперёд» остаётся «вперёд».
    let mut state = InputState::new();
    press(&mut state, Scancode::W);

    let mut map = InputMap::new();
    map.bind(
        Binding::new(MOVE, BindingSource::key(Scancode::W), PlayerId(0))
            .with_component(AxisComponent::axis(1, 1.0)),
    );
    map.contexts_mut().push(InputContext::GAMEPLAY);

    let intention = intention_from_input(
        &map,
        &state,
        PlayerId(0),
        IntentionActions::movement_only(MOVE.id),
        IntentionConfig::default(),
    );
    assert!(intention.move_axis[1] > 0.5);
}

#[test]
fn intention_is_a_copyable_send_sync_snapshot() {
    fn boundary<T: Copy + Default + Send + Sync + 'static>() {}
    boundary::<Intention>();
    assert!(Intention::IDLE.is_idle());
}

#[test]
fn intention_roundtrips_through_ron() {
    let intention = Intention {
        move_axis: [0.25, -0.5],
        look: [0.1, 0.0],
        jump: true,
        crouch: false,
    };
    let text = ron::ser::to_string(&intention).unwrap();
    let restored: Intention = ron::from_str(&text).unwrap();
    assert_eq!(intention, restored);
}

#[test]
fn raw_queue_is_drained_at_snapshot_boundary() {
    let mut queue = RawQueue::new();
    queue.push(RawEvent::new(
        Duration::ZERO,
        RawEventKind::Key {
            scancode: Scancode::W,
            pressed: true,
            repeat: false,
        },
    ));

    let mut state = InputState::new();
    for event in queue.take() {
        state.apply(&event);
    }
    assert!(queue.is_empty());
    assert!(state.key_pressed(Scancode::W));
}

#[test]
fn chord_public_api_triggers_action() {
    let mut state = InputState::new();
    press(&mut state, Scancode::LEFT_CTRL);
    press(&mut state, Scancode::S);

    let mut map = InputMap::new();
    map.bind_chord(ChordBinding::new(
        SAVE,
        Chord::new(Scancode::LEFT_CTRL, Scancode::S),
        PlayerId(0),
    ));
    map.contexts_mut().push(InputContext::GAMEPLAY);

    let values = map.resolve(&state, PlayerId(0));
    assert!(values.get(&SAVE.id).is_some_and(|value| value.as_bool()));
}
