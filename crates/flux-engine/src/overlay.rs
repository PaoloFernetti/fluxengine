//! Dev-overlay: снимок телеметрии кадра для отладочного HUD.
//!
//! Overlay — read-only представление: он не владеет миром и не влияет на
//! симуляцию. Данные собираются из уже существующих подсистем
//! ([`Engine::overlay`](crate::Engine::overlay)), поэтому его вызов не
//! меняет тик, порядок систем и хеш детерминизма (ADR-0033).

use flux_core::time::SimTick;

use crate::runtime::FrameStats;

/// Снимок состояния для dev-overlay.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OverlayData {
    /// Текущий тик симуляции.
    pub sim_tick: SimTick,
    /// Статистика последнего кадра.
    pub frame: FrameStats,
    /// Число воркеров job system.
    pub worker_threads: usize,
    /// Число сущностей, если игра его сообщила.
    pub entities: Option<usize>,
}

impl OverlayData {
    /// Однострочная сводка для заголовка окна/лога.
    #[must_use]
    pub fn summary(&self) -> String {
        let entities = match self.entities {
            Some(count) => count.to_string(),
            None => "-".to_owned(),
        };
        format!(
            "tick {} | frame {} | {:.1} fps | {:.2} ms (p50 {:.2}, p99 {:.2}) | jobs {} (panicked {}) | workers {} | entities {}",
            self.sim_tick.get(),
            self.frame.frame_index,
            self.frame.fps,
            self.frame.frame_ms,
            self.frame.p50_ms,
            self.frame.p99_ms,
            self.frame.jobs,
            self.frame.panicked_jobs,
            self.worker_threads,
            entities,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::bootstrap::EngineBuilder;

    #[test]
    fn overlay_reflects_tick_and_is_read_only() {
        let mut engine = EngineBuilder::new().with_worker_threads(1).build().unwrap();

        let tick = engine.tick();
        let frame = engine.frame();
        let overlay = engine.overlay();

        assert_eq!(overlay.sim_tick, tick.sim_tick);
        assert_eq!(overlay.frame, frame);
        assert_eq!(overlay.worker_threads, 1);
        assert!(overlay.summary().contains("tick"));

        // Повторное чтение не меняет состояние движка.
        let again = engine.overlay();
        assert_eq!(overlay, again);
        assert_eq!(engine.frame_index(), frame.frame_index);
    }

    #[test]
    fn summary_reports_optional_entities() {
        let mut engine = EngineBuilder::new().build().unwrap();
        engine.set_entity_count(Some(7));
        let overlay = engine.overlay();
        assert_eq!(overlay.entities, Some(7));
        assert!(overlay.summary().contains("entities 7"));
    }
}
