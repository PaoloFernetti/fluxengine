//! `winit` → платформенные типы (feature `windowing`, ADR-0033).
//!
//! Адаптер не создаёт event loop: он переводит события `winit` в
//! [`RawEvent`]/[`WindowEvent`] и применяет режим курсора. Это позволяет
//! тестировать конверсии без окна и держать headless-сборку чистой.

use std::time::Duration;

use winit::dpi::PhysicalPosition;
use winit::event::{
    ElementState, Ime, KeyEvent, MouseButton as WinitMouseButton, MouseScrollDelta,
    WindowEvent as WinitWindowEvent,
};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window};

use crate::cursor::CursorMode;
use crate::display::PhysicalSize;
use crate::input::raw::{MouseButton, RawEvent, RawEventKind, Scancode};
use crate::window::{CloseRequest, WindowEvent};

/// Физический код `winit` → `Scancode`.
#[must_use]
pub fn map_key_code(code: KeyCode) -> Option<Scancode> {
    match code {
        KeyCode::KeyW => Some(Scancode::W),
        KeyCode::KeyA => Some(Scancode::A),
        KeyCode::KeyS => Some(Scancode::S),
        KeyCode::KeyD => Some(Scancode::D),
        KeyCode::Space => Some(Scancode::SPACE),
        KeyCode::ControlLeft => Some(Scancode::LEFT_CTRL),
        KeyCode::Escape => Some(Scancode::ESCAPE),
        KeyCode::F1 => Some(Scancode::F1),
        _ => None,
    }
}

/// Кнопка `winit` → `MouseButton`.
#[must_use]
pub fn map_mouse_button(button: WinitMouseButton) -> Option<MouseButton> {
    match button {
        WinitMouseButton::Left => Some(MouseButton::Left),
        WinitMouseButton::Right => Some(MouseButton::Right),
        WinitMouseButton::Middle => Some(MouseButton::Middle),
        WinitMouseButton::Back => Some(MouseButton::Other(3)),
        WinitMouseButton::Forward => Some(MouseButton::Other(4)),
        WinitMouseButton::Other(code) => Some(MouseButton::Other(code)),
    }
}

/// Режим курсора → `winit` grab mode.
#[must_use]
pub fn cursor_grab_mode(mode: CursorMode) -> CursorGrabMode {
    match mode {
        CursorMode::Normal | CursorMode::Hidden => CursorGrabMode::None,
        CursorMode::Disabled => CursorGrabMode::Locked,
        CursorMode::Confined => CursorGrabMode::Confined,
    }
}

/// Применяет режим курсора к окну.
///
/// # Errors
///
/// Возвращает ошибку платформы, если захват курсора недоступен.
pub fn apply_cursor_mode(
    window: &Window,
    mode: CursorMode,
) -> Result<(), winit::error::ExternalError> {
    window.set_cursor_visible(mode.is_visible());
    window.set_cursor_grab(cursor_grab_mode(mode))
}

/// Состояние адаптера: предыдущая позиция курсора для дельты.
#[derive(Debug, Default)]
pub struct WinitInputAdapter {
    last_cursor: Option<(f64, f64)>,
}

