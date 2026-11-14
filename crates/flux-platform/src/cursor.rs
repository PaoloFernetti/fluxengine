//! Курсор: четыре режима и позиция.

use serde::{Deserialize, Serialize};

use crate::display::LogicalSize;

/// Режим курсора.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CursorMode {
    /// Видимый, свободный.
    #[default]
    Normal,
    /// Скрытый, свободный.
    Hidden,
    /// Скрытый, захваченный (камера от мыши).
    Disabled,
    /// Ограниченный границами окна.
    Confined,
}

impl CursorMode {
    /// Все режимы в каноническом порядке.
    pub const ALL: [Self; 4] = [Self::Normal, Self::Hidden, Self::Disabled, Self::Confined];

    /// Захватывает ли курсор.
    #[must_use]
    pub fn grabs(self) -> bool {
        matches!(self, Self::Disabled | Self::Confined)
    }

    /// Видим ли курсор.
    #[must_use]
    pub fn is_visible(self) -> bool {
        matches!(self, Self::Normal)
    }
}

/// Позиция курсора в логических координатах окна.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CursorPosition {
    /// X.
    pub x: f64,
    /// Y.
    pub y: f64,
}

impl CursorPosition {
    /// Позиция.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Ограничивает позицию логическим размером окна.
    #[must_use]
    pub fn clamped(self, logical: LogicalSize) -> Self {
        Self {
            x: self.x.clamp(0.0, logical.width.max(0.0)),
            y: self.y.clamp(0.0, logical.height.max(0.0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_have_expected_visibility_and_grab() {
        assert!(CursorMode::Normal.is_visible());
        assert!(!CursorMode::Hidden.is_visible());
        assert!(CursorMode::Disabled.grabs());
        assert!(CursorMode::Confined.grabs());
        assert!(!CursorMode::Normal.grabs());
        assert_eq!(CursorMode::ALL.len(), 4);
    }

    #[test]
    fn position_is_clamped_to_window() {
        let position = CursorPosition::new(-5.0, 200.0);
        assert_eq!(
            position.clamped(LogicalSize::new(100.0, 50.0)),
            CursorPosition::new(0.0, 50.0)
        );
    }
}
