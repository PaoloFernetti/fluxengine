//! Детерминизм-харнесс: seeded-бот, канонический хеш состояния, дифф.
//!
//! Бот генерирует `Intention` тем же интерфейсом, что игрок (ADR-0031).
//! Хеш строится по каноническому `SaveJournal`-диффу: дельты по
//! `(entity_uuid, type_uuid, schema_rev)`, поля по `field_uuid` (ADR-0030).
//! `HashMap` в порядке не участвует. Дифф печатает первую различающуюся
//! сущность/поле (ADR-0033).

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use flux_core::input::Intention;
use flux_core::schema::{uuid_from_u128, FieldId, SchemaRef, TypeId};
use flux_core::time::SimTick;
use flux_ecs::commands::Commands;
use flux_ecs::entity::{Entity, EntityUuid};
use flux_ecs::save::{ComponentDelta, FieldValue, SaveJournal, Value};
use flux_ecs::scheduler::{Schedule, Stage};
use flux_ecs::world::World;
use flux_physics::{CharacterConfig, CharacterState, DVec3, TileGrid};

use crate::simulation::{install_controller_schedule, CharacterBody, PhysicsStep};

/// Схема состояния персонажа в харнессе.
const CHARACTER_SCHEMA: SchemaRef = SchemaRef::new(
    TypeId::from_uuid(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0701)),
    1,
);

const FIELD_POSITION: FieldId = FieldId(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0710));
const FIELD_VELOCITY: FieldId = FieldId(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0711));
const FIELD_GROUNDED: FieldId = FieldId(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0712));
const FIELD_GROUND_NORMAL: FieldId =
    FieldId(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0713));
const FIELD_HYSTERESIS: FieldId =
    FieldId(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0714));
const FIELD_MOVE_AXIS: FieldId = FieldId(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0715));
const FIELD_LOOK: FieldId = FieldId(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0716));
const FIELD_JUMP: FieldId = FieldId(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0717));
const FIELD_CROUCH: FieldId = FieldId(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0718));
const FIELD_HIT: FieldId = FieldId(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0719));

const ENTITY_UUID: EntityUuid =
    EntityUuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0720));

/// Результат прогона: хеш + канонический журнал для диффа.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StateHash {
    /// FNV-1a по каноническому RON журнала.
    pub hash: u64,
    /// Тик на момент снятия.
    pub tick: SimTick,
    /// Канонический журнал состояния.
    pub journal: SaveJournal,
}

impl StateHash {
    /// Первое различие с другим состоянием; `None` — состояния идентичны.
    #[must_use]
    pub fn diff(&self, other: &Self) -> Option<StateDiff> {
        if self.journal == other.journal {
            return None;
        }
        first_diff(&self.journal, &other.journal)
    }
}

/// Первое различающееся поле/сущность.
#[derive(Clone, Debug, PartialEq)]
pub struct StateDiff {
    /// Тик, на котором обнаружено расхождение.
    pub tick: SimTick,
    /// Сущность.
    pub entity: EntityUuid,
    /// Схема компонента.
    pub schema: SchemaRef,
    /// Поле; `None` — различается структура дельт.
    pub field: Option<FieldId>,
    /// Значение слева.
    pub left: Option<Value>,
    /// Значение справа.
    pub right: Option<Value>,
}

impl fmt::Display for StateDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.field, &self.left, &self.right) {
            (Some(field), Some(left), Some(right)) => write!(
                f,
                "tick {} entity {:?} field {:?}: {:?} != {:?}",
                self.tick.get(),
                self.entity,
                field,
                left,
                right
            ),
            (Some(field), Some(left), None) => write!(
                f,
                "tick {} entity {:?} field {:?}: {:?} != <absent>",
                self.tick.get(),
                self.entity,
                field,
                left
            ),
            (Some(field), None, Some(right)) => write!(
                f,
                "tick {} entity {:?} field {:?}: <absent> != {:?}",
                self.tick.get(),
                self.entity,
                field,
                right
            ),
            _ => write!(
                f,
                "tick {} entity {:?} schema {:?}: структура дельт различается",
                self.tick.get(),
                self.entity,
                self.schema
            ),
        }
    }
}

/// Seeded-бот: детерминированный xorshift64.
struct SeededBot {
    state: u64,
}