impl WinitInputAdapter {
    /// Новый адаптер.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Оконное событие → платформенное.
    #[must_use]
    pub fn window_event(&mut self, event: &WinitWindowEvent) -> Option<WindowEvent> {
        match event {
            WinitWindowEvent::Resized(size) => Some(WindowEvent::Resize(PhysicalSize::new(
                size.width,
                size.height,
            ))),
            WinitWindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                Some(WindowEvent::ScaleFactorChanged(*scale_factor))
            }
            WinitWindowEvent::Focused(focused) => Some(WindowEvent::Focus(*focused)),
            WinitWindowEvent::CloseRequested => {
                Some(WindowEvent::CloseRequest(CloseRequest::new()))
            }
            WinitWindowEvent::Moved(position) => Some(WindowEvent::Moved {
                x: position.x,
                y: position.y,
            }),
            _ => None,
        }
    }

    /// Клавиша → raw-событие.
    #[must_use]
    pub fn key_event(&mut self, event: &KeyEvent, timestamp: Duration) -> Option<RawEvent> {
        let PhysicalKey::Code(code) = event.physical_key else {
            return None;
        };
        let scancode = map_key_code(code)?;
        Some(RawEvent::new(
            timestamp,
            RawEventKind::Key {
                scancode,
                pressed: event.state == ElementState::Pressed,
                repeat: event.repeat,
            },
        ))
    }

    /// Движение курсора → raw-дельта.
    #[must_use]
    pub fn cursor_moved(
        &mut self,
        position: PhysicalPosition<f64>,
        timestamp: Duration,
    ) -> Option<RawEvent> {
        let current = (position.x, position.y);
        let delta = self
            .last_cursor
            .map_or((0.0, 0.0), |last| (current.0 - last.0, current.1 - last.1));
        self.last_cursor = Some(current);
        if delta.0 == 0.0 && delta.1 == 0.0 {
            return None;
        }
        Some(RawEvent::new(
            timestamp,
            RawEventKind::MouseMotion {
                dx: delta.0,
                dy: delta.1,
            },
        ))
    }

    /// Кнопка мыши → raw-событие.
    #[must_use]
    pub fn mouse_button(
        &mut self,
        state: ElementState,
        button: WinitMouseButton,
        timestamp: Duration,
    ) -> Option<RawEvent> {
        Some(RawEvent::new(
            timestamp,
            RawEventKind::MouseButton {
                button: map_mouse_button(button)?,
                pressed: state == ElementState::Pressed,
            },
        ))
    }

    /// Колесо → raw-событие.
    #[must_use]
    pub fn mouse_wheel(&mut self, delta: MouseScrollDelta, timestamp: Duration) -> RawEvent {
        let (dx, dy) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (f64::from(x), f64::from(y)),
            MouseScrollDelta::PixelDelta(position) => (position.x, position.y),
        };
        RawEvent::new(timestamp, RawEventKind::Wheel { dx, dy })
    }

    /// IME-событие → raw-текст (только подтверждённый commit).
    #[must_use]
    pub fn ime(&mut self, event: &Ime, timestamp: Duration) -> Option<RawEvent> {
        match event {
            Ime::Commit(text) if !text.is_empty() => {
                Some(RawEvent::new(timestamp, RawEventKind::Text(text.clone())))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_codes_are_layout_independent() {
        assert_eq!(map_key_code(KeyCode::KeyW), Some(Scancode::W));
        assert_eq!(map_key_code(KeyCode::KeyA), Some(Scancode::A));
        assert_eq!(map_key_code(KeyCode::KeyS), Some(Scancode::S));
        assert_eq!(map_key_code(KeyCode::KeyD), Some(Scancode::D));
        assert_eq!(map_key_code(KeyCode::Space), Some(Scancode::SPACE));
        assert_eq!(map_key_code(KeyCode::Escape), Some(Scancode::ESCAPE));
        assert_eq!(map_key_code(KeyCode::F1), Some(Scancode::F1));
        assert_eq!(map_key_code(KeyCode::KeyZ), None);
    }

    #[test]
    fn cursor_delta_is_relative_to_previous_position() {
        let mut adapter = WinitInputAdapter::new();
        assert!(adapter
            .cursor_moved(PhysicalPosition::new(10.0, 20.0), Duration::ZERO)
            .is_none());
        let event = adapter
            .cursor_moved(PhysicalPosition::new(13.0, 18.0), Duration::from_millis(1))
            .unwrap();
        assert_eq!(event.kind, RawEventKind::MouseMotion { dx: 3.0, dy: -2.0 });
    }

    #[test]
    fn window_events_are_translated() {
        let mut adapter = WinitInputAdapter::new();
        assert_eq!(
            adapter.window_event(&WinitWindowEvent::Focused(true)),
            Some(WindowEvent::Focus(true))
        );
        assert_eq!(
            adapter.window_event(&WinitWindowEvent::Resized(winit::dpi::PhysicalSize::new(
                1280, 720
            ))),
            Some(WindowEvent::Resize(PhysicalSize::new(1280, 720)))
        );
        assert!(matches!(
            adapter.window_event(&WinitWindowEvent::CloseRequested),
            Some(WindowEvent::CloseRequest(CloseRequest { .. }))
        ));
    }

    #[test]
    fn mouse_buttons_and_wheel_are_translated() {
        let mut adapter = WinitInputAdapter::new();
        let event = adapter
            .mouse_button(
                ElementState::Pressed,
                WinitMouseButton::Left,
                Duration::ZERO,
            )
            .unwrap();
        assert_eq!(
            event.kind,
            RawEventKind::MouseButton {
                button: MouseButton::Left,
                pressed: true
            }
        );

        let wheel = adapter.mouse_wheel(MouseScrollDelta::LineDelta(1.0, -2.0), Duration::ZERO);
        assert_eq!(wheel.kind, RawEventKind::Wheel { dx: 1.0, dy: -2.0 });
    }

    #[test]
    fn ime_commit_becomes_text() {
        let mut adapter = WinitInputAdapter::new();
        assert!(adapter
            .ime(&Ime::Preedit("に".to_owned(), None), Duration::ZERO)
            .is_none());
        let event = adapter
            .ime(&Ime::Commit("にほ".to_owned()), Duration::ZERO)
            .unwrap();
        assert_eq!(event.kind, RawEventKind::Text("にほ".to_owned()));
    }
}
