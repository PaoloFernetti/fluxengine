//! GPU-зоны профилирования: контракт между RHI-таймстампами и trace.
//!
//! Реальные query pool-ы появятся вместе с GPU-проходами (I6b+); здесь —
//! детерминированная структура зон, которую заполняет рендер и читает
//! `FrameStats`/trace. Порядок зон — порядок добавления.

/// Зона GPU: имя и таймстемпы в наносекундах.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuZone {
    /// Стабильное имя зоны (`gbuffer`, `lighting`, ...).
    pub name: &'static str,
    /// Начало, нс.
    pub start_ns: u64,
    /// Конец, нс (не меньше начала).
    pub end_ns: u64,
}

impl GpuZone {
    /// Зона с нормализацией `end >= start`.
    #[must_use]
    pub const fn new(name: &'static str, start_ns: u64, end_ns: u64) -> Self {
        Self {
            name,
            start_ns,
            end_ns: if end_ns < start_ns { start_ns } else { end_ns },
        }
    }

    /// Длительность зоны, нс.
    #[must_use]
    pub const fn duration_ns(self) -> u64 {
        self.end_ns.saturating_sub(self.start_ns)
    }
}

/// Кольцевой профилировщик зон кадра.
#[derive(Clone, Debug)]
pub struct GpuProfiler {
    zones: Vec<GpuZone>,
    capacity: usize,
}

impl GpuProfiler {
    /// Профилировщик с ёмкостью (минимум 1).
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            zones: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    /// Добавляет зону; при переполнении вытесняет самую старую.
    ///
    /// Возвращает `true`, если зона была вытеснена.
    pub fn push(&mut self, zone: GpuZone) -> bool {
        let dropped = if self.zones.len() >= self.capacity {
            self.zones.remove(0);
            true
        } else {
            false
        };
        self.zones.push(zone);
        dropped
    }

    /// Добавляет зоны из пар таймстемпов `[start0, end0, start1, end1, ...]`.
    ///
    /// Нечётный хвост игнорируется. Возвращает число добавленных зон.
    pub fn push_from_timestamps(&mut self, name: &'static str, timestamps: &[u64]) -> usize {
        let mut added = 0;
        for pair in timestamps.chunks_exact(2) {
            self.push(GpuZone::new(name, pair[0], pair[1]));
            added += 1;
        }
        added
    }

    /// Очищает зоны кадра.
    pub fn clear(&mut self) {
        self.zones.clear();
    }

    /// Зоны в порядке добавления.
    #[must_use]
    pub fn zones(&self) -> &[GpuZone] {
        &self.zones
    }

    /// Число зон.
    #[must_use]
    pub fn len(&self) -> usize {
        self.zones.len()
    }

    /// Пуст ли профилировщик.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }

    /// Суммарная длительность всех зон, нс.
    #[must_use]
    pub fn total_ns(&self) -> u64 {
        self.zones.iter().map(|zone| zone.duration_ns()).sum()
    }

    /// Суммарная длительность зон с именем, нс.
    #[must_use]
    pub fn zone_ns(&self, name: &str) -> u64 {
        self.zones
            .iter()
            .filter(|zone| zone.name == name)
            .map(|zone| zone.duration_ns())
            .sum()
    }
}

impl Default for GpuProfiler {
    fn default() -> Self {
        Self::new(64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_normalizes_and_measures() {
        let zone = GpuZone::new("gbuffer", 100, 40);
        assert_eq!(zone.duration_ns(), 0);
        let zone = GpuZone::new("lighting", 100, 250);
        assert_eq!(zone.duration_ns(), 150);
    }

    #[test]
    fn profiler_sums_and_filters() {
        let mut profiler = GpuProfiler::new(8);
        profiler.push(GpuZone::new("gbuffer", 0, 100));
        profiler.push(GpuZone::new("lighting", 100, 250));
        profiler.push(GpuZone::new("lighting", 250, 300));
        assert_eq!(profiler.len(), 3);
        assert_eq!(profiler.total_ns(), 300);
        assert_eq!(profiler.zone_ns("lighting"), 200);
        assert_eq!(profiler.zone_ns("missing"), 0);
    }

    #[test]
    fn timestamps_become_zones() {
        let mut profiler = GpuProfiler::new(8);
        assert_eq!(
            profiler.push_from_timestamps("lighting", &[100, 250, 250, 400, 9]),
            2
        );
        assert_eq!(profiler.len(), 2);
        assert_eq!(profiler.zone_ns("lighting"), 300);
    }

    #[test]
    fn profiler_drops_oldest_when_full() {
        let mut profiler = GpuProfiler::new(2);
        assert!(!profiler.push(GpuZone::new("a", 0, 10)));
        assert!(!profiler.push(GpuZone::new("b", 10, 20)));
        assert!(profiler.push(GpuZone::new("c", 20, 30)));
        assert_eq!(profiler.zones()[0].name, "b");
        assert_eq!(profiler.zones()[1].name, "c");
        profiler.clear();
        assert!(profiler.is_empty());
    }
}
