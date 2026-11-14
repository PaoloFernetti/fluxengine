//! Детерминированный однопоточный планировщик.
//!
//! Порядок систем внутри стадии задаётся `before`/`after`, тай-брейк —
//! имя системы (не порядок регистрации). Стадии исполняются в порядке
//! первого появления (или явно заданном). `Commands` применяются на
//! границе каждой стадии; конфликты read/write только диагностируются —
//! параллельного исполнения на этапе 2 нет (ADR-0029).
//!
//! `TickRate` — детерминированный пропуск по `SimTick`: `Every`,
//! `EveryN(n)`, `Once`. Это задел под AI-LOD (№18), а не «каждая сущность
//! каждый тик».

use std::collections::{BTreeMap, BTreeSet};

use flux_core::schema::TypeId;
use flux_core::time::SimTick;
use thiserror::Error;

use crate::commands::{Commands, CommandsReport};
use crate::component::Component;
use crate::world::World;

/// Стадия кадра: явный барьер между группами систем.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Stage(&'static str);

impl Stage {
    /// Ввод и подготовка данных.
    pub const INPUT: Self = Self("input");
    /// Симуляция (основная стадия).
    pub const UPDATE: Self = Self("update");
    /// Пост-обработка: применение команд, очистка.
    pub const POST: Self = Self("post");

    /// Имя стадии.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.0
    }
}

impl std::fmt::Display for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// Объявленный доступ системы к компонентам (для диагностики конфликтов).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Access {
    /// Читаемые типы.
    pub reads: Vec<TypeId>,
    /// Изменяемые типы.
    pub writes: Vec<TypeId>,
}

impl Access {
    /// Пустой доступ.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Добавляет чтение `T`.
    #[must_use]
    pub fn read<T: Component>(mut self) -> Self {
        self.reads.push(T::TYPE_ID);
        self
    }

    /// Добавляет запись `T`.
    #[must_use]
    pub fn write<T: Component>(mut self) -> Self {
        self.writes.push(T::TYPE_ID);
        self
    }
}

/// Частота запуска системы.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TickRate {
    /// Каждый тик.
    #[default]
    Every,
    /// Каждый `n`-й тик (`n = 0` трактуется как 1).
    EveryN(u32),
    /// Один раз за жизнь расписания.
    Once,
}

impl TickRate {
    fn should_run(self, tick: SimTick, last_run: Option<SimTick>) -> bool {
        match self {
            Self::Every => true,
            Self::EveryN(n) => tick.get() % u64::from(n.max(1)) == 0,
            Self::Once => last_run.is_none(),
        }
    }
}

/// Описание системы для планировщика.
#[derive(Clone, Debug)]
pub struct SystemDescriptor {
    /// Уникальное имя (тай-брейк и ссылки before/after).
    pub name: &'static str,
    /// Стадия.
    pub stage: Stage,
    /// Доступ к компонентам.
    pub access: Access,
    /// Системы, которые должны выполниться после этой.
    pub before: Vec<&'static str>,
    /// Системы, которые должны выполниться до этой.
    pub after: Vec<&'static str>,
    /// Частота запуска.
    pub tick_rate: TickRate,
}

impl SystemDescriptor {
    /// Описание системы в стадии.
    #[must_use]
    pub fn new(name: &'static str, stage: Stage) -> Self {
        Self {
            name,
            stage,
            access: Access::new(),
            before: Vec::new(),
            after: Vec::new(),
            tick_rate: TickRate::Every,
        }
    }

    /// Задаёт доступ.
    #[must_use]
    pub fn access(mut self, access: Access) -> Self {
        self.access = access;
        self
    }

    /// Системы, которые должны выполниться после этой.
    #[must_use]
    pub fn before(mut self, names: &[&'static str]) -> Self {
        self.before.extend_from_slice(names);
        self
    }

    /// Системы, которые должны выполниться до этой.
    #[must_use]
    pub fn after(mut self, names: &[&'static str]) -> Self {
        self.after.extend_from_slice(names);
        self
    }

    /// Частота запуска.
    #[must_use]
    pub fn tick_rate(mut self, rate: TickRate) -> Self {
        self.tick_rate = rate;
        self
    }
}

/// Тип конфликта доступа.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConflictKind {
    /// Обе системы пишут.
    WriteWrite,
    /// Первая пишет, вторая читает.
    WriteRead,
    /// Первая читает, вторая пишет.
    ReadWrite,
}

/// Диагностированный конфликт двух систем в порядке исполнения.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    /// Первая система.
    pub first: &'static str,
    /// Вторая система.
    pub second: &'static str,
    /// Тип конфликта.
    pub kind: ConflictKind,
}

