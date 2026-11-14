//! Контракт цикла: шаги экспонированы, пауза останавливает симуляцию,
//! паника в job не роняет кадр, плагины проходят жизненный цикл.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use flux_engine::bootstrap::{EngineBuilder, EngineConfig};
use flux_engine::context::EngineContext;
use flux_engine::plugins::Plugin;

fn headless_config() -> EngineConfig {
    EngineConfig {
        logging: false,
        crash_handler: false,
        throttle: false,
        ..EngineConfig::default()
    }
}

#[derive(Default)]
struct Counters {
    built: AtomicUsize,
    fixed: AtomicUsize,
    variable: AtomicUsize,
    shutdown: AtomicUsize,
}

struct LifecyclePlugin {
    counters: Arc<Counters>,
    panic_once: bool,
}

impl Plugin for LifecyclePlugin {
    fn name(&self) -> &str {
        "lifecycle"
    }

    fn build(&self, _ctx: &mut EngineContext) {
        self.counters.built.fetch_add(1, Ordering::Relaxed);
    }

    fn update_fixed(&mut self, _ctx: &mut EngineContext, _step: u32) {
        self.counters.fixed.fetch_add(1, Ordering::Relaxed);
    }

    fn update_variable(&mut self, ctx: &mut EngineContext) {
        self.counters.variable.fetch_add(1, Ordering::Relaxed);
        if self.panic_once {
            self.panic_once = false;
            ctx.spawn(|| panic!("intentional panic in job (test)"));
        }
    }

    fn shutdown(&mut self) {
        self.counters.shutdown.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn plugin_lifecycle_runs_in_order() {
    let counters = Arc::new(Counters::default());
    let plugin = LifecyclePlugin {
        counters: Arc::clone(&counters),
        panic_once: false,
    };

    let engine = EngineBuilder::new()
        .with_config(headless_config())
        .with_plugin(plugin)
        .build()
        .expect("engine builds");

    assert_eq!(counters.built.load(Ordering::Relaxed), 1);

    let summary = engine.run_frames(3);

    assert_eq!(summary.frames, 3);
    assert_eq!(counters.variable.load(Ordering::Relaxed), 3);
    assert_eq!(counters.shutdown.load(Ordering::Relaxed), 1);
}

#[test]
fn pause_stops_fixed_steps() {
    let counters = Arc::new(Counters::default());
    let plugin = LifecyclePlugin {
        counters: Arc::clone(&counters),
        panic_once: false,
    };

    let mut engine = EngineBuilder::new()
        .with_config(headless_config())
        .with_plugin(plugin)
        .build()
        .expect("engine builds");

    engine.time_mut().set_paused(true);
    let result = engine.tick();

    assert_eq!(result.steps, 0);
    assert_eq!(counters.fixed.load(Ordering::Relaxed), 0);
}

#[test]
fn job_panic_is_reported_without_killing_the_engine() {
    let counters = Arc::new(Counters::default());
    let plugin = LifecyclePlugin {
        counters: Arc::clone(&counters),
        panic_once: true,
    };

    let mut engine = EngineBuilder::new()
        .with_config(headless_config())
        .with_plugin(plugin)
        .build()
        .expect("engine builds");

    let stats = engine.frame();

    assert_eq!(stats.panicked_jobs, 1);
    assert_eq!(stats.jobs, 1);
    assert_eq!(counters.variable.load(Ordering::Relaxed), 1);
    assert!(engine.is_running());
}

#[test]
fn invalid_config_is_rejected() {
    let config = EngineConfig {
        max_steps_per_frame: 0,
        ..headless_config()
    };

    let error = EngineBuilder::new().with_config(config).build();
    assert!(error.is_err());
}
