//! ECS-интеграция KCC: компонент `CharacterBody` и системы контроллера.
//!
//! `flux-physics` остаётся чистым (канон: physics → core), поэтому ECS-обвязка
//! живёт в фасаде. `CharacterBody` — компонент-обёртка, `TileGrid` — ресурс
//! мира. Порядок тика: Input → Controller (`UPDATE`) → Physics →
//! ControllerPost (`POST`) (ADR-0024).

use serde::{Deserialize, Serialize};

use flux_core::input::Intention;
use flux_core::schema::{uuid_from_u128, TypeId};
use flux_ecs::commands::Commands;
use flux_ecs::component::Component;
use flux_ecs::scheduler::{Access, Schedule, Stage, SystemDescriptor};
use flux_ecs::world::World;
use flux_physics::{move_character, CharacterConfig, CharacterState, TileGrid};

/// Имя системы Controller.
pub const CONTROLLER_SYSTEM: &str = "controller";
/// Имя пост-системы Controller.
pub const CONTROLLER_POST_SYSTEM: &str = "controller_post";

/// Шаг симуляции (ресурс мира).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PhysicsStep {
    /// Фиксированный шаг в секундах.
    pub dt: f64,
}

impl Default for PhysicsStep {
    fn default() -> Self {
        Self { dt: 1.0 / 60.0 }
    }
}

impl PhysicsStep {
    /// Шаг симуляции.
    #[must_use]
    pub const fn new(dt: f64) -> Self {
        Self { dt }
    }
}

/// Персонаж: состояние, конфиг и намерение на тик.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CharacterBody {
    /// Состояние KCC.
    pub state: CharacterState,
    /// Параметры персонажа.
    pub config: CharacterConfig,
    /// Намерение на текущий тик.
    pub intention: Intention,
    /// Был ли контакт на последнем шаге.
    pub hit: bool,
}

impl Component for CharacterBody {
    const TYPE_ID: TypeId =
        TypeId::from_uuid(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0700));
}

/// Система Controller: двигает персонажа по `Intention`.
///
/// `TileGrid` временно изымается из ресурсов мира, чтобы совместить
/// `query_mut` и чтение геометрии без аллокаций и заимствований.
pub fn controller_system(world: &mut World, _commands: &mut Commands) {
    let dt = world
        .resource::<PhysicsStep>()
        .map_or(1.0 / 60.0, |step| step.dt);
    let Some(grid) = world.resources_mut().remove::<TileGrid>() else {
        return;
    };

    world
        .query_mut::<CharacterBody>()
        .for_each(|_entity, body| {
            let result = move_character(&body.state, &body.config, &body.intention, dt, &grid);
            body.state = result.state;
            body.hit = result.hit;
        });

    world.insert_resource(grid);
}

/// Система ControllerPost: сбрасывает one-shot ввод после шага.
///
/// Без этого удержание `jump` приводило бы к повторному прыжку при касании
/// земли; пост-система делает прыжок событием тика.
pub fn controller_post_system(world: &mut World, _commands: &mut Commands) {
    world
        .query_mut::<CharacterBody>()
        .for_each(|_entity, body| {
            body.intention.jump = false;
            body.hit = false;
        });
}

/// Регистрирует Controller и ControllerPost в расписании.
pub fn install_controller_schedule(schedule: &mut Schedule) {
    schedule.add(
        SystemDescriptor::new(CONTROLLER_SYSTEM, Stage::UPDATE)
            .access(Access::new().write::<CharacterBody>()),
        controller_system,
    );
    schedule.add(
        SystemDescriptor::new(CONTROLLER_POST_SYSTEM, Stage::POST)
            .access(Access::new().write::<CharacterBody>()),
        controller_post_system,
    );
}
