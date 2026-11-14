//! Время: часы, фиксированный шаг, контекст времени, SimTick/SimTime.

use core::time::Duration;
use std::time::Instant;

/// Шаг симуляции по умолчанию: 60 Гц.
pub const DEFAULT_FIXED_STEP: Duration = Duration::from_nanos(16_666_667);

/// Часы реального времени.
#[derive(Debug)]
pub struct Clock {
    last: Instant,
    delta: Duration,
    elapsed: Duration,
}

impl Clock {
    /// Создаёт часы в текущий момент.
    #[must_use]
    pub fn new() -> Self {
        Self {
            last: Instant::now(),
            delta: Duration::ZERO,
            elapsed: Duration::ZERO,
        }
    }

    /// Возвращает дельту с прошлого вызова и накапливает elapsed.
    pub fn tick(&mut self) -> Duration {
        let now = Instant::now();
        self.delta = now.duration_since(self.last);
        self.last = now;
        self.elapsed = self.elapsed.saturating_add(self.delta);
        self.delta
    }

    /// Последняя дельта.
    #[must_use]
    pub fn delta(&self) -> Duration {
        self.delta
    }

    /// Время с создания часов.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new()
    }
}

/// Детерминированный тик симуляции.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct SimTick(u64);

impl SimTick {
    /// Нулевой тик.
    pub const ZERO: Self = Self(0);

    /// Значение тика.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Продвигает тик на `steps`.
    pub fn advance(&mut self, steps: u64) {
        self.0 = self.0.saturating_add(steps);
    }
}

/// Фиксированный шаг с аккумулятором и анти-spiral ограничением.
#[derive(Clone, Debug)]
pub struct FixedTimestep {
    step: Duration,
    max_steps: u32,
    accumulator: Duration,
    ticks: u64,
    dropped: u64,
}

impl FixedTimestep {
    /// Создаёт шаг с ограничением числа шагов за кадр.
    #[must_use]
    pub fn new(step: Duration, max_steps: u32) -> Self {
        Self {
            step,
            max_steps: max_steps.max(1),
            accumulator: Duration::ZERO,
            ticks: 0,
            dropped: 0,
        }
    }

    /// Шаг симуляции.
    #[must_use]
    pub fn step(&self) -> Duration {
        self.step
    }

    /// Число выполненных тиков.
    #[must_use]
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Отброшенные шаги из-за анти-spiral.
    #[must_use]
    pub fn dropped_steps(&self) -> u64 {
        self.dropped
    }

    /// Доля до следующего шага (`0.0..1.0`) для интерполяции рендера.
    #[must_use]
    pub fn alpha(&self) -> f32 {
        let step_nanos = self.step.as_nanos();
        if step_nanos == 0 {
            return 0.0;
        }
        (self.accumulator.as_nanos() as f64 / step_nanos as f64) as f32
    }

    /// Добавляет дельту и возвращает число выполненных шагов.
    pub fn advance(&mut self, delta: Duration) -> u32 {
        self.accumulator = self.accumulator.saturating_add(delta);

        let mut steps = 0_u32;
        while self.accumulator >= self.step && steps < self.max_steps {
            self.accumulator = self.accumulator.saturating_sub(self.step);
            self.ticks = self.ticks.saturating_add(1);
            steps += 1;
        }

        if self.accumulator >= self.step {
            let extra = self.accumulator.as_nanos() / self.step.as_nanos().max(1);
            let extra = u32::try_from(extra).unwrap_or(u32::MAX);
            self.dropped = self.dropped.saturating_add(u64::from(extra));
            self.accumulator = self
                .accumulator
                .saturating_sub(self.step.saturating_mul(extra));
        }

        steps
    }

    /// Сбрасывает аккумулятор и счётчики.
    pub fn reset(&mut self) {
        self.accumulator = Duration::ZERO;
        self.ticks = 0;
        self.dropped = 0;
    }
}

impl Default for FixedTimestep {
    fn default() -> Self {
        Self::new(DEFAULT_FIXED_STEP, 8)
    }
}

/// Результат обновления времени.
#[derive(Clone, Copy, Debug)]
pub struct TimeUpdate {
    /// Дельта кадра с учётом паузы и масштаба.
    pub delta: Duration,
    /// Выполненные шаги симуляции.
    pub steps: u32,
    /// Доля до следующего шага.
    pub alpha: f32,
    /// Текущий тик симуляции.
    pub sim_tick: SimTick,
    /// Суммарное время симуляции.
    pub sim_elapsed: Duration,
}

/// Контекст времени: пауза, масштаб, фиксированный шаг.
#[derive(Debug)]
pub struct TimeContext {
    clock: Clock,
    fixed: FixedTimestep,
    paused: bool,
    scale: f32,
    sim_tick: SimTick,
    sim_elapsed: Duration,
}

impl TimeContext {
    /// Создаёт контекст с шагом и лимитом шагов.
    #[must_use]
    pub fn new(step: Duration, max_steps: u32) -> Self {
        Self {
            clock: Clock::new(),
            fixed: FixedTimestep::new(step, max_steps),
            paused: false,
            scale: 1.0,
            sim_tick: SimTick::ZERO,
            sim_elapsed: Duration::ZERO,
        }
    }

