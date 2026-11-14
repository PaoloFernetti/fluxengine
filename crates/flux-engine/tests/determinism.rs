//! I3: same-binary детерминизм — 10k тиков дают идентичный хеш.
//!
//! Хеш строится по каноническому `SaveJournal`-диффу (ADR-0030/0033).
//! Расхождение печатает первую различающуюся сущность/поле.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use flux_engine::determinism::DeterminismHarness;
use flux_engine::EngineBuilder;

#[test]
fn ten_thousand_ticks_are_identical_same_binary() {
    let mut first = DeterminismHarness::new(0xC0FF_EE00);
    let mut second = DeterminismHarness::new(0xC0FF_EE00);

    let left = first.run(10_000);
    let right = second.run(10_000);

    assert_eq!(left.tick.get(), 10_000);
    assert_eq!(left.hash, right.hash);
    assert_eq!(left.journal, right.journal);
    assert!(left.diff(&right).is_none());
}

#[test]
fn different_seeds_diverge_and_diff_names_entity_and_field() {
    let mut first = DeterminismHarness::new(1);
    let mut second = DeterminismHarness::new(2);

    let left = first.run(600);
    let right = second.run(600);

    assert_ne!(left.hash, right.hash);
    let diff = left.diff(&right).unwrap();
    let text = diff.to_string();
    assert!(text.contains("tick"), "{text}");
    assert!(text.contains("entity"), "{text}");
    assert!(text.contains("field"), "{text}");
}

#[test]
fn headless_engine_ticks_without_frame() {
    // Headless-профиль: tick() без frame() не требует окна/рендера.
    let mut engine = EngineBuilder::new().build().unwrap();
    let first = engine.tick();
    let second = engine.tick();
    assert!(second.sim_tick >= first.sim_tick);
    assert!(first.steps <= 8);
}
