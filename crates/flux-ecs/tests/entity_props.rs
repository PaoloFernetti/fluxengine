//! Property-тесты I1: инварианты аллокатора, миграций и swap-remove.
//!
//! Модель: `HashMap<Entity, (Option<Position>, Option<Velocity>)>` сравнивается
//! с `World` после каждой операции; `World::validate` проверяет синхронность
//! чанков и столбцов.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;

use flux_core::schema::TypeId;
use flux_ecs::component::Component;
use flux_ecs::entity::Entity;
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
        TypeId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0020));
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Velocity {
    x: i32,
    y: i32,
}

impl Component for Velocity {
    const TYPE_ID: TypeId =
        TypeId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0021));
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 64,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    /// Любая последовательность spawn/despawn/insert/remove совпадает с моделью.
    #[test]
    fn world_matches_reference_model(ops in prop::collection::vec(any::<u8>(), 1..200)) {
        let mut world = World::new();
        let mut alive: Vec<Entity> = Vec::new();
        let mut model: HashMap<Entity, (Option<Position>, Option<Velocity>)> = HashMap::new();

        for op in ops {
            match op % 6 {
                0 | 1 => {
                    let entity = world.spawn();
                    prop_assert!(world.is_alive(entity));
                    alive.push(entity);
                    model.insert(entity, (None, None));
                }
                2 => {
                    if !alive.is_empty() {
                        let entity = alive.swap_remove(op as usize % alive.len());
                        prop_assert!(world.despawn(entity));
                        model.remove(&entity);
                        prop_assert!(!world.is_alive(entity));
                        prop_assert!(world.get::<Position>(entity).is_none());
                        prop_assert!(world.get::<Velocity>(entity).is_none());
                    }
                }
                3 => {
                    if !alive.is_empty() {
                        let entity = alive[op as usize % alive.len()];
                        let value = Position { x: i32::from(op), y: 1 };
                        prop_assert!(world.insert(entity, value).is_ok());
                        if let Some(slot) = model.get_mut(&entity) {
                            slot.0 = Some(value);
                        }
                    }
                }
                4 => {
                    if !alive.is_empty() {
                        let entity = alive[op as usize % alive.len()];
                        let value = Velocity { x: i32::from(op), y: 2 };
                        prop_assert!(world.insert(entity, value).is_ok());
                        if let Some(slot) = model.get_mut(&entity) {
                            slot.1 = Some(value);
                        }
                    }
                }
                _ => {
                    if !alive.is_empty() {
                        let entity = alive[op as usize % alive.len()];
                        if op % 2 == 0 {
                            let expected = model.get_mut(&entity).and_then(|slot| slot.0.take());
                            let removed = world.remove::<Position>(entity).unwrap();
                            prop_assert_eq!(removed, expected);
                        } else {
                            let expected = model.get_mut(&entity).and_then(|slot| slot.1.take());
                            let removed = world.remove::<Velocity>(entity).unwrap();
                            prop_assert_eq!(removed, expected);
                        }
                    }
                }
            }

            world.validate();
            prop_assert_eq!(world.entity_count(), alive.len());
            for &entity in &alive {
                let (position, velocity) = model.get(&entity).copied().unwrap_or_default();
                prop_assert_eq!(world.get::<Position>(entity).copied(), position);
                prop_assert_eq!(world.get::<Velocity>(entity).copied(), velocity);
            }
        }
    }

    /// Поколение слота строго возрастает: старый `Entity` не оживает.
    #[test]
    fn generations_never_repeat(steps in 1usize..64) {
        let mut world = World::new();
        let mut last: HashMap<u32, u32> = HashMap::new();

        for _ in 0..steps {
            let entity = world.spawn();
            if let Some(previous) = last.get(&entity.index()) {
                prop_assert!(entity.generation() > *previous);
            }
            last.insert(entity.index(), entity.generation());
            prop_assert!(world.despawn(entity));
            prop_assert!(!world.is_alive(entity));
        }
    }

    /// После произвольных despawn все живые сущности читаются без «дыр».
    #[test]
    fn swap_remove_leaves_no_holes(
        count in 1usize..96,
        kills in prop::collection::vec(any::<u8>(), 0..96),
    ) {
        let mut world = World::new();
        let mut alive = Vec::with_capacity(count);

        for index in 0..count {
            let entity = world.spawn();
            world
                .insert(entity, Position { x: index as i32, y: 0 })
                .unwrap();
            world
                .insert(entity, Velocity { x: 0, y: index as i32 })
                .unwrap();
            alive.push(entity);
        }

        for kill in kills {
            if alive.is_empty() {
                break;
            }
            let entity = alive.swap_remove(kill as usize % alive.len());
            prop_assert!(world.despawn(entity));
            world.validate();

            for &survivor in &alive {
                let position = world.get::<Position>(survivor).copied();
                let velocity = world.get::<Velocity>(survivor).copied();
                let position = position.expect("position column must stay dense");
                let velocity = velocity.expect("velocity column must stay dense");
                // Столбцы остаются синхронными: x из Position == y из Velocity.
                prop_assert_eq!(position.x, velocity.y);
            }
        }
    }
}
