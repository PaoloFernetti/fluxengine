//! Ввод: четыре слоя (raw → mapping → rebinding/curves → intention-снапшот).
//!
//! Raw-события живут только здесь; симуляция читает [`Intention`]
//! (ADR-0031). Публичный API не требует окна: тесты и headless-профиль
//! подают события вручную.

pub mod curves;
pub mod intention;
pub mod mapping;
pub mod raw;
pub mod rebinding;

pub use curves::{AxisSettings, Deadzone, ResponseCurve};
pub use intention::{
    intention_from_input, intention_from_values, IntentionActions, IntentionConfig,
};
pub use mapping::{
    ActionKind, ActionValue, AxisComponent, Binding, BindingSource, Chord, ChordBinding,
    ContextStack, InputAction, InputContext, InputMap, InputState, PlayerId,
};
pub use raw::{MouseButton, RawEvent, RawEventKind, RawQueue, Scancode};
pub use rebinding::{detect_conflicts, BindingProfile, Conflict, ProfileBinding};

pub use flux_core::input::Intention;
