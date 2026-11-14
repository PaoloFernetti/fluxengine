# 0033. Headless-платформа и детерминизм-харнесс: feature-гейт, same-binary CI

- Статус: Accepted
- Дата: 2026-11-13
- Затрагивает: flux-platform, flux-engine, flux-physics, CI
- Связано: ADR-0004 (платформы), ADR-0007 (детерминизм), ADR-0021
  (тестирование), ADR-0027 (tick/frame/acquire)

## Контекст

Детерминизм-харнесс и серверный цикл обязаны работать без окна, GPU и
event loop. Если `flux-platform` безусловно тянет `winit`, то:
- CI вынужден поднимать окно/дисплей (или Xvfb), тесты нестабильны;
- headless-сервер и soak-боты тянут лишние зависимости;
- `check-headless-deps` (этап 1) не может гарантировать «симуляция без
  платформы».

При этом окно, ввод с устройств и DPI нужны игре и редактору.

## Варианты

1. **winit всегда.** Плюс: один путь. Минус: CI/сервер тащат окно;
   детерминизм-тесты зависят от event loop.
2. **Отдельный крейт `flux-platform-headless`.** Плюс: чистое разделение.
   Минус: новый крейт — изменение канона (ADR-0006/0028), дублирование API.
3. **Feature-гейт внутри `flux-platform`.** Плюс: один крейт, headless по
   умолчанию, окно — опция. Минус: дисциплина feature-флагов.

## Решение

Выбран вариант 3.

### Гейты

| Feature | Что включает | По умолчанию |
|---|---|---|
| `windowing` | `winit`-адаптер окна/event loop, DPI-события | выключен |
| `devices` | `gilrs`, hot-plug, хаптика | выключен |
| `crash-dump` | per-OS дампы (Windows MiniDumpWriteDump) | выключен |

- **Базовый API — без окон.** `Scancode`, `ActionValue`, `Intention`,
  `WindowMode`, `MonitorInfo` и конверсии DPI — чистые типы, доступные
  всегда.
- **`Window`** — trait/структура-заглушка в headless: события можно
  подавать вручную (тесты, harness, сервер).
- **`flux-engine`**: headless-профиль (`Engine::tick` без `frame`); окно —
  только с `windowing`.
- **CI** собирает workspace без `windowing` (headless) и отдельно проверяет
  компиляцию feature-набора `windowing` на Windows-раннере.

### Детерминизм-харнесс

```rust
pub struct DeterminismHarness { /* seeded bot + World + Schedule */ }
impl DeterminismHarness {
    pub fn run(&mut self, ticks: u32) -> StateHash;
    pub fn diff(&self, other: &StateHash) -> Option<StateDiff>;
}
```

- **Бот** генерирует `Intention` детерминированно (seeded), тем же
  интерфейсом, что игрок (ADR-0031).
- **Хеш состояния** — по каноническому `SaveJournal`-диффу: дельты по
  `(entity_uuid, type_uuid, schema_rev, field_uuid)` (ADR-0030). `HashMap`
  не участвует в порядке.
- **same-binary** — обязательный CI-гейт: два прогона в одном бинаре,
  10k тиков, хеши равны. Cross-binary/cross-platform — nightly (этап 5+).
- **Дифф** печатает первую различающуюся сущность/поле, а не только хеш.

## Следствия

Плюсы: CI без окон; сервер/харнесс без платформенных зависимостей;
`check-headless-deps` сохраняет силу; feature-наборы явные.

Минусы/риски: feature-гейты нужно держать согласованными; headless-`Window`
не должен «притворяться» живым окном (только события).

Заделы: сетевой сервер (этап 5) использует тот же headless-профиль;
cross-platform детерминизм — nightly-гейт; моддинг не получает оконный API.
