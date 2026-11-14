//! G2: 10k сущностей, read-heavy / write-heavy / три системы.
//!
//! Метод — ADR-0029: один и тот же мир, 10 000 сущностей, три прохода
//! (чтение, запись, чтение); порог — в `docs/Roadmap.md` (гейт G2).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use flux_core::schema::TypeId;
use flux_ecs::component::Component;
use flux_ecs::world::World;
use uuid::Uuid;

const ENTITIES: usize = 10_000;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Position {
    x: f32,
    y: f32,
}

impl Component for Position {
    const TYPE_ID: TypeId =
        TypeId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0060));
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Velocity {
    x: f32,
    y: f32,
}

impl Component for Velocity {
    const TYPE_ID: TypeId =
        TypeId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0061));
}

fn build_world() -> World {
    let mut world = World::new();
    world.advance_tick();
    for index in 0..ENTITIES {
        let entity = world.spawn();
        world
            .insert(
                entity,
                Position {
                    x: index as f32,
                    y: 0.0,
                },
            )
            .unwrap();
        world.insert(entity, Velocity { x: 1.0, y: 0.5 }).unwrap();
    }
    world
}

fn read_heavy(c: &mut Criterion) {
    let mut world = build_world();
    let mut group = c.benchmark_group("ecs");
    group.bench_function("read_heavy_10k", |b| {
        b.iter(|| {
            let sum: f32 = world.query::<Position>().map(|(_, value)| value.x).sum();
            black_box(sum);
        });
    });
    group.finish();
}

fn write_heavy(c: &mut Criterion) {
    let mut world = build_world();
    let mut group = c.benchmark_group("ecs");
    group.bench_function("write_heavy_10k", |b| {
        b.iter(|| {
            world
                .query_mut::<Position>()
                .for_each_batch(256, |_, values| {
                    for value in values {
                        value.x += 1.0;
                    }
                });
        });
    });
    group.finish();
}

fn three_systems(c: &mut Criterion) {
    let mut world = build_world();
    let mut group = c.benchmark_group("ecs");
    group.bench_function("three_systems_10k", |b| {
        b.iter(|| {
            let read: f32 = world.query::<Position>().map(|(_, value)| value.x).sum();
            world
                .query_mut::<Position>()
                .for_each_batch(256, |_, values| {
                    for value in values {
                        value.x += 1.0;
                    }
                });
            let check: f32 = world.query::<Position>().map(|(_, value)| value.x).sum();
            black_box((read, check));
        });
    });
    group.finish();
}

criterion_group!(benches, read_heavy, write_heavy, three_systems);
criterion_main!(benches);
