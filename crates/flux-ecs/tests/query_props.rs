//! Property-тесты I2: запросы, фильтры `With`/`Without`, батч-мутация
//! и change detection согласованы с эталонной моделью.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use flux_core::schema::TypeId;
use flux_ecs::component::Component;
use flux_ecs::entity::Entity;
use flux_ecs::query::QuerySpec;
use flux_ecs::world::World;
use proptest::prelude::*;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Position {
    x: i32,
    y: i32,
}

impl Component for Position {
    const TYPE_ID: TypeId =
        TypeId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0040));
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Velocity {
    x: i32,
    y: i32,
}

impl Component for Velocity {
    const TYPE_ID: TypeId =
        TypeId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0041));
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 64,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    /// `Query`/`With`/`Without` и `QueryMut` совпадают с моделью.
    #[test]
    fn queries_match_reference_model(ops in prop::collection::vec(any::<u8>(), 1..160)) {
        let mut world = World::new();
        world.advance_tick();
        let mut alive: Vec<Entity> = Vec::new();
        let mut model: HashMap<Entity, (Option<Position>, Option<Velocity>)> = HashMap::new();

        for op in ops {
            match op % 8 {
                0 | 1 => {
                    let entity = world.spawn();
                    alive.push(entity);
                    model.insert(entity, (None, None));
                }
                2 => {
                    if !alive.is_empty() {
                        let entity = alive.swap_remove(op as usize % alive.len());
                        prop_assert!(world.despawn(entity));
                        model.remove(&entity);
                    }
                }
                3 | 4 => {
                    if !alive.is_empty() {
                        let entity = alive[op as usize % alive.len()];
                        let value = Position { x: i32::from(op), y: 1 };
                        prop_assert!(world.insert(entity, value).is_ok());
                        if let Some(slot) = model.get_mut(&entity) {
                            slot.0 = Some(value);
                        }
                    }
                }
                5 => {
                    if !alive.is_empty() {
                        let entity = alive[op as usize % alive.len()];
                        let value = Velocity { x: i32::from(op), y: 2 };
                        prop_assert!(world.insert(entity, value).is_ok());
                        if let Some(slot) = model.get_mut(&entity) {
                            slot.1 = Some(value);
                        }
                    }
                }
                6 => {
                    if !alive.is_empty() {
                        let entity = alive[op as usize % alive.len()];
                        let expected = model.get_mut(&entity).and_then(|slot| slot.0.take());
                        let removed = world.remove::<Position>(entity).unwrap();
                        prop_assert_eq!(removed, expected);
                    }
                }
                _ => {
                    if !alive.is_empty() {
                        let entity = alive[op as usize % alive.len()];
                        let expected = model.get_mut(&entity).and_then(|slot| slot.1.take());
                        let removed = world.remove::<Velocity>(entity).unwrap();
                        prop_assert_eq!(removed, expected);
                    }
                }
            }

            let mut with_position: Vec<(Entity, Position)> = world
                .query::<Position>()
                .map(|(entity, value)| (entity, *value))
                .collect();
            with_position.sort_by_key(|(entity, _)| *entity);

            let mut expected: Vec<(Entity, Position)> = alive
                .iter()
                .filter_map(|entity| model.get(entity)?.0.map(|value| (*entity, value)))
                .collect();
            expected.sort_by_key(|(entity, _)| *entity);
            prop_assert_eq!(with_position, expected);

            let without_velocity: Vec<Entity> = world
                .query_filtered::<Position>(&QuerySpec::new().without::<Velocity>())
                .map(|(entity, _)| entity)
                .collect();
            let mut expected_without: Vec<Entity> = alive
                .iter()
                .filter_map(|entity| {
                    let (position, velocity) = model.get(entity)?;
                    (position.is_some() && velocity.is_none()).then_some(*entity)
                })
                .collect();
            expected_without.sort_unstable();
            let mut actual_without = without_velocity;
            actual_without.sort_unstable();
            prop_assert_eq!(actual_without, expected_without);

            world.validate();
        }
    }

    /// Батч-мутация `QueryMut` покрывает все сущности ровно один раз.
    #[test]
    fn query_mut_visits_every_entity_once(
        count in 1usize..80,
        batch_size in 1usize..16,
    ) {
        let mut world = World::new();
        world.advance_tick();
        let entities = world.spawn_batch(count);
        for (index, entity) in entities.iter().enumerate() {
            world.insert(*entity, Position { x: index as i32, y: 0 }).unwrap();
        }

        let mut visits: HashMap<Entity, usize> = HashMap::new();
        world.query_mut::<Position>().for_each_batch(batch_size, |batch, values| {
            assert_eq!(batch.len(), values.len());
            for (entity, value) in batch.iter().zip(values.iter_mut()) {
                *visits.entry(*entity).or_default() += 1;
                value.x += 10;
            }
        });

        prop_assert_eq!(visits.len(), count);
        prop_assert!(visits.values().all(|visits| *visits == 1));

        for (index, entity) in entities.iter().enumerate() {
            prop_assert_eq!(
                world.get::<Position>(*entity),
                Some(&Position { x: index as i32 + 10, y: 0 })
            );
        }
    }

    /// `Added`/`Changed` видят только мутации текущего тика.
    #[test]
    fn added_changed_are_scoped_to_tick(mutations in prop::collection::vec(any::<u8>(), 1..80)) {
        let mut world = World::new();
        world.advance_tick();
        let entity = world.spawn();
        world.insert(entity, Position::default()).unwrap();

        let mut last_run = world.tick();
        for op in mutations {
            world.advance_tick();
            let tick = world.tick();
            let changed = op % 2 == 0;
            if changed {
                let value = Position { x: i32::from(op), y: 0 };
                world.insert(entity, value).unwrap();
                prop_assert_eq!(world.query::<Position>().changed_since(last_run).count(), 1);
                prop_assert_eq!(world.query::<Position>().added_since(last_run).count(), 0);
            } else {
                prop_assert_eq!(world.query::<Position>().changed_since(last_run).count(), 0);
                prop_assert_eq!(world.query::<Position>().added_since(last_run).count(), 0);
            }
            prop_assert_eq!(world.query::<Position>().added_since(tick).count(), 0);
            last_run = tick;
        }
    }
}