/// Ошибка планирования.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ScheduleError {
    /// Две системы в стадии имеют одинаковое имя.
    #[error("duplicate system name: {0}")]
    DuplicateName(&'static str),
    /// `before`/`after` ссылается на систему вне стадии.
    #[error("unknown system in dependency: {0}")]
    UnknownDependency(&'static str),
    /// Цикл в before/after.
    #[error("dependency cycle in stage {0}")]
    Cycle(&'static str),
}

/// Отчёт по стадии.
#[derive(Debug, PartialEq, Eq)]
pub struct StageReport {
    /// Стадия.
    pub stage: Stage,
    /// Порядок имён систем.
    pub order: Vec<&'static str>,
    /// Сколько систем выполнено.
    pub executed: usize,
    /// Сколько пропущено по `TickRate`.
    pub skipped: usize,
    /// Диагностированные конфликты.
    pub conflicts: Vec<Conflict>,
    /// Результат применения команд на барьере.
    pub commands: CommandsReport,
}

/// Отчёт по всему проходу расписания.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScheduleReport {
    /// Стадии в порядке исполнения.
    pub stages: Vec<StageReport>,
    /// Ошибки планирования (стадия с ошибкой не исполнялась).
    pub errors: Vec<ScheduleError>,
}

type SystemFn = Box<dyn FnMut(&mut World, &mut Commands) + Send>;

struct System {
    descriptor: SystemDescriptor,
    run: SystemFn,
    last_run: Option<SimTick>,
}

/// Расписание: системы, стадии, детерминированный порядок.
#[derive(Default)]
pub struct Schedule {
    systems: Vec<System>,
    stages: Vec<Stage>,
}

impl Schedule {
    /// Пустое расписание.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Расписание с явным порядком стадий.
    #[must_use]
    pub fn with_stages(stages: &[Stage]) -> Self {
        Self {
            systems: Vec::new(),
            stages: stages.to_vec(),
        }
    }

    /// Порядок стадий.
    #[must_use]
    pub fn stages(&self) -> &[Stage] {
        &self.stages
    }

    /// Число систем.
    #[must_use]
    pub fn len(&self) -> usize {
        self.systems.len()
    }

    /// Нет ли систем.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.systems.is_empty()
    }

    /// Добавляет систему.
    pub fn add<F>(&mut self, descriptor: SystemDescriptor, run: F)
    where
        F: FnMut(&mut World, &mut Commands) + Send + 'static,
    {
        if !self.stages.contains(&descriptor.stage) {
            self.stages.push(descriptor.stage);
        }
        self.systems.push(System {
            descriptor,
            run: Box::new(run),
            last_run: None,
        });
    }

    /// Порядок имён систем в стадии.
    ///
    /// # Errors
    ///
    /// Дубликат имени, неизвестная зависимость или цикл.
    pub fn plan(&self, stage: Stage) -> Result<Vec<&'static str>, ScheduleError> {
        let order = self.plan_indices(stage)?;
        Ok(order
            .into_iter()
            .map(|index| self.systems[index].descriptor.name)
            .collect())
    }

    /// Конфликты доступа в порядке исполнения стадии.
    ///
    /// # Errors
    ///
    /// Те же, что у [`Schedule::plan`].
    pub fn conflicts(&self, stage: Stage) -> Result<Vec<Conflict>, ScheduleError> {
        let order = self.plan_indices(stage)?;
        Ok(self.collect_conflicts(&order))
    }

    /// Исполняет один тик: продвигает `SimTick`, проходит стадии,
    /// применяет `Commands` на границе каждой стадии.
    pub fn run(&mut self, world: &mut World, commands: &mut Commands) -> ScheduleReport {
        world.advance_tick();
        let tick = world.tick();
        let mut report = ScheduleReport::default();
        let stages = self.stages.clone();

        for stage in stages {
            let order = match self.plan_indices(stage) {
                Ok(order) => order,
                Err(error) => {
                    report.errors.push(error);
                    continue;
                }
            };

            let mut stage_report = StageReport {
                stage,
                order: order
                    .iter()
                    .map(|index| self.systems[*index].descriptor.name)
                    .collect(),
                executed: 0,
                skipped: 0,
                conflicts: self.collect_conflicts(&order),
                commands: CommandsReport::default(),
            };

            for index in order {
                let system = &mut self.systems[index];
                if !system
                    .descriptor
                    .tick_rate
                    .should_run(tick, system.last_run)
                {
                    stage_report.skipped += 1;
                    continue;
                }
                (system.run)(world, commands);
                system.last_run = Some(tick);
                stage_report.executed += 1;
            }

            stage_report.commands = commands.apply(world);
            report.stages.push(stage_report);
        }

        report
    }

    fn plan_indices(&self, stage: Stage) -> Result<Vec<usize>, ScheduleError> {
        let indices: Vec<usize> = self
            .systems
            .iter()
            .enumerate()
            .filter(|(_, system)| system.descriptor.stage == stage)
            .map(|(index, _)| index)
            .collect();

        let mut by_name: BTreeMap<&'static str, usize> = BTreeMap::new();
        for (position, &index) in indices.iter().enumerate() {
            let name = self.systems[index].descriptor.name;
            if by_name.insert(name, position).is_some() {
                return Err(ScheduleError::DuplicateName(name));
            }
        }

        let mut indegree = vec![0_usize; indices.len()];
        let mut edges: Vec<Vec<usize>> = vec![Vec::new(); indices.len()];

        for (position, &index) in indices.iter().enumerate() {
            for dependency in &self.systems[index].descriptor.before {
                let target = *by_name
                    .get(dependency)
                    .ok_or(ScheduleError::UnknownDependency(dependency))?;
                edges[position].push(target);
                indegree[target] += 1;
            }
            for dependency in &self.systems[index].descriptor.after {
                let source = *by_name
                    .get(dependency)
                    .ok_or(ScheduleError::UnknownDependency(dependency))?;
                edges[source].push(position);
                indegree[position] += 1;
            }
        }

        let mut ready: BTreeSet<(&'static str, usize)> = indegree
            .iter()
            .enumerate()
            .filter(|(_, degree)| **degree == 0)
            .map(|(position, _)| {
                let index = indices[position];
                (self.systems[index].descriptor.name, position)
            })
            .collect();

        let mut order = Vec::with_capacity(indices.len());
        while let Some((_, position)) = ready.pop_first() {
            order.push(indices[position]);
            for &next in &edges[position] {
                indegree[next] -= 1;
                if indegree[next] == 0 {
                    let index = indices[next];
                    ready.insert((self.systems[index].descriptor.name, next));
                }
            }
        }

        if order.len() != indices.len() {
            return Err(ScheduleError::Cycle(stage.name()));
        }
        Ok(order)
    }

    fn collect_conflicts(&self, order: &[usize]) -> Vec<Conflict> {
        let mut conflicts = Vec::new();
        for (position, &first) in order.iter().enumerate() {
            for &second in &order[position + 1..] {
                let first = &self.systems[first].descriptor;
                let second = &self.systems[second].descriptor;
                let kind = if intersects(&first.access.writes, &second.access.writes) {
                    Some(ConflictKind::WriteWrite)
                } else if intersects(&first.access.writes, &second.access.reads) {
                    Some(ConflictKind::WriteRead)
                } else if intersects(&first.access.reads, &second.access.writes) {
                    Some(ConflictKind::ReadWrite)
                } else {
                    None
                };
                if let Some(kind) = kind {
                    conflicts.push(Conflict {
                        first: first.name,
                        second: second.name,
                        kind,
                    });
                }
            }
        }
        conflicts
    }
}

