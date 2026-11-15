# flux-platform

Окно, display, input, системные сервисы.

Канон: [docs/canon.md](../../docs/canon.md). Архитектура: [docs/architecture.md](../../docs/architecture.md).

## Ответственность одной строкой

Платформа превращает сырые события ОС в [`Intention`] и не участвует в тике
симуляции: raw-события живут только здесь (ADR-0031, ADR-0033).

## Модули

| Модуль | Содержание |
|---|---|
| [`input::raw`](src/input/raw.rs) | `Scancode` (физическая клавиша), `RawEvent`, `RawQueue` с таймстемпами |
| [`input::mapping`](src/input/mapping.rs) | `InputAction`, `ActionValue`, `Binding`, `Chord`, `PlayerId`, контексты со стеком |
| [`input::rebinding`](src/input/rebinding.rs) | `BindingProfile`, захват события, конфликты, RON-roundtrip |
| [`input::curves`](src/input/curves.rs) | радиальный deadzone + rescale, response curve, sensitivity/инверсия |
| [`input::intention`](src/input/intention.rs) | конвертер `InputState` + `InputMap` → `Intention` |
| [`window`](src/window/mod.rs) | `WindowMode`, `WindowEvent`, `CloseRequest`, коалесинг resize/DPI |
| [`window::winit_adapter`](src/window/winit_adapter.rs) | `WinitInputAdapter`, `map_key_code`, `apply_cursor_mode` (`windowing`) |
| [`display`](src/display/mod.rs) | `PhysicalSize`/`LogicalSize`, `DpiInfo`, `MonitorInfo`, `VideoMode` |
| [`cursor`](src/cursor.rs) | четыре режима курсора, позиция в логических координатах |
| [`clipboard`](src/clipboard.rs) | trait `Clipboard` + `MemoryClipboard` для headless/тестов |
| [`text`](src/text.rs) | IME-композиция и committed-поток без потерь |
| [`paths`](src/paths.rs) | каталоги проекта: ассеты, сейвы, логи, дампы |
| [`power`](src/power.rs) | батарея и тепловое давление → подсказка троттлинга |
| [`devices`](src/devices.rs) | реестр геймпадов, hot-plug; `gilrs_backend::poll` (`devices`) |
| [`crash`](src/crash.rs) | breadcrumbs, паник-хук, текстовый дамп; native per-OS — позже |

## Инварианты

1. **Raw не входит в тик.** `Intention` — единственный вход симуляции;
   `InputState` и `RawQueue` очищаются на границе кадра.
2. **Раскладка не влияет на управление.** `Scancode` — USB HID usage:
   WASD на AZERTY — те же физические клавиши.
3. **Детерминизм.** Порядок разрешения действий — по UUID, контексты —
   по приоритету; результат — `BTreeMap`, не `HashMap`.
4. **Deadzone не теряет диапазон.** Радиальный deadzone перенормирует
   вход в `0..1` (rescale), а не обрезает.
5. **Headless по умолчанию.** `winit`/`gilrs` — только за feature-гейтами;
   базовые типы доступны без окна и GPU.

## Feature-гейты

| Feature | Что включает | По умолчанию |
|---|---|---|
| `windowing` | `winit`-адаптер окна/event loop | выключен |
| `devices` | `gilrs`, hot-plug, хаптика | выключен |
| `crash-dump` | per-OS дампы (Windows MiniDumpWriteDump) | выключен |

## Этап

Этап 3, итерация I4: `winit`-адаптер (`windowing`) и `gilrs` hot-plug
(`devices`) за feature-гейтами; clipboard/text — headless, нативные дампы
per-OS отложены до ADR. Детерминизм-харнесс и интеграция `flux-engine` — I3.
