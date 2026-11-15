# flux-ecs

Архетипная ECS: сущности, компоненты, запросы, планировщик, SchemaRegistry
и SaveState-дельты. Сердце данных движка.

Канон: [`docs/canon.md`](../../docs/canon.md). Архитектура: [`docs/architecture.md`](../../docs/architecture.md),
раздел «Собственная архетипная ECS». Roadmap: этап 2 (WP 2.1–2.9).
Детали: [`plans/stage-2-ecs.md`](../../plans/stage-2-ecs.md),
[`plans/stage-2-archetypes-delta.md`](../../plans/stage-2-archetypes-delta.md).

## Ответственность одной строкой

Хранение компонентов по архетипам (SoA-чанки), запросы с change detection,
детерминированный планировщик и схемы компонентов как данные.

## Инварианты

- `Entity = index + generation`; generation не реюзится (ADR-0028/0030 — формат).
- `entity_uuid` стабилен при миграциях архетипа.
- Порядок компонентов — по `ComponentIndex` (порядок регистрации; канонический
  порядок через `freeze`/сортировку `type_uuid` — TODO до I4).
- Порядок систем — тай-брейк по имени, не по регистрации.
- Схемы, поля и дельты обходятся по UUID; `HashMap`-итерация не влияет на вывод.
- `WorldPos` — канон координат; f32-позиции только внутри `SimRegion`.

## Этап

Каркас — I0; реализация — I1–I5 плана исполнения.

- **I1 (готово)**: `component` (`Component`, `Column`, `ColumnVec`),
  `entity` (`Entity`, `EntityAllocator`, `Location`, `EntityUuid`),
  `archetypes` (`Signature`, `ComponentRegistry`, `Chunk`, `Archetype`),
  `world` (`World`, `EcsError`). Property-тесты — `tests/entity_props.rs`.
- **I2 (готово)**: `query` (`Query`, `QuerySpec`, `QueryCache`, `QueryMut`),
  `change` (`ComponentTick`, `LastRun`), `commands` (`Commands`,
  `CommandTarget`, `CommandsReport`), `resources` (`Resources`); тик и
  ресурсы в `World`. Property-тесты — `tests/query_props.rs`.
- **I3 (готово)**: `scheduler` (`Stage`, `Access`, `SystemDescriptor`,
  `TickRate`, `Schedule`, `ScheduleReport`, `Conflict`); порядок —
  топосортировка before/after + имя, `Commands` применяются на барьере
  стадии. Property-тесты — `tests/schedule_props.rs`.
- **I4 (готово)**: `schema` (`FieldType`, `EditorMeta`, `OrphanPolicy`,
  `ComponentSchema`, `SchemaRegistry`), `save` (`Value`, `FieldValue`,
  `ComponentDelta`, `SaveJournal`, `OrphanedField`), `sim_time`
  (`SimTimeComponent`). Дерайв `flux_macros::Reflect` + compile-fail
  (`tests/ui`), тесты `tests/reflect_derive.rs`.
- **I5 (готово)**: `benches/ecs.rs` (read/write/three-systems, 10k),
  `examples/m2.rs` (M2 + G2), property-тесты `entity_props`, `query_props`,
  `schedule_props`. G2: ~590 µs/тик при 10k × 3 системы (порог 2 мс).
- **Вне этапа 2**: `events` пока содержит только документацию и
  `TODO(WP …)`; интеграция с `flux-engine` — этап 3.
