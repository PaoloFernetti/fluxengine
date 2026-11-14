//! M2: 10k сущностей, три системы, SchemaRegistry и дифф сейва.
//!
//! Запуск: `cargo run -p flux-ecs --example m2 --release`.
//! В release проверяется гейт G2 (10k × 3 системы < 2 мс, ADR-0029/Roadmap);
//! в debug печатается предупреждение без падения.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use flux_ecs::commands::Commands;
use flux_ecs::entity::Entity;
use flux_ecs::scheduler::{Access, Schedule, Stage, SystemDescriptor};
use flux_ecs::schema::SchemaRegistry;
use flux_ecs::sim_time::SimTimeComponent;
use flux_ecs::world::World;
use flux_ecs::{component::Component, save::ComponentDelta, save::FieldValue, save::SaveJournal};

const ENTITIES: usize = 10_000;
const TICKS: usize = 60;
const STEP: f32 = 1.0 / 60.0;
const G2_BUDGET: Duration = Duration::from_micros(2_000);

#[derive(Clone, Copy, Debug, Default, PartialEq, flux_macros::Reflect)]
#[reflect(type_uuid = "a1b2c3d4-0000-4000-8000-000000000001")]
struct Position {
    #[save]
    #[reflect(min = -100_000.0, max = 100_000.0, slider, category = "Transform")]
    x: f32,
    #[save]
    #[reflect(min = -100_000.0, max = 100_000.0, slider, category = "Transform")]
    y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, flux_macros::Reflect)]
#[reflect(type_uuid = "a1b2c3d4-0000-4000-8000-000000000002")]
struct Velocity {
    #[save]
    x: f32,
    #[save]
    y: f32,
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
                    x: index as f32 * 0.5,
                    y: 0.0,
                },
            )
            .expect("position insert");
        world
            .insert(entity, Velocity { x: 1.0, y: 0.25 })
            .expect("velocity insert");
    }

    let clock = world.spawn();
    world
        .insert(clock, SimTimeComponent::from_tick(world.tick()))
        .expect("sim time insert");
    world
}

fn main() {
    let mut world = build_world();
    let mut commands = Commands::new();
    let mut schedule = Schedule::with_stages(&[Stage::UPDATE, Stage::POST]);

    schedule.add(
        SystemDescriptor::new("integrate", Stage::UPDATE)
            .access(Access::new().read::<Velocity>().write::<Position>()),
        |world, _| {
            let mut velocities: HashMap<Entity, (f32, f32)> = HashMap::new();
            for (entity, velocity) in world.query::<Velocity>() {
                velocities.insert(entity, (velocity.x, velocity.y));
            }
            world.query_mut::<Position>().for_each(|entity, position| {
                if let Some((vx, vy)) = velocities.get(&entity) {
                    position.x += vx * STEP;
                    position.y += vy * STEP;
                }
            });
        },
    );

    schedule.add(
        SystemDescriptor::new("gravity", Stage::UPDATE)
            .after(&["integrate"])
            .access(Access::new().write::<Velocity>()),
        |world, _| {
            world
                .query_mut::<Velocity>()
                .for_each(|_, velocity| velocity.y -= 9.81 * STEP);
        },
    );

    schedule.add(
        SystemDescriptor::new("report", Stage::POST).access(Access::new().read::<Position>()),
        |world, _| {
            let count = world.query::<Position>().count();
            assert_eq!(count, ENTITIES, "all entities must keep Position");
        },
    );

    let mut elapsed = Duration::ZERO;
    for _ in 0..TICKS {
        let start = Instant::now();
        let report = schedule.run(&mut world, &mut commands);
        elapsed += start.elapsed();
        assert!(
            report.errors.is_empty(),
            "schedule errors: {:?}",
            report.errors
        );
    }
    let average = elapsed / TICKS as u32;

    println!(
        "M2: {} сущностей, {} тиков, {:?}/тик, {} архетипов",
        world.entity_count(),
        TICKS,
        average,
        world.archetype_count()
    );

    inspect_schemas();
    save_roundtrip(&mut world);

    if cfg!(debug_assertions) {
        println!("G2: debug-профиль, порог не проверяется (запустите --release)");
    } else if average < G2_BUDGET {
        println!("G2: ok ({average:?} < {G2_BUDGET:?})");
    } else {
        println!("G2: FAIL ({average:?} >= {G2_BUDGET:?})");
        std::process::exit(1);
    }
}

fn inspect_schemas() {
    let mut registry = SchemaRegistry::new();
    for schema in [
        Position::schema(),
        Velocity::schema(),
        SimTimeComponent::schema(),
    ] {
        registry.register(schema).expect("schema register");
    }
    println!("SchemaRegistry: {} ревизий", registry.len());
    for schema in registry.schemas() {
        println!(
            "  {} rev {}: {} полей ({} save, {} replicated)",
            schema.name,
            schema.schema.schema_rev,
            schema.fields.len(),
            schema.save_fields().count(),
            schema.replicated_fields().count()
        );
    }
}

fn save_roundtrip(world: &mut World) {
    let schema = Position::schema();
    let x_field = schema
        .fields
        .iter()
        .find(|field| field.name == "x")
        .map(|field| field.field_uuid);
    let y_field = schema
        .fields
        .iter()
        .find(|field| field.name == "y")
        .map(|field| field.field_uuid);
    let (Some(x_field), Some(y_field)) = (x_field, y_field) else {
        return;
    };

    let snapshot: Vec<(Entity, Position)> = world
        .query::<Position>()
        .take(16)
        .map(|(entity, position)| (entity, *position))
        .collect();
    let mut journal = SaveJournal::new();
    let mut deltas = Vec::new();
    for (entity, position) in snapshot {
        let Some(uuid) = world.uuid(entity) else {
            continue;
        };
        deltas.push(
            ComponentDelta::new(uuid, schema.schema)
                .with_field(FieldValue::new(
                    x_field,
                    flux_ecs::save::Value::F32(position.x),
                ))
                .with_field(FieldValue::new(
                    y_field,
                    flux_ecs::save::Value::F32(position.y),
                )),
        );
    }

    let seq = journal.append(world.tick(), deltas);
    let text = ron::to_string(&journal).expect("journal to ron");
    let restored: SaveJournal = ron::from_str(&text).expect("journal from ron");
    assert_eq!(restored, journal, "journal roundtrip must be stable");
    println!(
        "SaveJournal: seq {}, {} дельт, {} байт RON",
        seq,
        journal.entries[0].deltas.len(),
        text.len()
    );
}
