//! Отложенные команды: `spawn`/`despawn`/`insert`/`remove`.
//!
//! Команды ставятся в очередь системой и применяются на барьере фазы
//! (`Commands::apply`). Порядок применения — FIFO, то есть порядок
//! постановки; это делает результат детерминированным (ADR-0030).
//!
//! Ссылка на сущность до применения — [`CommandTarget`]: существующая
//! [`Entity`] или `Deferred`-хэндл, выданный [`Commands::spawn`].

use std::collections::HashMap;

use crate::component::Component;
use crate::entity::{Entity, EntityUuid};
use crate::world::{EcsError, World};

/// Ссылка на сущность в команде.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandTarget {
    /// Сущность уже живёт в мире.
    Existing(Entity),
    /// Сущность, которую создаст команда `Spawn` из этой же очереди.
    Deferred(u32),
}

type ApplyFn = Box<dyn FnOnce(Entity, &mut World) -> Result<(), EcsError> + Send>;

enum Command {
    Spawn {
        handle: u32,
        uuid: Option<EntityUuid>,
    },
    Despawn(CommandTarget),
    Insert {
        target: CommandTarget,
        apply: ApplyFn,
    },
    Remove {
        target: CommandTarget,
        apply: ApplyFn,
    },
}

/// Очередь отложенных операций.
#[derive(Default)]
pub struct Commands {
    queue: Vec<Command>,
    next_handle: u32,
}

impl Commands {
    /// Создаёт пустую очередь.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Число команд в очереди.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Пуста ли очередь.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Ставит спавн со случайным UUID; возвращает хэндл для последующих команд.
    pub fn spawn(&mut self) -> CommandTarget {
        self.spawn_inner(None)
    }

    /// Ставит спавн с заданным UUID (сеть, сейвы).
    pub fn spawn_with_uuid(&mut self, uuid: EntityUuid) -> CommandTarget {
        self.spawn_inner(Some(uuid))
    }

    fn spawn_inner(&mut self, uuid: Option<EntityUuid>) -> CommandTarget {
        let handle = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1);
        self.queue.push(Command::Spawn { handle, uuid });
        CommandTarget::Deferred(handle)
    }

    /// Ставит удаление сущности.
    pub fn despawn(&mut self, target: CommandTarget) {
        self.queue.push(Command::Despawn(target));
    }

    /// Ставит вставку компонента.
    pub fn insert<T: Component>(&mut self, target: CommandTarget, value: T) {
        self.queue.push(Command::Insert {
            target,
            apply: Box::new(move |entity, world| world.insert(entity, value)),
        });
    }

    /// Ставит удаление компонента (значение отбрасывается).
    pub fn remove<T: Component>(&mut self, target: CommandTarget) {
        self.queue.push(Command::Remove {
            target,
            apply: Box::new(move |entity, world| world.remove::<T>(entity).map(|_| ())),
        });
    }

    /// Очищает очередь без применения.
    pub fn clear(&mut self) {
        self.queue.clear();
        self.next_handle = 0;
    }

    /// Применяет очередь к миру в порядке постановки.
    ///
    /// Ошибки не прерывают применение: они собираются в отчёте, чтобы
    /// детерминированно обработать мёртвые ссылки на барьере.
    pub fn apply(&mut self, world: &mut World) -> CommandsReport {
        let mut report = CommandsReport::default();
        let mut resolved: HashMap<u32, Entity> = HashMap::new();

        for command in self.queue.drain(..) {
            match command {
                Command::Spawn { handle, uuid } => {
                    let entity = match uuid {
                        Some(uuid) => world.spawn_with_uuid(uuid),
                        None => world.spawn(),
                    };
                    resolved.insert(handle, entity);
                    report.applied += 1;
                }
                Command::Despawn(target) => {
                    let Some(entity) = resolve(target, &resolved) else {
                        report.fail(EcsError::DeadEntity);
                        continue;
                    };
                    if world.despawn(entity) {
                        report.applied += 1;
                    } else {
                        report.fail(EcsError::DeadEntity);
                    }
                }
                Command::Insert { target, apply } => {
                    let Some(entity) = resolve(target, &resolved) else {
                        report.fail(EcsError::DeadEntity);
                        continue;
                    };
                    match apply(entity, world) {
                        Ok(()) => report.applied += 1,
                        Err(error) => report.fail(error),
                    }
                }
                Command::Remove { target, apply } => {
                    let Some(entity) = resolve(target, &resolved) else {
                        report.fail(EcsError::DeadEntity);
                        continue;
                    };
                    match apply(entity, world) {
                        Ok(()) => report.applied += 1,
                        Err(error) => report.fail(error),
                    }
                }
            }
        }

        self.next_handle = 0;
        report
    }
}

