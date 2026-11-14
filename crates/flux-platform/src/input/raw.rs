//! Raw-слой ввода: физические scancodes, дельты мыши, очередь с таймстемпами.
//!
//! Раскладка клавиатуры здесь не существует: WASD на AZERTY — это те же
//! физические клавиши (ADR-0031). Raw-события не попадают в ECS: на границе
//! тика они превращаются в `Intention`.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Физический код клавиши (USB HID usage, не символ раскладки).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Scancode(pub u32);

impl Scancode {
    /// Клавиша W (физическая позиция; на AZERTY — Z).
    pub const W: Self = Self(0x1A);
    /// Клавиша A (на AZERTY — Q).
    pub const A: Self = Self(0x04);
    /// Клавиша S.
    pub const S: Self = Self(0x16);
    /// Клавиша D.
    pub const D: Self = Self(0x07);
    /// Пробел.
    pub const SPACE: Self = Self(0x2C);
    /// Левый Ctrl.
    pub const LEFT_CTRL: Self = Self(0xE0);
    /// Escape.
    pub const ESCAPE: Self = Self(0x29);
    /// F1.
    pub const F1: Self = Self(0x3A);
}

/// Кнопка мыши.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MouseButton {
    /// Левая.
    Left,
    /// Правая.
    Right,
    /// Средняя.
    Middle,
    /// Прочие (номер).
    Other(u16),
}

/// Тип сырого события.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum RawEventKind {
    /// Нажатие/отпускание клавиши.
    Key {
        /// Физический код.
        scancode: Scancode,
        /// Нажата (иначе отпущена).
        pressed: bool,
        /// Автоповтор ОС (не считается новым нажатием).
        repeat: bool,
    },
    /// Кнопка мыши.
    MouseButton {
        /// Кнопка.
        button: MouseButton,
        /// Нажата.
        pressed: bool,
    },
    /// Сырая дельта мыши (без OS-ускорения).
    MouseMotion {
        /// Дельта по X.
        dx: f64,
        /// Дельта по Y.
        dy: f64,
    },
    /// Колесо.
    Wheel {
        /// Горизонталь.
        dx: f64,
        /// Вертикаль.
        dy: f64,
    },
    /// Фокус окна.
    Focus {
        /// Получен ли фокус.
        focused: bool,
    },
    /// Текстовый/IME-поток (не собирается из KeyDown).
    Text(String),
}

/// Сырое событие с таймстемпом (для input buffering и детерминизма).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RawEvent {
    /// Время события относительно начала кадра/сессии.
    pub timestamp: Duration,
    /// Полезная нагрузка.
    pub kind: RawEventKind,
}

impl RawEvent {
    /// Создаёт событие.
    #[must_use]
    pub const fn new(timestamp: Duration, kind: RawEventKind) -> Self {
        Self { timestamp, kind }
    }

    /// Это нажатие клавиши (не автоповтор).
    #[must_use]
    pub fn is_key_press(&self) -> bool {
        matches!(
            self.kind,
            RawEventKind::Key {
                pressed: true,
                repeat: false,
                ..
            }
        )
    }
}

/// Очередь сырых событий кадра.
#[derive(Clone, Debug, Default)]
pub struct RawQueue {
    events: Vec<RawEvent>,
}

impl RawQueue {
    /// Пустая очередь.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Добавляет событие.
    pub fn push(&mut self, event: RawEvent) {
        self.events.push(event);
    }

    /// Число событий.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Пуста ли очередь.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// События без очистки.
    #[must_use]
    pub fn events(&self) -> &[RawEvent] {
        &self.events
    }

    /// Забирает события (очередь очищается).
    pub fn take(&mut self) -> Vec<RawEvent> {
        std::mem::take(&mut self.events)
    }

    /// Очищает очередь.
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Таймстемп последнего события.
    #[must_use]
    pub fn last_timestamp(&self) -> Option<Duration> {
        self.events.last().map(|event| event.timestamp)
    }

    /// Потеря фокуса в кадре (все клавиши считаются отпущенными).
    #[must_use]
    pub fn focus_lost(&self) -> bool {
        self.events
            .iter()
            .any(|event| matches!(event.kind, RawEventKind::Focus { focused: false }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scancode_is_layout_independent() {
        // На AZERTY физическая клавиша W печатает Z, но scancode тот же.
        let w = Scancode::W;
        assert_eq!(w, Scancode(0x1A));
        assert_ne!(w, Scancode::A);
    }

    #[test]
    fn queue_preserves_order_and_clears() {
        let mut queue = RawQueue::new();
        queue.push(RawEvent::new(
            Duration::from_millis(1),
            RawEventKind::Key {
                scancode: Scancode::W,
                pressed: true,
                repeat: false,
            },
        ));
        queue.push(RawEvent::new(
            Duration::from_millis(2),
            RawEventKind::Focus { focused: false },
        ));

        assert_eq!(queue.len(), 2);
        assert_eq!(queue.last_timestamp(), Some(Duration::from_millis(2)));
        assert!(queue.focus_lost());
        assert!(queue.events()[0].is_key_press());

        let events = queue.take();
        assert_eq!(events.len(), 2);
        assert!(queue.is_empty());
    }
}