impl SeededBot {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.state = value;
        value
    }

    fn intention(&mut self) -> Intention {
        let random = self.next_u64();
        let move_x = ((random & 0xFFFF) as f64 / 32_767.5 - 1.0).clamp(-1.0, 1.0);
        let move_y = (((random >> 16) & 0xFFFF) as f64 / 32_767.5 - 1.0).clamp(-1.0, 1.0);
        Intention {
            move_axis: [move_x, move_y],
            look: [0.0, 0.0],
            jump: (random >> 32) & 0x3F == 0,
            crouch: (random >> 38) & 0x3F == 0,
        }
    }
}

/// Сцена харнесса: пол и ступень.
#[must_use]
pub fn harness_scene() -> TileGrid {
    TileGrid::from_rows(&["..........", "..........", "....##....", "##########"])
}

/// Детерминизм-харнесс: `World` + `Schedule` + seeded-бот.
pub struct DeterminismHarness {
    world: World,
    schedule: Schedule,
    commands: Commands,
    bot: SeededBot,
    entity: Entity,
}

impl DeterminismHarness {
    /// Новый харнесс с seed бота.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let config = CharacterConfig::default();
        let state = CharacterState {
            position: DVec3::new(2.0, 1.0 + config.half_height + config.radius, 0.0),
            ..CharacterState::default()
        };

        let mut world = World::new();
        let entity = world.spawn_with_uuid(ENTITY_UUID);
        let inserted = world.insert(
            entity,
            CharacterBody {
                state,
                config,
                intention: Intention::IDLE,
                hit: false,
            },
        );
        debug_assert!(inserted.is_ok(), "fresh entity must accept component");
        world.insert_resource(harness_scene());
        world.insert_resource(PhysicsStep::default());

        let mut schedule = Schedule::with_stages(&[Stage::UPDATE, Stage::POST]);
        install_controller_schedule(&mut schedule);

        Self {
            world,
            schedule,
            commands: Commands::new(),
            bot: SeededBot::new(seed),
            entity,
        }
    }

    /// Один тик: бот → Controller → Physics → ControllerPost.
    pub fn step(&mut self) {
        let intention = self.bot.intention();
        if let Some(body) = self.world.get_mut::<CharacterBody>(self.entity) {
            body.intention = intention;
        }
        self.schedule.run(&mut self.world, &mut self.commands);
    }

    /// Прогон `ticks` тиков и снятие хеша.
    pub fn run(&mut self, ticks: u32) -> StateHash {
        for _ in 0..ticks {
            self.step();
        }
        self.state_hash()
    }

    /// Текущий тик мира.
    #[must_use]
    pub fn tick(&self) -> SimTick {
        self.world.tick()
    }

    /// Канонический хеш текущего состояния.
    #[must_use]
    pub fn state_hash(&self) -> StateHash {
        let Some(body) = self.world.get::<CharacterBody>(self.entity) else {
            return StateHash {
                hash: fnv1a(b""),
                tick: self.world.tick(),
                journal: SaveJournal::new(),
            };
        };

        let delta = ComponentDelta::new(ENTITY_UUID, CHARACTER_SCHEMA)
            .with_field(FieldValue::new(
                FIELD_POSITION,
                Value::DVec3([
                    body.state.position.x,
                    body.state.position.y,
                    body.state.position.z,
                ]),
            ))
            .with_field(FieldValue::new(
                FIELD_VELOCITY,
                Value::DVec3([
                    body.state.velocity.x,
                    body.state.velocity.y,
                    body.state.velocity.z,
                ]),
            ))
            .with_field(FieldValue::new(
                FIELD_GROUNDED,
                Value::Bool(body.state.grounded),
            ))
            .with_field(FieldValue::new(
                FIELD_GROUND_NORMAL,
                Value::DVec3([
                    body.state.ground_normal.x,
                    body.state.ground_normal.y,
                    body.state.ground_normal.z,
                ]),
            ))
            .with_field(FieldValue::new(
                FIELD_HYSTERESIS,
                Value::U32(u32::from(body.state.ground_hysteresis)),
            ))
            .with_field(FieldValue::new(
                FIELD_MOVE_AXIS,
                Value::DVec3([
                    body.intention.move_axis[0],
                    body.intention.move_axis[1],
                    0.0,
                ]),
            ))
            .with_field(FieldValue::new(
                FIELD_LOOK,
                Value::DVec3([body.intention.look[0], body.intention.look[1], 0.0]),
            ))
            .with_field(FieldValue::new(
                FIELD_JUMP,
                Value::Bool(body.intention.jump),
            ))
            .with_field(FieldValue::new(
                FIELD_CROUCH,
                Value::Bool(body.intention.crouch),
            ))
            .with_field(FieldValue::new(FIELD_HIT, Value::Bool(body.hit)));

        let mut journal = SaveJournal::new();
        journal.append(self.world.tick(), vec![delta]);
        let bytes = ron::ser::to_string(&journal)
            .unwrap_or_default()
            .into_bytes();

        StateHash {
            hash: fnv1a(&bytes),
            tick: self.world.tick(),
            journal,
        }
    }
}