    /// Обновляет время и возвращает результат кадра.
    pub fn update(&mut self) -> TimeUpdate {
        let raw = self.clock.tick();
        let delta = if self.paused {
            Duration::ZERO
        } else {
            raw.mul_f32(self.scale.max(0.0))
        };

        let steps = self.fixed.advance(delta);
        self.sim_tick.advance(u64::from(steps));
        self.sim_elapsed = self
            .sim_elapsed
            .saturating_add(self.fixed.step().saturating_mul(steps));

        TimeUpdate {
            delta,
            steps,
            alpha: self.fixed.alpha(),
            sim_tick: self.sim_tick,
            sim_elapsed: self.sim_elapsed,
        }
    }

    /// Пауза: шаги не выполняются.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Признак паузы.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Масштаб времени.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale.max(0.0);
    }

    /// Текущий масштаб.
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Текущий тик.
    #[must_use]
    pub fn sim_tick(&self) -> SimTick {
        self.sim_tick
    }

    /// Время симуляции.
    #[must_use]
    pub fn sim_elapsed(&self) -> Duration {
        self.sim_elapsed
    }

    /// Доступ к фиксированному шагу.
    #[must_use]
    pub fn fixed(&self) -> &FixedTimestep {
        &self.fixed
    }
}

impl Default for TimeContext {
    fn default() -> Self {
        Self::new(DEFAULT_FIXED_STEP, 8)
    }
}

/// Компонент времени суток мира: детерминированный, реплицируемый.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SimTime {
    tick: u64,
    day_fraction: u16,
}

impl SimTime {
    /// Тиков в сутках при 60 Гц.
    pub const TICKS_PER_DAY: u64 = 86_400 * 60;

    /// Создаёт время в начале суток.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Продвигает время на один тик.
    pub fn tick(&mut self) {
        self.tick = self.tick.saturating_add(1);
        let fraction =
            (self.tick % Self::TICKS_PER_DAY) * u64::from(u16::MAX) / Self::TICKS_PER_DAY;
        self.day_fraction = u16::try_from(fraction).unwrap_or(u16::MAX);
    }

    /// Номер тика.
    #[must_use]
    pub fn ticks(self) -> u64 {
        self.tick
    }

    /// Доля суток (`0..=65535`).
    #[must_use]
    pub fn day_fraction(self) -> u16 {
        self.day_fraction
    }
}

/// Секундомер.
#[derive(Debug, Default)]
pub struct Stopwatch {
    started: Option<Instant>,
    elapsed: Duration,
}

impl Stopwatch {
    /// Создаёт остановленный секундомер.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Запускает.
    pub fn start(&mut self) {
        if self.started.is_none() {
            self.started = Some(Instant::now());
        }
    }

    /// Останавливает и накапливает время.
    pub fn stop(&mut self) {
        if let Some(started) = self.started.take() {
            self.elapsed = self.elapsed.saturating_add(started.elapsed());
        }
    }

    /// Идущее время.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.elapsed.saturating_add(
            self.started
                .map_or(Duration::ZERO, |started| started.elapsed()),
        )
    }

    /// Идёт ли сейчас.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.started.is_some()
    }

    /// Сбрасывает.
    pub fn reset(&mut self) {
        self.started = None;
        self.elapsed = Duration::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulator_is_deterministic() {
        let step = Duration::from_millis(10);
        let mut a = FixedTimestep::new(step, 8);
        let mut b = FixedTimestep::new(step, 8);

        let mut ticks_a = Vec::new();
        let mut ticks_b = Vec::new();
        for _ in 0..100 {
            ticks_a.push(a.advance(Duration::from_millis(7)));
            ticks_b.push(b.advance(Duration::from_millis(7)));
        }

        assert_eq!(ticks_a, ticks_b);
        assert_eq!(a.ticks(), b.ticks());
    }

    #[test]
    fn anti_spiral_drops_excess_steps() {
        let mut fixed = FixedTimestep::new(Duration::from_millis(10), 4);
        let steps = fixed.advance(Duration::from_millis(100));
        assert_eq!(steps, 4);
        assert!(fixed.dropped_steps() > 0);
        assert!(fixed.alpha() < 1.0);
    }

    #[test]
    fn pause_stops_simulation() {
        let mut time = TimeContext::new(Duration::from_millis(10), 8);
        time.set_paused(true);
        let update = time.update();
        assert_eq!(update.steps, 0);
        assert_eq!(update.sim_tick, SimTick::ZERO);
    }

    #[test]
    fn sim_time_is_deterministic() {
        let mut a = SimTime::new();
        let mut b = SimTime::new();
        for _ in 0..10_000 {
            a.tick();
            b.tick();
        }
        assert_eq!(a, b);
        assert!(a.day_fraction() > 0);
    }

    #[test]
    fn stopwatch_accumulates() {
        let mut stopwatch = Stopwatch::new();
        stopwatch.start();
        std::thread::sleep(Duration::from_millis(2));
        stopwatch.stop();
        assert!(stopwatch.elapsed() >= Duration::from_millis(1));
        assert!(!stopwatch.is_running());
    }
}
