//! G1: frame-арена не выполняет системных аллокаций после прогрева.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::alloc::System;

use flux_core::memory::FrameArena;
use stats_alloc::{StatsAlloc, INSTRUMENTED_SYSTEM};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

#[test]
fn frame_arena_has_zero_allocations_after_warmup() {
    let mut arena = FrameArena::new(64 * 1024);

    // Прогрев: арена и тестовый рантайм выходят на стабильный режим.
    for _ in 0..100 {
        arena.reset();
        for _ in 0..64 {
            assert!(arena.alloc(64, 16).is_some());
        }
    }

    let before = GLOBAL.stats();

    for _ in 0..1_000 {
        arena.reset();
        for _ in 0..64 {
            assert!(arena.alloc(64, 16).is_some());
        }
    }

    let after = GLOBAL.stats();
    assert_eq!(
        after.allocations - before.allocations,
        0,
        "frame arena must not allocate after warmup"
    );
    assert_eq!(after.reallocations - before.reallocations, 0);
}
