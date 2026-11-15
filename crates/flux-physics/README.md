# flux-physics

Физика f64, collision, KCC.

Канон: [docs/canon.md](../../docs/canon.md). Архитектура: [docs/architecture.md](../../docs/architecture.md).

## Ответственность одной строкой

Collision-минимум и KCC-минимум на `f64`: запросы пересечений/свипов без
solver и чистая функция `move_character` (ADR-0032, ADR-0024).

## Модули

| Модуль | Содержание |
|---|---|
| [`collision`](src/collision/mod.rs) | `TileGrid` (ASCII-сцена), `Capsule`, `Ray`, `Contact`, `sweep_capsule`, `raycast` |
| [`kcc`](src/kcc/mod.rs) | `CharacterState`, `CharacterConfig`, `move_character`, `MoveResult` |

## Инварианты

1. **Без solver.** Запросы возвращают `Contact`; коррекция и решение о
   движении — в KCC. Полный solver — этап 6, контракт запросов сохраняется.
2. **Детерминированный обход.** Тайлы перебираются по `(y, x)`; при равном
   времени контакта побеждает меньший индекс. Никаких `HashMap`.
3. **f64 везде.** Позиции/скорости/свипы — `DVec3`; канон `WorldPos`
   конвертируется на границе тика (ADR-0008).
4. **KCC — чистая функция.** `move_character(state, config, intention, dt, grid)`
   не хранит скрытых полей; всё состояние — в `CharacterState`.
5. **Порядок тика.** Input → Controller → Physics → ControllerPost.

## Особенности реализации

- Свип капсулы использует зазор `SKIN = 1e-4`: капсула останавливается чуть
  раньше поверхности, чтобы горизонтальное скольжение по полу не считалось
  столкновением (защита от f64-погрешности на границе).
- `raycast::time` — дистанция вдоль нормализованного луча;
  `sweep_capsule::time` — доля `0..1` вдоль перемещения.
- `TileGrid::from_rows` читает ASCII: `#` — твёрдая ячейка, первая строка —
  верхний ряд (`y = height - 1`).

## Этап

Этап 3, итерации I0–I2: типы, свипы, KCC, step-up и детерминированная трасса
бота. ECS-системы `Controller`/`ControllerPost` живут в
[`flux-engine::simulation`](../flux-engine/src/simulation.rs) — канон
`physics → core` не нарушается. Детерминизм-харнесс — I3.
