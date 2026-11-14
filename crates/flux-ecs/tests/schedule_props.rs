//! Property-тесты I3: порядок систем согласован со всеми before/after,
//! детерминирован между прогонами, `TickRate::EveryN` считает тики.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use flux_ecs::commands::Commands;
use flux_ecs::scheduler::{Schedule, Stage, SystemDescriptor, TickRate};
use flux_ecs::world::World;
use proptest::prelude::*;

const NAMES: [&str; 12] = [
    "s00", "s01", "s02", "s03", "s04", "s05", "s06", "s07", "s08", "s09", "s10", "s11",
];

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 64,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    /// Любой DAG: порядок уважает все рёбра и повторяем между вызовами.
    #[test]
    fn plan_respects_all_dependencies(
        count in 1usize..12,
        edges in prop::collection::vec(any::<bool>(), 0..144),
    ) {
        let names = &NAMES[..count];
        let mut schedule = Schedule::new();
        let mut expected: Vec<(usize, usize)> = Vec::new();

        for index in 0..count {
            let mut dependencies: Vec<&'static str> = Vec::new();
            for (source, name) in names.iter().enumerate().take(index) {
                if edges.get(index * count + source).copied().unwrap_or(false) {
                    dependencies.push(*name);
                    expected.push((source, index));
                }
            }
            schedule.add(
                SystemDescriptor::new(names[index], Stage::UPDATE).after(&dependencies),
                |_, _| {},
            );
        }

        let order = schedule.plan(Stage::UPDATE).unwrap();
        prop_assert_eq!(order.len(), count);
        for (source, target) in expected {
            let source_position = order.iter().position(|name| *name == names[source]);
            let target_position = order.iter().position(|name| *name == names[target]);
            prop_assert!(source_position < target_position);
        }
        prop_assert_eq!(schedule.plan(Stage::UPDATE).unwrap(), order);
    }

    /// `EveryN(n)` выполняется ровно `ticks / n` раз.
    #[test]
    fn every_n_matches_tick_arithmetic(n in 1u32..8, ticks in 1usize..24) {
        let mut world = World::new();
        let mut commands = Commands::new();
        let counter = Arc::new(AtomicUsize::new(0));

        let mut schedule = Schedule::new();
        let handle = Arc::clone(&counter);
        schedule.add(
            SystemDescriptor::new("periodic", Stage::UPDATE).tick_rate(TickRate::EveryN(n)),
            move |_, _| {
                handle.fetch_add(1, Ordering::Relaxed);
            },
        );

        for _ in 0..ticks {
            schedule.run(&mut world, &mut commands);
        }

        prop_assert_eq!(counter.load(Ordering::Relaxed), ticks / n as usize);
    }
}