fn intersects(first: &[TypeId], second: &[TypeId]) -> bool {
    first
        .iter()
        .any(|candidate| second.iter().any(|other| candidate == other))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_core::schema::TypeId;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use uuid::Uuid;

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct Position {
        x: f32,
    }

    impl Component for Position {
        const TYPE_ID: TypeId =
            TypeId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0050));
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct Velocity {
        x: f32,
    }

    impl Component for Velocity {
        const TYPE_ID: TypeId =
            TypeId::from_uuid(Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0051));
    }

    fn noop(_: &mut World, _: &mut Commands) {}

    #[test]
    fn order_respects_dependencies_and_name_tiebreak() {
        let mut schedule = Schedule::new();
        schedule.add(SystemDescriptor::new("gamma", Stage::UPDATE), noop);
        schedule.add(
            SystemDescriptor::new("alpha", Stage::UPDATE).after(&["beta"]),
            noop,
        );
        schedule.add(SystemDescriptor::new("beta", Stage::UPDATE), noop);

        let order = schedule.plan(Stage::UPDATE).unwrap();
        assert_eq!(order, vec!["beta", "alpha", "gamma"]);
        assert_eq!(schedule.plan(Stage::UPDATE).unwrap(), order);
    }

    #[test]
    fn before_and_after_are_symmetric() {
        let mut schedule = Schedule::new();
        schedule.add(
            SystemDescriptor::new("first", Stage::UPDATE).before(&["second"]),
            noop,
        );
        schedule.add(SystemDescriptor::new("second", Stage::UPDATE), noop);
        assert_eq!(
            schedule.plan(Stage::UPDATE).unwrap(),
            vec!["first", "second"]
        );
    }

    #[test]
    fn cycle_is_reported() {
        let mut schedule = Schedule::new();
        schedule.add(
            SystemDescriptor::new("a", Stage::UPDATE).after(&["b"]),
            noop,
        );
        schedule.add(
            SystemDescriptor::new("b", Stage::UPDATE).after(&["a"]),
            noop,
        );
        assert_eq!(
            schedule.plan(Stage::UPDATE),
            Err(ScheduleError::Cycle("update"))
        );
    }

    #[test]
    fn unknown_dependency_and_duplicates_are_reported() {
        let mut schedule = Schedule::new();
        schedule.add(
            SystemDescriptor::new("a", Stage::UPDATE).after(&["missing"]),
            noop,
        );
        assert_eq!(
            schedule.plan(Stage::UPDATE),
            Err(ScheduleError::UnknownDependency("missing"))
        );

        let mut schedule = Schedule::new();
        schedule.add(SystemDescriptor::new("dup", Stage::UPDATE), noop);
        schedule.add(SystemDescriptor::new("dup", Stage::UPDATE), noop);
        assert_eq!(
            schedule.plan(Stage::UPDATE),
            Err(ScheduleError::DuplicateName("dup"))
        );
    }

    #[test]
    fn conflicts_are_diagnosed_without_reordering() {
        let mut schedule = Schedule::new();
        schedule.add(
            SystemDescriptor::new("writer", Stage::UPDATE)
                .access(Access::new().write::<Position>()),
            noop,
        );
        schedule.add(
            SystemDescriptor::new("reader", Stage::UPDATE).access(Access::new().read::<Position>()),
            noop,
        );
        schedule.add(
            SystemDescriptor::new("velocity", Stage::UPDATE)
                .access(Access::new().write::<Velocity>()),
            noop,
        );

        let conflicts = schedule.conflicts(Stage::UPDATE).unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].first, "reader");
        assert_eq!(conflicts[0].second, "writer");
        assert_eq!(conflicts[0].kind, ConflictKind::ReadWrite);
        assert_eq!(
            schedule.plan(Stage::UPDATE).unwrap(),
            vec!["reader", "velocity", "writer"]
        );
    }

    #[test]
    fn tick_rate_skips_deterministically() {
        let mut world = World::new();
        let mut commands = Commands::new();
        let every_n = Arc::new(AtomicUsize::new(0));
        let once = Arc::new(AtomicUsize::new(0));

        let mut schedule = Schedule::new();
        let counter = Arc::clone(&every_n);
        schedule.add(
            SystemDescriptor::new("every_two", Stage::UPDATE).tick_rate(TickRate::EveryN(2)),
            move |_, _| {
                counter.fetch_add(1, Ordering::Relaxed);
            },
        );
        let counter = Arc::clone(&once);
        schedule.add(
            SystemDescriptor::new("once", Stage::UPDATE).tick_rate(TickRate::Once),
            move |_, _| {
                counter.fetch_add(1, Ordering::Relaxed);
            },
        );

        for _ in 0..4 {
            schedule.run(&mut world, &mut commands);
        }

        assert_eq!(every_n.load(Ordering::Relaxed), 2);
        assert_eq!(once.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn commands_are_applied_on_stage_barrier() {
        let mut world = World::new();
        let mut commands = Commands::new();

        let mut schedule = Schedule::with_stages(&[Stage::UPDATE, Stage::POST]);
        schedule.add(
            SystemDescriptor::new("spawner", Stage::UPDATE),
            |_, commands| {
                let target = commands.spawn();
                commands.insert(target, Position { x: 1.0 });
            },
        );
        schedule.add(
            SystemDescriptor::new("observer", Stage::POST),
            |world, _| {
                assert_eq!(world.query::<Position>().count(), 1);
            },
        );

        let report = schedule.run(&mut world, &mut commands);
        assert!(report.errors.is_empty());
        assert_eq!(report.stages.len(), 2);
        assert_eq!(report.stages[0].commands.applied, 2);
        assert_eq!(world.entity_count(), 1);
    }
}
