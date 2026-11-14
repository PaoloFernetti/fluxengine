//! I2: ECS-интеграция KCC через `Schedule` (Controller → ControllerPost).
//!
//! Проверяются инварианты ADR-0024: всё состояние — в компоненте, KCC —
//! чистая функция, порядок стадий детерминирован, one-shot `jump` сбрасывается
//! пост-системой.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use flux_core::input::Intention;
use flux_ecs::commands::Commands;
use flux_ecs::entity::Entity;
use flux_ecs::scheduler::{Schedule, Stage};
use flux_ecs::world::World;
use flux_engine::simulation::{install_controller_schedule, CharacterBody, PhysicsStep};
use flux_physics::{CharacterConfig, CharacterState, DVec3, TileGrid};

fn scene() -> TileGrid {
    // 10×4: пол внизу, стена на x=7, y=1.
    TileGrid::from_rows(&["..........", "..........", ".......#..", "##########"])
}

fn setup(move_axis: [f64; 2], jump: bool) -> (World, Entity, Schedule) {
    let config = CharacterConfig::default();
    let state = CharacterState {
        position: DVec3::new(2.0, 1.0 + config.half_height + config.radius, 0.0),
        ..CharacterState::default()
    };

    let mut world = World::new();
    let entity = world.spawn();
    world
        .insert(
            entity,
            CharacterBody {
                state,
                config,
                intention: Intention {
                    move_axis,
                    jump,
                    ..Intention::IDLE
                },
                hit: false,
            },
        )
        .unwrap();
    world.insert_resource(scene());
    world.insert_resource(PhysicsStep::new(1.0 / 60.0));

    let mut schedule = Schedule::with_stages(&[Stage::UPDATE, Stage::POST]);
    install_controller_schedule(&mut schedule);
    (world, entity, schedule)
}

#[test]
fn controller_moves_character_through_schedule() {
    let (mut world, entity, mut schedule) = setup([1.0, 0.0], false);
    let mut commands = Commands::new();

    for _ in 0..60 {
        schedule.run(&mut world, &mut commands);
    }

    let body = *world.get::<CharacterBody>(entity).unwrap();
    assert!(body.state.position.x > 2.0, "x = {}", body.state.position.x);
    assert!(body.state.grounded);
    // Ресурс геометрии возвращён после системы.
    assert!(world.resource::<TileGrid>().is_some());
}

#[test]
fn controller_post_clears_one_shot_jump() {
    let (mut world, entity, mut schedule) = setup([0.0, 0.0], false);
    let mut commands = Commands::new();

    // Сначала приземляемся: прыжок возможен только с земли.
    for _ in 0..5 {
        schedule.run(&mut world, &mut commands);
    }
    world
        .get_mut::<CharacterBody>(entity)
        .unwrap()
        .intention
        .jump = true;
    schedule.run(&mut world, &mut commands);

    let body = *world.get::<CharacterBody>(entity).unwrap();
    assert!(body.state.velocity.y > 0.0);
    assert!(!body.intention.jump);
    assert!(!body.hit);
}

#[test]
fn ecs_trace_is_deterministic() {
    let run = || {
        let (mut world, entity, mut schedule) = setup([1.0, 0.0], false);
        let mut commands = Commands::new();
        for _ in 0..120 {
            schedule.run(&mut world, &mut commands);
        }
        *world.get::<CharacterBody>(entity).unwrap()
    };

    assert_eq!(run(), run());
}