fn first_diff(left: &SaveJournal, right: &SaveJournal) -> Option<StateDiff> {
    let entries = left.entries.len().max(right.entries.len());
    for index in 0..entries {
        match (left.entries.get(index), right.entries.get(index)) {
            (Some(left_entry), Some(right_entry)) => {
                if let Some(diff) =
                    diff_deltas(left_entry.tick, &left_entry.deltas, &right_entry.deltas)
                {
                    return Some(diff);
                }
            }
            (Some(entry), None) => {
                return Some(entry_diff(entry.tick, entry.deltas.first()));
            }
            (None, Some(entry)) => {
                return Some(entry_diff(entry.tick, entry.deltas.first()));
            }
            (None, None) => break,
        }
    }
    None
}

fn entry_diff(tick: SimTick, delta: Option<&ComponentDelta>) -> StateDiff {
    StateDiff {
        tick,
        entity: delta.map_or(EntityUuid(Uuid::nil()), |delta| delta.entity),
        schema: delta.map_or(CHARACTER_SCHEMA, |delta| delta.schema),
        field: None,
        left: None,
        right: None,
    }
}

fn diff_deltas(
    tick: SimTick,
    left: &[ComponentDelta],
    right: &[ComponentDelta],
) -> Option<StateDiff> {
    let count = left.len().max(right.len());
    for index in 0..count {
        match (left.get(index), right.get(index)) {
            (Some(left_delta), Some(right_delta)) => {
                if left_delta.entity != right_delta.entity
                    || left_delta.schema != right_delta.schema
                {
                    return Some(StateDiff {
                        tick,
                        entity: left_delta.entity,
                        schema: left_delta.schema,
                        field: None,
                        left: None,
                        right: None,
                    });
                }
                if let Some(diff) = diff_fields(tick, left_delta, right_delta) {
                    return Some(diff);
                }
            }
            (Some(delta), None) | (None, Some(delta)) => {
                return Some(StateDiff {
                    tick,
                    entity: delta.entity,
                    schema: delta.schema,
                    field: None,
                    left: None,
                    right: None,
                });
            }
            (None, None) => break,
        }
    }
    None
}

fn diff_fields(tick: SimTick, left: &ComponentDelta, right: &ComponentDelta) -> Option<StateDiff> {
    let count = left.fields.len().max(right.fields.len());
    for index in 0..count {
        match (left.fields.get(index), right.fields.get(index)) {
            (Some(left_field), Some(right_field))
                if left_field.field == right_field.field
                    && left_field.value == right_field.value => {}
            (Some(left_field), Some(right_field)) => {
                return Some(StateDiff {
                    tick,
                    entity: left.entity,
                    schema: left.schema,
                    field: Some(left_field.field),
                    left: Some(left_field.value.clone()),
                    right: Some(right_field.value.clone()),
                });
            }
            (Some(left_field), None) => {
                return Some(StateDiff {
                    tick,
                    entity: left.entity,
                    schema: left.schema,
                    field: Some(left_field.field),
                    left: Some(left_field.value.clone()),
                    right: None,
                });
            }
            (None, Some(right_field)) => {
                return Some(StateDiff {
                    tick,
                    entity: left.entity,
                    schema: left.schema,
                    field: Some(right_field.field),
                    left: None,
                    right: Some(right_field.value.clone()),
                });
            }
            (None, None) => break,
        }
    }
    None
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