fn resolve(target: CommandTarget, resolved: &HashMap<u32, Entity>) -> Option<Entity> {
    match target {
        CommandTarget::Existing(entity) => Some(entity),
        CommandTarget::Deferred(handle) => resolved.get(&handle).copied(),
    }
}

/// Результат применения очереди.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CommandsReport {
    /// Успешно применённые команды.
    pub applied: usize,
    /// Ошибки применения (по порядку команд).
    pub errors: Vec<EcsError>,
}

impl CommandsReport {
    /// Число неуспешных команд.
    #[must_use]
    pub fn failed(&self) -> usize {
        self.errors.len()
    }

    fn fail(&mut self, error: EcsError) {
        self.errors.push(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::Component;
    use crate::world::World;
    use flux_core::schema::TypeId;
    use uuid::Uuid;

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct Position {
        x: f32,
        y: f32,
    }

    impl Component for Position {
        const TYPE_ID: TypeId =
            TypeId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0030));
    }

    #[test]
    fn deferred_spawn_resolves_for_later_commands() {
        let mut world = World::new();
        let mut commands = Commands::new();
        let target = commands.spawn();
        commands.insert(target, Position { x: 4.0, y: 5.0 });

        assert_eq!(world.entity_count(), 0);
        let report = commands.apply(&mut world);
        assert_eq!(report.applied, 2);
        assert_eq!(world.entity_count(), 1);

        let entity = world.query::<Position>().next().map(|(entity, _)| entity);
        assert_eq!(
            world.get::<Position>(entity.unwrap()),
            Some(&Position { x: 4.0, y: 5.0 })
        );
    }

    #[test]
    fn fifo_order_is_preserved() {
        let mut world = World::new();
        let existing = world.spawn();
        world.insert(existing, Position::default()).unwrap();

        let mut commands = Commands::new();
        commands.remove::<Position>(CommandTarget::Existing(existing));
        commands.insert(
            CommandTarget::Existing(existing),
            Position { x: 9.0, y: 0.0 },
        );

        let report = commands.apply(&mut world);
        assert_eq!(report.applied, 2);
        assert_eq!(
            world.get::<Position>(existing),
            Some(&Position { x: 9.0, y: 0.0 })
        );
    }

    #[test]
    fn dead_target_is_reported_without_aborting_queue() {
        let mut world = World::new();
        let dead = world.spawn();
        assert!(world.despawn(dead));

        let mut commands = Commands::new();
        commands.insert(CommandTarget::Existing(dead), Position::default());
        let live = commands.spawn();
        commands.insert(live, Position::default());

        let report = commands.apply(&mut world);
        assert_eq!(report.applied, 2);
        assert_eq!(report.failed(), 1);
        assert_eq!(report.errors, vec![EcsError::DeadEntity]);
        assert_eq!(world.entity_count(), 1);
    }

    #[test]
    fn despawn_removes_entity() {
        let mut world = World::new();
        let entity = world.spawn();
        let mut commands = Commands::new();
        commands.despawn(CommandTarget::Existing(entity));
        let report = commands.apply(&mut world);
        assert_eq!(report.applied, 1);
        assert!(!world.is_alive(entity));
    }
}
