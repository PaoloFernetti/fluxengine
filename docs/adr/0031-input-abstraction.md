# 0031. Input abstraction: scancode-модель, ActionValue, контексты, Intention

- Статус: Accepted
- Дата: 2026-11-13
- Затрагивает: flux-platform (input), flux-engine, flux-physics (KCC), flux-ai,
  flux-net (client-owned), canon.md
- Связано: ADR-0024 (KCC — чистая функция), ADR-0027 (tick/frame/acquire),
  ADR-0007 (детерминизм)

## Контекст

Ввод — единственный канал, через который игрок и AI влияют на симуляцию.
Если raw-события (нажатия, дельты мыши) попадают в тик, ломаются
детерминизм, rewind/replay и сеть: количество событий зависит от частоты
кадров, а не тиков. Раскладка клавиатуры не должна менять управление
(WASD на AZERTY — это ZQSD), мёртвые зоны геймпада не должны терять
диапазон, а боты детерминизм-харнесса обязаны водить персонажа тем же
интерфейсом, что и игрок (ADR-0024).

Нужен контракт между платформой (сырые события), геймплеем (действия) и
симуляцией (снапшот на тик), единый для клавиатуры, мыши, геймпада и AI.

## Варианты

1. **Raw-события в ECS.** Плюс: минимум кода. Минус: число событий зависит
   от кадров; раскладка протекает в геймплей; боты идут другим путём.
2. **Готовая библиотека (gilrs + собственный mapping).** Плюс: устройства
   «из коробки». Минус: mapping/кривые/контексты всё равно свои; контракт
   Intention отсутствует.
3. **Четыре слоя + Intention-снапшот.** Raw → Mapping → Rebinding → Curves →
   `Intention`. Плюс: детерминизм, единый вход для игрока/бота/AI,
   тестируемость без окна. Минус: больше типов и дисциплина.

## Решение

Выбран вариант 3.

### Слои

| Слой | Тип/модуль | Правило |
|---|---|---|
| Raw | `input::raw::{Scancode, RawEvent, RawQueue}` | физические коды, таймстемпы; раскладка не влияет |
| Mapping | `input::mapping::{InputAction, ActionValue, Binding, PlayerId, InputMap}` | действия объявляет геймплей; контексты со стеком и приоритетом поглощения |
| Rebinding | `input::rebinding::{BindingProfile, Conflict}` | захват следующего события, конфликты, RON-профиль |
| Curves | `input::curves::{Deadzone, ResponseCurve}` | радиальный deadzone + rescale, sensitivity/инверсия per-axis |
| Snapshot | `input::intention::Intention` | единственный вход симуляции на тик |

### Контракты

```rust
pub struct Scancode(pub u32);              // физическая клавиша, не символ
pub struct InputAction { pub name: &'static str, pub id: Uuid }
pub enum ActionValue { Bool(bool), Axis1(f32), Axis2([f32; 2]), Axis3([f32; 3]) }
pub struct Binding { pub action: InputAction, pub source: BindingSource, pub player: PlayerId }

pub struct Intention {
    pub move_axis: [f64; 2],   // -1..1, уже после deadzone/rescale
    pub look: [f64; 2],
    pub jump: bool,
    pub crouch: bool,
}
```

- **Raw не входит в тик.** `InputState::snapshot()` вызывается на границе
  тика; `Intention` копируется в ECS-компонент/ресурс. Raw-очередь
  очищается после снапшота.
- **Действия не знают про «прыжок».** Платформа сопоставляет источники с
  действиями; семантика — в геймплее.
- **Контексты.** Стек `InputContext` (`Gameplay`, `UI`, `Dialogue`) с
  приоритетом: верхний поглощает действие, нижние не видят.
- **Чорды и hold/tap** — в mapping-слое, не в геймплее.
- **Боты** используют `Intention` напрямую; путь «бот → raw» запрещён.

### Детерминизм

- `Intention` — `f64` на границе тика; внутри KCC (ADR-0024) — `f64`.
- Порядок действий в `InputMap` — по UUID действия, не по `HashMap`.
- Ввод не аллоцирует в тике: снапшот — фиксированный массив/`SmallVec`
  (этап 3: `Vec` с `reserve` на кадре).

## Следствия

Плюсы: детерминизм и replay; AZERTY по scancodes; единый интерфейс
игрок/бот/AI; тесты без окна; rebinding — данные, не код.

Минусы/риски: больше слоёв и типов; нужна дисциплина «raw не в ECS»;
кривые и контексты требуют тестов.

Заделы: сетевой client-owned ввод (этап 5) реплицирует `Intention`;
моддинг получает действия как UUID-контракт; UI-контексты — во flux-ui.
