//! Окно, display, input, системные сервисы.
//!
//! Headless по умолчанию: `winit` и `gilrs` включаются feature-гейтами
//! (`windowing`, `devices`), per-OS дампы — `crash-dump` (ADR-0033).
//!
//! Raw-события ввода не попадают в ECS: на границе тика они превращаются
//! в [`Intention`] (ADR-0031). Канон: `docs/canon.md`. План:
//! `docs/architecture.md`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod clipboard;
pub mod crash;
pub mod cursor;
pub mod devices;
pub mod display;
pub mod input;
pub mod paths;
pub mod power;
pub mod text;
pub mod window;

pub use flux_core::input::Intention;

pub use display::{DpiInfo, LogicalSize, MonitorInfo, PhysicalSize, VideoMode};
pub use input::{
    ActionKind, ActionValue, AxisSettings, Binding, BindingProfile, BindingSource, Chord,
    ChordBinding, Conflict, ContextStack, Deadzone, InputAction, InputContext, InputMap,
    InputState, IntentionActions, IntentionConfig, MouseButton, PlayerId, RawEvent, RawEventKind,
    RawQueue, ResponseCurve, Scancode,
};
pub use window::{
    CloseDecision, CloseRequest, WindowEvent, WindowEventQueue, WindowId, WindowMetrics, WindowMode,
};
