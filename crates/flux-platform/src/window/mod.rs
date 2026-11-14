//! Оконный слой: режимы, события, close-request, метрики.
//!
//! Типы не зависят от `winit`: headless-профиль подаёт события вручную
//! (ADR-0033). Resize и DPI-события коалесцируются в один кадр.

use serde::{Deserialize, Serialize};

use crate::display::{DpiInfo, LogicalSize, PhysicalSize};

#[cfg(feature = "windowing")]
pub mod winit_adapter;

#[cfg(feature = "windowing")]
pub use winit_adapter::{
    apply_cursor_mode, cursor_grab_mode, map_key_code, map_mouse_button, WinitInputAdapter,
};

/// Идентификатор окна.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct WindowId(pub u64);

/// Режим окна.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowMode {
    /// Оконный.
    #[default]
    Windowed,
    /// Без рамки на весь монитор.
    Borderless,
    /// Эксклюзивный полноэкранный.
    Exclusive,
}

/// Решение по close-request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloseDecision {
    /// Ещё не решено.
    #[default]
    Pending,
    /// Закрыть окно.
    Accept,
    /// Отклонить (например, есть несохранённые данные).
    Deny,
}

/// Запрос на закрытие окна, который можно отклонить.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseRequest {
    decision: CloseDecision,
}

impl CloseRequest {
    /// Новый неразрешённый запрос.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Принять закрытие.
    pub fn accept(&mut self) {
        self.decision = CloseDecision::Accept;
    }

    /// Отклонить закрытие.
    pub fn deny(&mut self) {
        self.decision = CloseDecision::Deny;
    }

    /// Текущее решение.
    #[must_use]
    pub fn decision(self) -> CloseDecision {
        self.decision
    }

    /// Закрытие принято.
    #[must_use]
    pub fn is_accepted(self) -> bool {
        self.decision == CloseDecision::Accept
    }

    /// Закрытие отклонено.
    #[must_use]
    pub fn is_denied(self) -> bool {
        self.decision == CloseDecision::Deny
    }
}

/// Событие окна.
#[derive(Clone, Debug, PartialEq)]
pub enum WindowEvent {
    /// Изменение физического размера.
    Resize(PhysicalSize),
    /// Получение/потеря фокуса.
    Focus(bool),
    /// Смена DPI.
    DpiChanged(DpiInfo),
    /// Запрос на закрытие.
    CloseRequest(CloseRequest),
    /// Перемещение окна.
    Moved {
        /// Координата X.
        x: i32,
        /// Координата Y.
        y: i32,
    },
    /// Смена масштаба (до пересчёта размеров).
    ScaleFactorChanged(f64),
}

/// Очередь событий окна с коалесингом resize/DPI.
#[derive(Clone, Debug, Default)]
pub struct WindowEventQueue {
    events: Vec<WindowEvent>,
}

impl WindowEventQueue {
    /// Пустая очередь.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Добавляет событие, коалесцируя resize и DPI-события.
    ///
    /// В очереди остаётся только последнее событие каждого коалесцируемого
    /// типа; порядок остальных событий сохраняется.
    pub fn push(&mut self, event: WindowEvent) {
        match &event {
            WindowEvent::Resize(_) => {
                self.events
                    .retain(|queued| !matches!(queued, WindowEvent::Resize(_)));
            }
            WindowEvent::DpiChanged(_) | WindowEvent::ScaleFactorChanged(_) => {
                self.events.retain(|queued| {
                    !matches!(
                        queued,
                        WindowEvent::DpiChanged(_) | WindowEvent::ScaleFactorChanged(_)
                    )
                });
            }
            _ => {}
        }
        self.events.push(event);
    }

    /// События (только чтение).
    #[must_use]
    pub fn events(&self) -> &[WindowEvent] {
        &self.events
    }

    /// Забирает события, очищая очередь.
    pub fn take(&mut self) -> Vec<WindowEvent> {
        std::mem::take(&mut self.events)
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

    /// Есть ли неразрешённый close-request.
    #[must_use]
    pub fn is_close_requested(&self) -> bool {
        self.events.iter().any(|event| {
            matches!(
                event,
                WindowEvent::CloseRequest(request) if request.decision() == CloseDecision::Pending
            )
        })
    }

    /// Разрешает первый неразрешённый close-request.
    ///
    /// Возвращает `true`, если запрос найден.
    pub fn resolve_close(&mut self, decision: CloseDecision) -> bool {
        for event in &mut self.events {
            if let WindowEvent::CloseRequest(request) = event {
                if request.decision() == CloseDecision::Pending {
                    match decision {
                        CloseDecision::Accept => request.accept(),
                        CloseDecision::Deny => request.deny(),
                        CloseDecision::Pending => {}
                    }
                    return true;
                }
            }
        }
        false
    }
}

/// Метрики окна: физический/логический размер и DPI.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowMetrics {
    /// Физический размер.
    pub physical: PhysicalSize,
    /// Логический размер.
    pub logical: LogicalSize,
    /// DPI.
    pub dpi: DpiInfo,
    /// Режим окна.
    pub mode: WindowMode,
}

impl WindowMetrics {
    /// Метрики по физическому размеру.
    #[must_use]
    pub fn from_physical(physical: PhysicalSize, dpi: DpiInfo, mode: WindowMode) -> Self {
        Self {
            physical,
            logical: physical.to_logical(dpi),
            dpi,
            mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_events_are_coalesced() {
        let mut queue = WindowEventQueue::new();
        queue.push(WindowEvent::Resize(PhysicalSize::new(800, 600)));
        queue.push(WindowEvent::Focus(true));
        queue.push(WindowEvent::Resize(PhysicalSize::new(1280, 720)));
        queue.push(WindowEvent::Resize(PhysicalSize::new(1920, 1080)));

        let resizes: Vec<_> = queue
            .events()
            .iter()
            .filter(|event| matches!(event, WindowEvent::Resize(_)))
            .collect();
        assert_eq!(resizes.len(), 1);
        assert_eq!(
            *resizes[0],
            WindowEvent::Resize(PhysicalSize::new(1920, 1080))
        );
        assert_eq!(queue.len(), 2);
    }

    #[test]
    fn dpi_events_are_coalesced_across_kinds() {
        let mut queue = WindowEventQueue::new();
        queue.push(WindowEvent::ScaleFactorChanged(1.5));
        queue.push(WindowEvent::DpiChanged(DpiInfo::new(2.0)));
        assert_eq!(queue.len(), 1);
        assert_eq!(
            queue.events()[0],
            WindowEvent::DpiChanged(DpiInfo::new(2.0))
        );
    }

    #[test]
    fn close_request_can_be_denied() {
        let mut queue = WindowEventQueue::new();
        queue.push(WindowEvent::CloseRequest(CloseRequest::new()));
        assert!(queue.is_close_requested());
        assert!(queue.resolve_close(CloseDecision::Deny));

        let request = queue
            .events()
            .iter()
            .find_map(|event| match event {
                WindowEvent::CloseRequest(request) => Some(*request),
                _ => None,
            })
            .unwrap();
        assert!(request.is_denied());
        assert!(!queue.is_close_requested());
        // Повторное разрешение уже решённого запроса не срабатывает.
        assert!(!queue.resolve_close(CloseDecision::Accept));
    }

    #[test]
    fn metrics_convert_physical_to_logical() {
        let metrics = WindowMetrics::from_physical(
            PhysicalSize::new(2560, 1440),
            DpiInfo::new(2.0),
            WindowMode::Windowed,
        );
        assert_eq!(metrics.logical, LogicalSize::new(1280.0, 720.0));
    }
}
