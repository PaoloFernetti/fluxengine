# FluxEngine — архитектура

- Владелец: архитектор
- Статус: Living
- Канон имён и глоссарий: [canon.md](canon.md) (приоритет при расхождении)
- Статус и порядок работ: [Roadmap.md](Roadmap.md); принятые решения — [adr/README.md](adr/README.md)

## Ключевые решения

| Решение | Выбор |
|---|---|
| Графический API | Собственный RHI с двумя бэкендами: **Vulkan (ash)** + **DX12** |
| ECS | **Собственная архетипная ECS** в `flux-ecs` (полный контроль над планированием и памятью) |
| Физика | **Собственная физика на f64** в `flux-physics`; Rapier не используется |
| Модель разработки | Crate Workspace, модульные подсистемы, Data-Oriented Design |

## Структура workspace

```text
fluxengine/
├── Cargo.toml              # [workspace], общие версии зависимостей
├── crates/
│   ├── flux-core/          # Ядро: time, jobs, memory, events, logging,
│   │                       # profiling, config, platform, contracts
│   ├── flux-math/          # Векторы, кватернионы, AABB, frustum, преобразования
│   ├── flux-ecs/           # Архетипная ECS: сущности, компоненты, системы, запросы
│   ├── flux-graphics/      # rhi (trait) + rhi::vulkan + rhi::dx12 + render
│   ├── flux-platform/      # Окно, display, input abstraction, системные сервисы
│   ├── flux-asset/         # UUID-хэндлы, реестр, асинхронная загрузка, hot-reload
│   ├── flux-scene/         # Сцены, world partition, prefab-ы, save/load
│   ├── flux-physics/       # Собственная физика на f64
│   ├── flux-anim/          # Анимационная система
│   ├── flux-ai/            # AI: navmesh, pathfinding, BT/HFSM, crowd
│   ├── flux-script/        # Скриптинг: Lua 5.4 через mlua
│   ├── flux-net/           # Сетевой слой: репликация, relevance, транспорт
│   ├── flux-audio/         # Звук
│   ├── flux-ui/            # In-game UI
│   ├── flux-editor/        # Редактор сцены
│   ├── flux-engine/        # Фасад: сборка подсистем, публичный API для игр
│   └── flux-macros/        # Служебные дерайвы: Reflect/save/replicated (ADR-0028)
├── examples/               # Примеры игр
└── tools/
    ├── flux-assetc/        # Компилятор ассетов: KTX2, меши, Opus, .fpf
    ├── flux-build/         # Оркестратор сборки: DAG, кэш, пакинг
    └── flux-release/       # Релизный цикл: версии, CHANGELOG, теги (ADR-0040)
```

## Граф зависимостей крейтов

- Игра пользователя → flux-engine
- flux-editor → flux-engine
- flux-engine → flux-core
- flux-engine → flux-ecs
- flux-engine → flux-physics
- flux-engine → flux-graphics
- flux-engine → flux-platform
- flux-engine → flux-asset
- flux-engine → flux-scene
- flux-graphics → flux-asset
- flux-graphics → flux-math
- flux-graphics → flux-core
- flux-physics → flux-core
- flux-audio → flux-core
- flux-scene → flux-asset
- flux-scene → flux-math
- flux-ecs → flux-core
- flux-ecs → flux-macros
- flux-core → flux-math
- flux-macros → ничего из движка (дерайвы потребляются, обратной связи нет)

## Ядро

_math, core, ecs_

### Математика и системы координат

**Крейт:** `flux-math` · **ID:** `math`

Двухслойная схема: свой публичный API поверх glam как backend, три пространства координат для больших миров, физика на f64 вне sim-rebase.

#### Двухслойная схема

- Слой типов — свой публичный API: Vec3/Mat4/Quat/DVec3/DQuat, жёсткий контракт: no allocation, детерминизм операций, выравнивание для SIMD.
- Слой реализации — glam как backend (default-features = false, libm для no_std), типы экспортируются через re-export в модуле types.rs — бэкенд меняется в одном месте.
- Vec3 (alignment 4, для полей структур) vs Vec3A (alignment 16, для SoA-массивов и SIMD-загрузок) — в контракте разделены явно.
- Свой SIMD не вводится, пока профилировщик не покажет, что glam мешает; бутылочное горлышко — пропуски памяти и батчинг, а не векторная математика.

#### Три пространства координат (большие миры)

- Канон — WorldPos(i64, нанометры): единственный источник истины для позиции; применяется в сейвах, сети и манифестах сцены; конверсии — только на границах.
- Sim-пространство — f32 floating origin: симуляция (ECS Transform, навигация, стриминг, LOD-дистанции, audio emitters, AI); rebase детерминирован и выполняется на границах тиков.
- Физика — f64 вне sim-region rebase: собственный решатель, конверсии на входе/выходе тика (осознанное исключение, ADR-0003/0008).
- Рендер — camera-relative f32: экстрактор раз в кадр считает rel_pos = (obj_pos − cam_pos) в f64 → cast в f32; дальше весь пайплайн — f32, Reversed-Z обязателен.
- GlobalTransform в ECS — относительно sim-origin (f32); иерархии аккумулируются в sim-пространстве, канон применяется только на границе.
- Camera-relative применяются ко ВСЕМ: тени, cubemap/IBL-пробы, скайбокс — иначе мерцание теней на дистанции.
- Terrain/статика: вершины в .fpf локальны относительно опорной точки меша/чанка; pivot чанка — в метаданных .fpf (согласовано с квантованием к origin чанка).
- Детерминизм: физика считает на скалярном libm-пути (glam SIMD не bit-exact между x86/NEON), рендер может использовать SIMD.

> **Физика и f64**
>
> Собственная физика на f64 (flux-physics), Rapier не используется. Физика считает на скалярном libm-пути — детерминизм. Обоснование: полноценный уровень, полный контроль, соответствие f64-миру и философии движка «всё своё».

### Детальный дизайн ядра

**Крейт:** `flux-core` · **ID:** `core`

Ядро не знает о подсистемах. Ничего глобального — контексты передаются явно, детерминизм опирается только на fixed_delta.

| Модуль | Содержимое |
|---|---|
| time/ | Clock, FixedTimestep с аккумулятором, TimeContext (пауза, time scale), DeltaTime, Stopwatch/Cooldown/RepeatingTimer, timestamp в наносек |
| jobs/ | JobHandle, DAG зависимостей (handle.then), work-stealing (Chase-Lev, crossbeam-deque), scoped jobs с borrow-проверками, MainThreadTaskQueue, приоритеты Critical/High/Background |
| memory/ | Allocator trait, BumpAllocator (арена), PoolAllocator, FrameAllocator (очистка раз в кадр), трекинг по тегам, OOM-политика, бюджет памяти, SlotMap / GenerationalIndex-контейнеры |
| events/ | EventRegistry, типизированные очереди, Reader/Writer, отложенные события на кадр, once-события |
| logging/ | tracing + tracing-subscriber, каналы (Render/Physics/Asset), фильтрация в рантайме, Telemetry-lite (FPS, frametime), assert/verify-макросы (в релизе логировать, не падать) |
| profiling/ | zone!() Tracy-style зоны, счётчики, гистограммы, frame маркеры, экспорт Chrome Trace / Tracy; GPU-зоны прокидываются через RHI |
| config/ | типизированные секции ConfigSection<RenderSettings>, дефолты + валидация, hot-reload с событием ConfigChanged (CLI-парсинг НЕ здесь — в flux-engine) |
| platform/ | thread priority, affinity к ядрам (аудио/рендер), DynamicLibrary (hot-reload для editor), memory-mapped файлы для стриминга; обёртки над atomics НЕ делаем — std::sync::atomic напрямую |
| crash/ | КОНТРАКТ CrashHandler (platform-независимо): register, metadata, breadcrumbs; формат дампа как кроссплатформенные данные (стеки потоков, модули, регистры). NO WinAPI внутри core; per-OS реализации — в flux-platform/crash |
| contracts/ | trait-контракты подсистем: Renderer, Platform, AudioDevice — подсистемы видят друг друга без взаимных импортов |

#### Правила

1. Зависит только от flux-math и стандартных крейтов (crossbeam, tracing).
2. Никаких знаний о подсистемах: NO ecs_bridge / render_bridge / math_compat — ECS и остальные подсистемы используют core, а не наоборот.
3. Ничего глобального — контексты передаются явно.
4. Детерминизм — симуляция опирается только на fixed_delta.

### Собственная архетипная ECS

**Крейт:** `flux-ecs` · **ID:** `ecs`

Архетипное хранилище: комбинация компонентов = таблица плотных столбцов. Планировщик вычисляет конфликты по read/write заявкам систем.

```text
flux-ecs/src/
├── entity/        # Entity = index + generation — GenerationalIndex из flux-core/memory
├── archetypes/    # архетипные таблицы SoA, swap-remove, миграции при add/remove
├── component/     # trait Component, registry, drop-политика
├── query/         # Query с With/Without/Added/Changed, кэш архетипов по сигнатуре
├── scheduler/     # DAG систем по read/write доступу → job system
├── resources/     # singleton-данные вне сущностей
├── events/        # ECS-события: spawn/despawn, add/remove компонентов
├── change/        # change detection: Tick компонента из flux-core/time
└── world/         # World, Commands (deferred), spawn_batch
```

#### Ключевые решения

- Архетипное хранилище: комбинация компонентов = таблица плотных столбцов — линейная итерация с идеальной локальностью кэша.
- Entity generational index — валидация устаревших ссылок.
- Отложенные команды (Commands) — применение в конце фазы, без инвалидации запросов.
- Планировщик: системы декларируют reads/writes; DAG конфликтов мапится на job-и flux-core (параллелизм без ручных локов); явный before/after для порядка без конфликтов; детерминированный порядок внутри stage.
- Change detection: Tick у компонента, Changed/Added фильтры — экстрактор рендера читает только изменённое (синергия с RenderScene).
- Иерархия Parent/Child — компоненты + отдельная таблица отношений, обход — система.
- Сериализация через ComponentRegistry + schema-описания (для редактора и сохранений), не через serde на типе — редактор правит любые зарегистрированные компоненты.
- Один World на игру + отдельные миры для превью редактора и тестов.

#### Schema-система (единая для всех потребителей)

- Один SchemaRegistry в flux-ecs: derive-macro #[derive(Reflect)] генерирует схему при регистрации компонента; атрибуты полей через #[reflect(min, max, slider, category, tooltip, editor_visible)]. Дерайв живёт в служебном пакете flux-macros (ADR-0028), формат схемы и дельты — ADR-0030.
- Схема: имя типа, UUID, поля (имя, тип, диапазон, tooltip, категория, дефолт, флаг показа в инспекторе).
- Потребители одной схемы: сериализация сцен (flux-scene), сейвы, инспектор редактора (float→слайдер по min/max, enum→dropdown, UUID→asset-picker), типы гизмо.
- Решение «схемы как данные» (не трейты) — сериализация и инспекция без дублирования.

## Рантайм

_platform, engine, цикл кадра_

### Окно, display, input abstraction

**Крейт:** `flux-platform` · **ID:** `platform`

Платформа владеет окном и отдаёт render нативный handle + resize-события; swapchain и презентация — в rhi.

```text
flux-platform/src/
├── window/          # winit-обёртка: режимы, события, жизненный цикл
├── display/         # мониторы, видеорежимы, DPI, HDR/гамма (информация)
├── input/
│   ├── raw/         # сырые события: клавиатура, мышь, геймпад, тач
│   ├── mapping/     # action mapping: действия, профили, ребиндинг
│   ├── curves/      # deadzone, response curves, sensitivity
│   └── devices/     # gilrs, хаптика, hot-plug, глифы кнопок
├── cursor/          # режимы курсора, pointer lock, виртуальный курсор
├── clipboard/       # буфер обмена
├── text/            # текстовый ввод, IME
├── paths/           # user dirs, сохранения
├── power/           # батарея, троттлинг (ноутбуки)
└── crash/           # per-OS реализации CrashHandler
```

> **Политика крэшей**
>
> dev падает громко (RUST_BACKTRACE=full + трейсинг), prod пишет дамп и выходит чисто. Отправка дампов на сервер — отдельная подсистема телеметрии, platform только сохраняет.

#### Оконно-дисплейный слой

- WindowBuilder: заголовок, иконки (несколько разрешений), Windowed / BorderlessFullscreen / ExclusiveFullscreen — выбор остаётся пользователю (borderless быстрее переключается, exclusive — lower latency и честный VRR).
- События: resize, focus, close-request с возможностью отклонить, drag&drop. Resize — событие для rebuild render graph таргетов.
- Мониторы/видеорежимы/refresh rate — для меню настроек графики.
- Граница: platform владеет окном и отдаёт render нативный handle (raw-window-handle) + resize-события.
- DPI: три уровня (OS scale factor, physical vs logical pixels, внутренний UI scale). Рендер — physical, UI — logical. Per-Monitor DPI change доходит до UI и render graph; конверсия logical/physical в обе стороны для курсора; min/max размеры окна в logical.

#### Input abstraction layer — 4 слоя

1. Raw events: scancodes (не раскладки — WASD на AZERTY = ZQSD это фича), raw mouse delta в обход OS-ускорения (Raw Input API), собственная очередь с таймстемпами для input buffering и детерминизма.
2. Action mapping: ActionValue (Bool/Axis1/Axis2/Axis3), InputAction с именем/UUID, bindings, consumed. Действия объявляет геймплей, платформа не знает про «прыжок». Один источник — много потребителей. Чорды, hold/tap, dead triggers — в mapping-слое. PlayerId в биндингах — local multiplayer. Контексты UI/dialogue/gameplay со стеком и приоритетами поглощения.
3. Rebinding: рантайм-захват следующего raw event, проверка коллизий, персистентность через flux-core/config профили, пресеты (клавиатура/геймпад/левша), hot-reload TOML в dev.
4. Кривые: радиальные deadzones с rescale (не покоординатные), response curves linear/expo/custom, sensitivity per-axis с инверсией, anti-deadzone для триггеров — всё per-action.

#### Геймпады, курсор, текст

- gilrs: XInput/DirectInput Windows, udev Linux (udev-rules в пакете), DualSense HID.
- Rumble/haptics: play_effect(player, left, right, duration); adaptive triggers DualSense — отдельно. Hot-plug события подключения/отключения.
- Внутренний enum кнопок (South/East/North/West/DPad/Trigger...) — глифы в UI по типу геймпада (Xbox/PS/Switch).
- Режимы курсора: Normal / Hidden / Captured / Relative (pointer lock). При отпускании курсор возвращается на прежнюю позицию. Виртуальный курсор геймпада для UI.
- Текстовый ввод: отдельный Unicode-поток событий (не сборка строк из KeyDown), IME (CJK): позиция окна кандидатов, события композиции — критично для редактора.

#### Интеграция

- Input → ECS: сырые события копятся в буфер; action-система конвертирует их в ресурсы/события ECS на начале кадра; геймплей читает только actions, не raw.
- Input buffering на fixed step: снапшот ввода на каждый шаг симуляции; дельты мыши (Look) сэмплятся от последнего снапшота с таймстемпами — иначе камера дёргается.
- Граница с flux-core: события через event bus из core; platform не тянет ECS/рендер.

> **Чего здесь НЕТ**
>
> Swapchain/презентация (rhi), аудио-устройства (flux-audio), remap-UI (flux-ui), файловая система (std::path + flux-core/config).

### Фасад и точка сборки

**Крейт:** `flux-engine` · **ID:** `engine`

Композиция подсистем, главный цикл кадра и публичный API игр. Подсистемы импортируют только trait-контракты из flux-core/contracts; все конкретные связки — здесь.

```text
flux-engine/src/
├── lib.rs          # публичный API: EngineBuilder, Engine, re-export контрактов
├── bootstrap/      # CLI-парсинг (clap), конфиги → EngineConfig, порядок инициализации
├── runtime/        # главный цикл кадра, фазы, back-pressure, shutdown
├── plugins/        # plugin-система: подсистемы и геймплей регистрируются одинаково
├── context/        # EngineContext — ручка доступа ко всем подсистемам для плагинов
└── settings/       # user-settings (качество графики, громкость) → config-секции
```

> **Порядок инициализации**
>
> Config (CLI + конфиги) → logging/profiling → job system → platform → assets → ECS мир → render (RHI-бэкенд по config) → physics/audio/ui → Engine готов. Shutdown — в обратном порядке, через EngineEvent::PreShutdown из flux-core/events.

#### Цикл кадра: движок экспонирует шаги

Цикл владеет ВЫЗЫВАЮЩИЙ, а не движок.

```rust
impl Engine {
    pub fn tick(&mut self) -> TickResult;    // фикс. шаг симуляции (0..N за цикл)
    pub fn frame(&mut self) -> FrameStats;   // extract → upload → record → submit → present
    pub fn acquire(&mut self);               // ожидание GPU — отдельно: редактор
                                             // в edit-режиме может пропускать
}
impl EngineBuilder {
    /// Тонкая обёртка для игр: pump events → tick() пока накопитель просит
    /// → frame() → acquire(). Никакой собственной логики.
    pub fn run(self) -> !;
}
```

#### Контрактные правила

1. Движок НИКОГДА не касается окна и event loop: pump событий — обязанность вызывающего (run() делает это по умолчанию для игр; редактор — сам: ввод вьюпорта и геймплейный ввод не борются за event loop).
2. Накопитель FixedTimestep живёт внутри Engine, не в run(): количество тиков — функция sim-времени (SimTick), не wall-clock вызывающего. PIE получает тики только через tick() — PIE и финальная игра исполняют одну симуляцию.
3. Гранулярность по стадиям: edit-режим — только frame(); PIE — полный цикл на PIE-инстансе; headless-сервер — только tick(): тот же контракт закрывает серверный цикл, детерминизм-харнесс и soak-боты без окон и без run().
4. Per-frame ресурсы сбрасываются в начале соответствующего шага (FrameArena — в frame(), scratch сим-арена — в tick()), не в run() — инвариант кольца кадров держится при нескольких вьюпортах/PIE-инстансах.
5. Re-entrancy запрещена: step-методы не реентерабельны, job system дренируется на границах шагов (ловля паник сидит ровно здесь).
6. Пауза — отсутствие вызова tick(); никаких paused-флагов внутри движка (дублируют цикл, ломаются в PIE-клонах).
7. Bootstrap не дублируется: инициализация подсистем, паник-хук, настройки — в EngineBuilder::build(); цикл — выбор вызывающего. PIE-мир — отдельный Engine-инстанс/isolate.

#### Внутренний порядок (run() / frame())

- clock.begin_frame → EngineEvent::FrameStart
- while накопитель просит: tick() → plugins.update_fixed (ECS-системы, физика)
- extractor.extract → RenderScene; plugins.update_variable (UI, стриминг)
- renderer.render_frame → present (main-thread)
- jobs.wait_idle (fence кадра) → telemetry.end_frame

- Back-pressure: анти-spiral-of-death ограничитель N фиксированных шагов за кадр; render-on-demand — present можно пропускать при перегрузке.
- Hot-reload dev-режима: ConfigChanged, шейдеры, DynamicLibrary-reload геймплей-плагинов.
- Пауза, time scale, слоу-мо — через TimeContext из flux-core/time.

#### Plugin-система и EngineContext

```rust
pub trait Plugin {
    fn build(&self, ctx: &mut EngineContext);
    fn name(&self) -> &str;
}

EngineBuilder::new()
    .with_backend(GfxBackend::Vulkan)   // или DX12 — из config
    .with_plugin(RenderPlugin)          // подсистема
    .with_plugin(PhysicsPlugin)
    .with_plugin(GamePlugin)            // геймплей пользователя
    .run()                              // bootstrap + цикл + shutdown
```

EngineContext — структурированный доступ плагинов к подсистемам через trait-объекты контрактов: world (flux-ecs), events (flux-core/events), assets (flux-asset), time, jobs. У плагина НЕТ доступа к конкретным типам бэкендов — только к контрактам.

> **Чего НЕ делает flux-engine**
>
> Не содержит геймплейной логики — всё в плагинах и ECS-системах. Не знает деталей RHI — только контракт Renderer. Не редактор: flux-editor — отдельное приложение поверх flux-engine (как игра).

### Цикл кадра

**ID:** `frame-loop`

Фиксированный timestep для симуляции, переменный — для рендера, интерполяция трансформов.

`Ввод` → `Фикс. шаг симуляции` → `Physics update` → `Gameplay update` → `Рендер с интерполяцией` → `Present`

Порядок кадра: [Sim step 60 Гц] → [Extract: снапшот + alpha] → [Upload: submit_batch] → [Record: jobs, late-latch view] → [Submit] → [Present по режиму].

## Графика

_RHI, Render Graph, PBR_

### Компоновка: один крейт с модулями и feature-гейтами

**Крейт:** `flux-graphics` · **ID:** `graphics`

rhi и render всё равно компилируются вместе (жёсткая зависимость) — экономия boilerplate-крейтов; изоляция бэкендов сохраняется через cfg-feature.

```text
flux-graphics/src/
├── lib.rs           # выбор бэкенда по feature: create_device(backend) -> impl Device
├── rhi/             # trait-абстракция (module rhi):
│   ├── device.rs    # Device, queues, capabilities
│   ├── swapchain.rs # Swapchain: present, resize
│   ├── cmd.rs       # CmdBuffer: draws, binds, barriers
│   ├── resources.rs # GpuBuffer / GpuImage — хэндлы в VRAM-менеджер
│   ├── pipeline.rs  # PipelineStateObject: graphics, compute, mesh
│   ├── barrier.rs   # BarrierBatch — строится render graph, RHI транслирует
│   ├── vram.rs      # VramAllocator: бюджеты, сегментация, дефрагментация
│   ├── vulkan/      # бэкенд Vulkan (ash)   — #[cfg(feature = "vulkan")]
│   └── dx12/        # бэкенд DirectX 12     — #[cfg(feature = "dx12")]
├── shaders/         # пермутации шейдеров: compiler, registry, scheduler,
│                    # fallback, declarations
└── render/          # высокоуровневый рендер, зависит ТОЛЬКО от rhi:
    ├── scene/       # RenderScene: SoA-снимок ECS-данных кадра + alpha интерполяции
    ├── graph/       # Render Graph
    ├── pipeline/    # PBR-материалы, PSO-кэш, mesh-модель
    ├── culling/     # frustum + occlusion culling над SoA-массивами (frame allocator)
    ├── lighting/    # свет, CSM-тени, IBL
    ├── post/        # SSAO, bloom, tonemapping (ACES), TAA
    └── debug/       # debug-draw слой: линии, текст в мире, коллизии, тайлы
                     # стриминга; Quality::Editor профиль render graph
```

Cargo.toml: `default = []` — бэкенды реализуют один trait-набор модуля rhi и включаются opt-in (`vulkan`, `dx12`); выбор бэкенда — конфигурация в flux-engine (и в `create_device`). `Null` доступен всегда, поэтому headless-сборка и CI не требуют Vulkan SDK (ADR-0033, ADR-0034).

> **Почему один крейт вместо четырёх**
>
> rhi и render всё равно компилируются вместе (жёсткая зависимость), экономия boilerplate-крейтов; изоляция бэкендов сохраняется через cfg-feature и внутренние модули, а не через границы крейтов.

### GraphicsSettings: пресеты и настройки

**Крейт:** `flux-graphics` · **ID:** `graphics-settings`

Трёхуровневая модель: HardwareTier → QualityPreset → полная гранулярность. Пресет — генератор значений.

- **HardwareTier** — Автоопределение GPU (VRAM, vendor, фичи RHI) → дефолтный пресет.
- **QualityPreset** — Low/Medium/High/Ultra — именованные наборы для меню одним переключателем.
- **GraphicsSettings** — Полная гранулярность под пресетом; пресет = генератор значений.

#### Параметры, которые экспонирует движок

| Группа | Параметры |
|---|---|
| Разрешение | resolution_scale, dynamic resolution, отдельный масштаб UI |
| Тени | вкл/выкл, число CSM-каскадов 1–4, разрешение shadow map 512–4096, дистанция |
| SSAO | вкл/выкл, радиус, интенсивность, half-res |
| Сглаживание (AAMode) | Off \| FXAA \| MSAA 2x/4x/8x \| TAA \| DLAA; у MSAA — только на forward/transparent проходах, с deferred совмещается через resolve; у TAA — jitter strength |
| Апскейлинг (Upscaler) | Native \| DLSS (NVIDIA, Quality/Balanced/Performance/Ultra Performance) \| FSR (AMD). DLSS интегрируется на уровне RHI через NVNGX SDK (feature-гейт); resolution_scale согласован с режимом апскейлера |
| Вывод | VSync, framerate cap, fullscreen, HDR (по поддержке свопчейна) |
| Bloom/пост | bloom, tonemapper ACES/Reinhard, exposure, film grain |
| Reflections | SSR off/low/high (опциональная фича) |
| Текстуры | mipmap LOD bias, анизотропия 1–16, texture_quality (коэффициент бюджета, НЕ абсолютные гигабайты), vram_cap_override («не больше X МБ») |
| Геометрия | LOD distance multiplier, per-frame бюджет видимых мешей |

#### Механика применения

- Настройки НЕ ветвят код if-ами: Render Graph декларативный — каждая фича регистрирует проходы с профилем качества (enabled, half-res targets, число каскадов).
- Изменение → rebuild render graph: пере-аллокация только затронутых transient-ресурсов (aliasing), без пересоздания Device и фриза. Дифф SettingsChanged за один кадр.
- Персистентность: ConfigSection<GraphicsSettings> в flux-core/config + user-settings в flux-engine/settings.
- LOD/стриминг: lod multiplier и VRAM-бюджеты проваливаются в flux-asset и flux-scene через ту же шину SettingsChanged.
- Dynamic resolution: автопонижение resolution_scale по frame time из flux-core telemetry.

#### Контракт для игры

```rust
// игра генерирует меню настроек автоматически
engine.graphics.features() -> GraphicsFeatureRegistry
engine.graphics.set_preset(QualityPreset::High)
engine.graphics.set(ShadowCascades(3))
```

### Render Graph и поток данных ECS → рендер

**Крейт:** `flux-graphics` · **ID:** `render-graph`

Декларативное описание кадра: автоматически выводит барьеры, aliasing временных ресурсов, планирование на async compute и включение проходов по пресетам.

- явные barrier-и из read/write отношений pass-ов (RHI только транслирует в API),
- aliasing временных ресурсов (экономия VRAM),
- планирование на async compute (SSAO/тени параллельно G-buffer),
- лёгкое включение/выключение проходов по quality-пресетам из config.

`culling` → `CSM shadows ×4` → `G-buffer (deferred PBR)` → `deferred lighting` → `SSAO` → `forward+ transparent` → `TAA` → `bloom + tonemap` → `UI` → `present`

#### Поток данных ECS → рендер

- Рендер никогда не ходит в ECS напрямую; extractor раз в кадр собирает RenderScene.
- SoA-массивы без allocations через frame allocator из flux-core.
- GPU-driven rendering (draw-indirect от GPU) — расширение поверх CPU-side visible list.

> **Шейдеры**
>
> HLSL + DXC: DXC компилирует в DXIL (DX12) и SPIR-V (Vulkan) — единый язык для обоих бэкендов. Компиляция офлайн через tools/flux-assetc; в dev-режиме — hot-reload шейдеров.
>
> Вход RHI — готовый SPIR-V (ADR-0035): `PipelineDesc` фиксирует entry points, vertex layout v1 (позиция `f32x3`, stride 12), форматы аттачментов и топологию. DX12 получает DXIL кросс-компиляцией SPIR-V при сборке шейдеров, а не вторым контрактом RHI.

### Контракт Renderer и владение

**Крейт:** `flux-graphics` · **ID:** `renderer-ownership`

Три решения о владении: Device живёт дольше рендереров, Swapchain — часть рендера, rebuild графа — явная операция.

1. RhiDevice НЕ принадлежит рендереру: flux-engine создаёт Device на старте (Device::create(&WindowHandle, &RenderConfig) — конструктор в RHI, вызов из фасада), Device живёт дольше всех рендереров; Renderer получает &mut Device как borrow. Renderer — per-viewport объект: main, editor viewport, picture-in-picture, превью материалов — каждый со своим render graph, но над общим Device.
2. Swapchain — часть рендера, привязана к окну; для editor viewport его нет (RTT-текстура). Выход абстрагирован: PresentTarget = swapchain ИЛИ texture handle.
3. Rebuild графа — ЯВНАЯ операция, не внутри render_frame: при resize, смене quality preset, тоггле фичи, смене таргета, вкл/выкл пассов. Пайплайны компилируются асинхронно через job system; политика «старый PSO работает до конца кадра, пока готов новый»; освобождение через ref-count в pipeline cache.

```rust
pub trait RenderBackend {
    fn configure(&mut self, device: &mut RhiDevice,
                 target: &RenderTargetDesc) -> Result<()>;
    fn rebuild_graph(&mut self, device: &mut RhiDevice,
                     features: &FeatureSet, ctx: &FrameCtx) -> Result<()>;
    fn render_frame(&mut self, device: &mut RhiDevice,
                    scene: &RenderScene, ctx: &FrameCtx) -> Result<FrameStats>;
}

pub struct FrameCtx<'a> {
    pub jobs: &'a JobScheduler,
    pub frame_alloc: &'a FrameArena,
    pub resources: &'a ResourceRegistry,
    pub present: PresentTarget,
    pub stats: &'a mut FrameStats,
}
```

Логика кадра внутри render_frame: prepare (upload/eviction) → cull+sort → record (параллельно через jobs) → submit (барьеры графа → очередь → present).

Pass-фабрики: граф собирается из регистрируемых пассов — register_pass_factory(PassId, factory); UI-пасс (flux-ui), egui-пасс (editor bridge), debug-draw регистрируются снаружи; render_frame включает UI-пассы. render_frame НЕ принимает ECS World никогда — только RenderScene-снапшот.

> **Resize-путь**
>
> Событие окна → platform → engine → configure() → swapchain пересоздан → rebuild_graph(). Resize-события КОАЛЕСЦИРУЮТ: engine применяет одно за кадр в точке синхронизации (после wait_idle) — против шторма пересозданий при drag окна.

| Сущность | Владелец | Обоснование |
|---|---|---|
| RhiDevice | создаёт flux-engine, живёт независимо | несколько рендереров |
| Swapchain | render, per main-window | пересоздаётся на resize |
| Render graph | Renderer, per-viewport | main и editor — разные графы |
| Pipeline cache | на Device, общий | перекомпиляция дорогая, шарить |
| Resource registry | на Device | лизинг ресурсов между рендерерами |
| TransferSystem | на Device, общая | upload: staging-кольцо, батчи, очереди |
| Transient RT | allocator графа, на кадр | frame arena из flux-core |
| RenderScene | снапшот на кадр, создаёт engine | изоляция borrow ECS |

FrameStats: фреймтайм по пассам наружу в flux-core/profiling — трейсинг-маркеры внутри render_frame, часть контракта.

### Путь ассета → GPU: Transfer-система

**Крейт:** `flux-graphics/rhi` · **ID:** `transfer`

Три контракта: (а) кто решает грузить — flux-scene/стриминг; (б) байты в GPU — flux-graphics/rhi; (в) жизнь без готового ассета — прокси.

- Решение о загрузке — flux-scene/стриминг (приоритет тайла → запрос ассета).
- IO + распаковка — flux-asset: чанк .fpf → LZ4-декод → payload В RAM, не VRAM. flux-asset НИКОГДА не вызывает графические API — отдаёт «payload готов» (байты + метаданные: формат, мипы, размеры) в очередь.
- Конвертация в RHI-ресурс — rhi TransferSystem (одна на Device, общая для всех рендереров).
- Handle ↔ GPU-ресурс — ResourceRegistry: единственное место разрешения Handle<Texture> → GpuImage.

`запрос Handle` → `IO payload в RAM` → `Registry.reserve(pending)` → `TransferSystem.upload` → `submit_batch раз в кадр` → `барьеры (ShaderReadOnlyOptimal)` → `Ready` → `bindless-слот`

eviction — только между кадрами и после fence-ов. Upload-контракт RHI: reserve_image(desc) -> GpuImage (память выделена, содержимого нет); upload(image, payload) — payload во владение transfer-а, аллокация из frame arena; submit_batch(device, ctx) — фиксированная точка кадра в scheduler: сразу после extract, ДО параллельной записи графических команд.

#### Staging

- Кольцевой пул фиксированного бюджета (64–128 МБ, конфигурируемо).
- fence-ретирмент: кадр N переиспользует память только после fence кадра N−K (in-flight окно 2–3 кадра).
- Ограничение батча по бюджету — хвост на следующий кадр = естественный backpressure стримера. Бесконечные очереди upload запрещены.
- Отдельная transfer-очередь: dedicated transfer queue (Vulkan QUEUE_TRANSFER) или copy command list (DX12) с timeline semaphore — копии 4K-текстур не съедают frametime графического пасса.
- Mipmap-генерация (dev-ассеты без прекомпилированных цепочек) — compute-шейдером на async compute очереди (vkCmdBlitImage НЕ работает на чистой transfer-очереди).

#### Два уровня разрешения Handle

- Уровень 1 CPU: ResourceRegistry slot → состояние Empty / Pending / Ready { GpuImage, descriptor_slot } / Evicting; ref-count: asset-кэш держит strong-ref, сцены/материалы — weak.
- Уровень 2 GPU: биндлесс-массив (VK_EXT_descriptor_indexing / bindless heap DX12); биндлесс-индекс выдаётся при Reserve, не при Ready — материал ссылается на индекс сразу, шейдер читает прокси до догрузки; обновление дескриптора — точечное. При Eviction слот рециклируется (LRU поверх descriptor heap), прокси остаётся в слоте.

- **Прокси на классы** — белый 1x1, чёрный, нейтральная нормаль, розово-чёрный шейкер (ассета нет вообще), серый (грузится). Код рендера не знает о состоянии загрузки — деградация автоматическая.
- **Eviction** — только между кадрами (после submit, перед extract следующего) — снапшот содержит слоты, выгрузка под живым снапшотом = use-after-free на GPU.
- **GPU-рефкаунт** — per-resource last_used_frame, выгрузка допустима при current_frame − last_used_frame > frames_in_flight.
- **LRU** — решение с гистерезисом — у flux-asset; один общий LRU-кэш на текстуры и меши; бюджет VRAM общий, учёт в ResourceRegistry.

Меши: та же схема, барьеры не нужны (копия в TransferDst), eviction валидирует index range-ы. Dev-режим: hot-reload → re-upload по тому же контракту, политика «не выгружать никогда» (реестр игнорирует LRU). Контракты upload и eviction разделены.

### VramBudget: единый владелец VRAM

**Крейт:** `flux-graphics/rhi` · **ID:** `vram-budget`

Двойное владение бюджетом (GraphicsSettings vs VramAllocator) устранено: бюджет — объявление рендера, настройки — один из входов.

- VramBudget живёт на Device рядом с VramAllocator; единственный источник истины. Стример — клиент: читает BudgetSnapshot и соблюдает.
- Почему НЕ стример: VRAM потребляют swapchain + render targets (зависят от разрешения и пресета — это зона рендера); реальный объём ≠ размер карты (OS-резерв, shared memory, резидентность DX12); только рендер видит траекторию потребления.
- Контракт: VramBudget::recalculate(device, settings) — входы: query_device_memory (VK_KHR_memory_budget / DXGI), os_driver_reserve (эвристика 200–400 МБ + доля), targets_reserve(preset, resolution), geometry_reserve(preset). texture_budget = остаток × quality_coefficient.
- ResourceRegistry::allocate_vram(bytes, MemoryTag) -> VramLease — все длительные захваты через одну точку учёта; теги Textures/Geometry/BLAS/Audio/Other. Транзиентные таргеты — не через lease: reserved-строка в recalculate.
- BudgetSnapshot { total, reserved, used, pressure } обновляется раз в кадр.

#### Pressure-лестница с гистерезисом

Порог выхода ниже порога входа — против текстурного пинг-понга.

| Уровень | Порог | Поведение |
|---|---|---|
| Ok | < 80% | Штатно |
| Warning | 80–90% | Тормозится спекулятивная загрузка, HLOD-приоритеты выше |
| Critical | 90–97% | Агрессивный eviction вне radii, LOD дальше |
| Overflow | > 97% | Сброс до HLOD, запрет новых upload, приоритет только «видимой камере» |

- Критический pressure = деградация по спроектированной лестнице, не вылет.
- Роли: eviction решает стример ПО ДАННЫМ рендера (last_used_frame — единственный источник «живости»); рендер не выгружает сам — объявляет давление. Заявки upload проверяются против pressure ДО старта: при Critical TransferSystem отклоняет текстуры ниже HLOD-приоритета.
- Dev-режим: политика never-evict (hot-reload) ПРЕВЫШЕ pressure-лестницу по eviction, но давление логируется в канал Render; в release — приоритет у лестницы.
- Наблюдаемость: переходы pressure-уровней — события event bus + маркеры profiling + строка в debug-draw overlay.

> **Модель потоков**
>
> Старт: main-thread submission (симуляция параллелится через job system). Архитектура готова к выделенному render thread: RenderScene — снимок через каналы, симуляция и рендер с лагом 1 кадр. Переход — по данным профилировщика.

### Материальная модель PBR (ADR-0017)

**Крейт:** `flux-graphics/render/pipeline` · **ID:** `pbr`

База BRDF — UE/Filament-style, «как у всех»: эталонные числа открыты, художники и инструменты знают её.

- Lambert-диффуз + GGX/Trowbridge-Reitz спекуляр; Smith height-correlated visibility (коррелированный G, не раздельные G-функции — заметно на grazing angles).
- multiscatter energy compensation — ОБЯЗАТЕЛЬНО (без него металлы темнеют при high roughness; исправлять после = ломать golden-image базу).
- параметризация metallic/roughness (glTF 2.0-совместимая — бесплатная совместимость с asset-пайплайном), AO как отдельный модулятор.
- Fresnel: F0 из metallic (диэлектрики 0,04); вариативные IOR — за пределами базы.

Расширения — keyword-пермутации, активируются материалом декларативно; база вычисляется всегда, расширения добавляют члены поверх, обязаны падать в fallback.

| Расширение | G-buffer-затраты | Включать |
|---|---|---|
| Clearcoat (лак: авто, прод) | 2 параметра | да |
| Sheen (ткань/бархат) | 1–2 параметра | да, дёшево |
| Subsurface cheap (wrap/half-Lambert) | 1 параметр | да |
| Anisotropy (причёсанные металлы, волосы) | 1–2 параметра | да, дёшево |
| Full SSS (RNM/профили) | отдельный пасс или 2-й G-buffer | нет, по потребности |
| Iridescence | редкий кейс | нет, по потребности |

Экономика пермутаций: расширение входит в матрицу пермутаций только если используется ≥ N материалами эталонной сцены. База+clearcoat+sheen+SSS-cheap+aniso ≈ 32–64 комбинации — приемлемо; всё вместе — взрыв.

#### G-buffer layout — ABI рендера

| Таргет | Формат | Содержимое |
|---|---|---|
| RT0 | RGBA8 sRGB | albedo.rgb + [A: материал-флаг/маска] |
| RT1 | RGBA16F | oct-encoded normal 16 бит (8-битный oct даёт бандинг на гладких поверхностях с зеркалом/SSR — 16 бит база, 8-битный с дизерингом — fallback-профиль) |
| RT2 | RGBA8 linear | ORM — occlusion, roughness, metallic + [A: индекс расширенного дескриптора — bindless гетерогенный материал-буфер: тяжёлые параметры расширений живут в материальном буфере, читаются только пикселями с флагом] |
| Depth | D32 reversed-Z | — |

- Deferred lighting расширяется флагами: база всегда; clearcoat-лоб — второй GGX-лоб в light-пассе, ветвящийся по флагу. SSR/транслусент — forward-пасс поверх (hybrid). Full SSS при потребности — feature-set слот на High+, или переработка layout.
- Тон-маппинг: фиксируем семейство (ACES-fitted / AgX — выбор в GraphicsSettings, но семейство ABI художников; смена = перетонирование всех референсов).
- Связи: расширения = статические keyword-флаги, PBR-параметры — данные: «структура шейдера — keyword, значения — данные». Golden-сцены обязаны покрывать каждое включённое расширение.

## Контент

_scene, ассеты, .fpf_

### Сцены, world partition, prefab-ы, save/load

**Крейт:** `flux-scene` · **ID:** `scene`

Оркестратор стриминга и структуры мира, не хранилище данных: данные — в ECS, ассеты — в flux-asset.

```text
flux-scene/src/
├── scene/           # сцена: корневой документ, манифест
├── partition/       # world partition: гриды, регионы, LOD-паки
├── streaming/       # streaming volume, радиусы, приоритеты, eviction
├── prefab/          # префабы: вложенность, инстансинг, наследование
├── entity/          # базовые компоненты сцены: Transform, Hierarchy, Names
├── serialize/       # формат сцены: блоб-сериализация, диффы
├── save/            # game save/load: поверх сериализации
└── lib.rs
```

#### Формат сцены

- Сцена = иерархия: манифест + ссылки на ассеты, НЕ монолитный файл.
- Scene manifest: UUID сцены, дерево top-level сущностей/префабов, ссылки на регионы partition, глобальные настройки, спавн-точки. Текстовый формат RON (git-мерж, код-ревью, художники; Rust-native, serde/schema-совместимость с flux-ecs). TOML — для конфигов.
- Данные сущностей: текст с блоб-зонами (читаемые поля + бинарные массивы).
- Ссылки на ассеты ТОЛЬКО по UUID; пути — свойство реестра flux-asset.
- Мерж-дружелюбность: стабильный порядок полей, entity UUID генерируется один раз при создании и живёт до удаления (пере-UUID-вание = вечные диффы).
- Entity UUID и asset UUID — разные пространства имён.
- Манифест — SOURCE-формат: flux-assetc компилирует его в чанк .fpf; в рантайме текстовый парсер не живёт.

#### World partition и стриминг

- Грид-тайлы 256–1024 м (конфигурируемо), тайл = пак: подмножество сущностей + ссылки на ассеты. Сущность принадлежит тайлу по опорной точке (не AABB).
- Грид: квадратный + HLOD (запечённые импосторы/агрегаты для дальних тайлов); PartitionStrategy как trait — иерархический квадtree добавляется без переписывания стримера.
- Streaming radii зонами: FullyLoaded (всё: анимации, физика, аудио) → Actors (сущности загружены, сложные компоненты спят) → HLOD; per-регион и per-тип контента.
- Спекулятивная загрузка с упреждением по скорости движения (стример получает скорость от физики/скриптов). Бюджеты: байт/сек, VRAM, лимит таск-загрузок. Приоритет: видимая камере > близкая > спекулятивная > HLOD.
- Eviction: LRU по радиусам с гистерезисом (граница выгрузки дальше границы загрузки — против пинг-понга).
- Состояния сущности: Unloaded → MetadataOnly → Geometry → FullActive — рендер/физика/стриминг знают, что читать.

#### Prefab-ы

- Префаб = ассет (UUID, живёт в flux-asset); инстанс = override-слой: сущность хранит только изменённые поля, остальное из префаба.
- PrefabRef { uuid, overrides }; вложенные префабы с наследованием; обновление базового обновляет все инстансы кроме overridden полей.
- Детекция циклов вложенности (A→B→A) — ошибка компиляции ассета, не рантайм.
- API редактора: Make Prefab / Revert to Prefab / Apply to Prefab.
- Спавн префабов — батчинг через spawn_batch flux-ecs.

> **Двухслойная модель мира**
>
> Статический слой (тайлы partition) — владение редактора: сущности в манифесте сцены, runtime туда НИКОГДА не пишет. Динамический слой — владение сейва: runtime-спавны (в т.ч. в выгруженных тайлах) идут в дифф SaveState; загрузка тайла = статические данные + дифф сейва. Снимает вопрос владения сущностью: тайл = уровень сцены, динамика = уровень сейва.

#### Базовые компоненты сцены

- Transform: позиция в sim-пространстве (f32 относительно origin) + кватернион + scale; канонический WorldPos(i64) — на границах (сейв/сеть/манифест); иерархия parent→child (согласовано с flux-math).
- Name (имя + UUID), Hierarchy (parent/children через ECS-сущности), Enabled/Visible/Culled флаги, теги принадлежности региону.

#### Сериализация vs Save/Load — два разных механизма

- Serialization (сцена на диске): полный формат уровня для редактора, читаемый, мерж-дружелюбный.
- Save/Load: ТОЛЬКО динамическое состояние — (entity_uuid, component_data) диффы поверх UUID-стабильности; загрузка = сцена с диска + дифф сейва.
- Маркер сохраняемости: proc-macro #[save] или явный реестр — нетронутые геймплеем компоненты в сейв не пишутся.
- Политика для сейв-записей, которых нет в сцене после патча: спавн заново ИЛИ удаление записи — per-компонентная настройка.

> **SaveState × eviction**
>
> Выгрузка тайла не теряет состояние. Eviction корректен тогда и только тогда, когда динамический слой тайла сериализован в SaveState до возврата памяти в пул. Ассетный слой константный (восстанавливается из .fpf); всё изменённое живёт только в SaveState. SaveState — источник истины, стриминг — механика доставки.

1. **FLUSH** — Динамические дельты тайла сериализуются в SaveState асинхронно (job с низким приоритетом, дедлайн до начала eviction): компоненты с #[save] + скриптовые состояния.
2. **FENCE** — Никто не держит ссылок: рендер не рендерит (убран из RenderScene), физика не тикает, сеть не реплицирует, pending-джобы тайла дренированы.
3. **EVICT** — Страницы в пул, диффы остаются в SaveState.

- Flush — часть контракта dirty→disk, не отдельная операция: дельты тайла дописываются в event-journal персистентности ДО eviction, снапшоты — производные. Крэш после eviction, но до следующего сейва, не теряет состояние.
- Повторная загрузка тайла: диффы применяются поверх свежеразпарсенного ассетного слоя; orphaned-записи — по принятой политике (логирование, не крах).
- Сущность на границе тайла в момент eviction — единая политика с cross-zone из flux-net: transfer с подтверждением по SimTick (сеть) или сериализация в SaveState тайла назначения (оффлайн).
- Бюджетный монитор стриминга получает колонку «flush pending»: тайлы с густыми дельтами — сигнал перегрузки записи, не тихая потеря. Типичный тайл: сотни КБ дельт, миллисекунды сериализации.
- Один формат диффов (entity_uuid, component_data), один журнал, одна политика миграции — обслуживают стриминг, сеть и сейвы.

#### Миграция схем сейвов

Три источника сериализованных данных стареют одинаково: SaveState (сейвы игроков), event-journal персистентности (старые записи НЕ переписываются: миграция обязана читать ЛЮБУЮ историческую ревизию), пики ассетов (решается content-hash + пересборка).

1. Версия схемы на каждой записи: SchemaRef { type_uuid, schema_rev } — на диске и на проводе. schema_rev — версия схемы КОНКРЕТНОГО типа, не версия игры.
2. UUID полей — идентификатор совместимости: переименование поля НЕ требует миграции; смена типа — новая ревизия.
3. Миграции — чистые детерминированные функции в SchemaRegistry, ЦЕПОЧКАМИ (v1→v2→v3); отсутствующий шаг = ошибка загрузки с внятным сообщением, не молчаливый дефолт.
4. READ-TIME миграция по умолчанию («никогда не разрушать источник»): сейвы и журналы не перезаписываются при чтении; откат патча работает.
5. Несовместимое → ЯВНОЕ отбрасывание с логом («dropped N records of type X rev 2»), молчаливые потери запрещены. Для журнала: удалённая механика посередине replay = «пустая» миграция.
6. Тестирование: roundtrip-свойство migrate(serialize(deserialize(x))) == migrate(x); golden-сейвы каждого релиза — CI-гейт; фаззинг миграций.

#### Границы крейтов

flux-scene — ОРКЕСТРАТОР стриминга и структуры мира, не хранилище данных. Спавнит через ECS API; просит ассеты по UUID; не знает физику/рендер; события тайлов через event bus flux-core.

#### Environment / time-of-day / weather

Нерушимое правило: компоненты описывают ФАКТ — «дождь, ветер 5 м/с»; потребители решают, как рендерить/озвучивать. Рендер не спрашивает туман у аудио, аудио — у рендера: оба читают один компонент.

| Слой | Владелец | Содержимое |
|---|---|---|
| Данные (компоненты) | flux-scene | Environment (fog, wind, поверхностная насыщенность), SunSky (направление, интенсивность, цвет), WeatherVolume (региональные переопределения) |
| Вычисления | flux-graphics/lighting | Environment → параметры рендера (объёмный туман, skybox, освещение), GI-условия |
| Time-of-day | симуляция | SimTime — детерминированный компонент мира: реплицируемый, сейвимый, квантованный (16 бит/сутки) |
| Weather state machine | симуляция | data-driven ресурс с seeded-RNG, в SaveState и репликации |

Почему SimTime — симуляция, не рендер: в рендере он не реплицируется (рассинхрон «закат/рассвет» между игроками), не детерминирован (реплеи не воспроизводят кадр), не сейвится (wall-clock после загрузки). SimTime-компонент закрывает всё: потребители — рендер (солнце), AI (ночная активность NPC), аудио (ambience), golden-тесты.

- flux-graphics: объёмный туман, небо, GI; golden-сцены фиксируют SimTime + weather state — тривиальная воспроизводимость.
- flux-audio: ambience по weather state, send-уровни реверба по Environment — дождь → мокрый реверб.
- flux-anim/IK: ground alignment читает состояние поверхности — лужа/снег из Environment через компонент тайла.
- flux-ai: модификаторы LOS — туман/ночь → perception-бюджет.
- flux-net: дельты weather state, квантование SimTime.
- flux-scene: WeatherVolume стримится с тайлом, flush в SaveState по контракту eviction.

### Пайплайн ассетов и формат .fpf

**Крейт:** `flux-asset + tools/flux-assetc` · **ID:** `assets`

flux-assetc ПИШЕТ формат и знает все кодеки; flux-asset ТОЛЬКО читает. В рантайме нет энкодеров и импортёров исходников.

- `tools/flux-assetc` (компилятор) — ПИШЕТ формат, знает все кодеки: KTX2-энкодер, импортёры FBX/GLTF, Opus-энкодер, упаковщик .fpf.
- `flux-asset` (runtime) — ТОЛЬКО читает: ContainerReader .fpf, mmap через flux-core/platform, декомпрессия LZ4/Zstd, реестр, UUID-хэндлы.
- flux-asset не знает конкретные типы ассетов — их объявляют подсистемы через trait AssetType.
- Import settings: per-asset конфиг импорта живёт рядом с исходником в репо (<source>.import — metadata-файл, правится в git); flux-assetc читает его при компиляции; артефакты — в .fpf.

#### Компиляция ассетов

- Текстуры → KTX2: mip-цепочка запечена заранее; канальное блочное сжатие BC1 (albedo без альфы) / BC3 (с альфой) / BC5 (нормали) / BC7 (качество) / BC4 (маски).
- Vertex buffer с перс-атрибутным кодированием: Position f32 на LOD0 с квантованием к origin чанка, Normal SNORM8/16, Tangent SNORM8 + знак, UV f16, Color UNORM8x4, веса UNORM8 + индексы костей UINT8; vertex layout id в метаданных.
- Index buffer: u16 до 65k вершин, иначе u32. LOD-уровни LOD0–LOD3 запечены заранее — в рантайме переключение индексных буферов.
- Bounding volumes: AABB, OBB, Sphere — предвычислены для culling.
- Скиннинг: иерархия костей, rest-позы, inverse bind matrices; палитры матриц — рантайм-конструкция рендера, в файле отсутствуют.
- Animation clips: квантованные кейфреймы (позиции f16, кватернионы SNORM16), линейная/кватернионная интерполяция, дедупликация кривых.
- Звук → Opus. UUID ассетов выводятся из content-hash: дедупликация + детерминированные сборки.

#### Контейнер .fpf

| Секция | Содержимое |
|---|---|
| Header | магическое число, версия формата, смещение таблицы индексов |
| Index table | UUID, тип ассета, смещение, размер, xxhash64, флаги (сжатие: None \| LZ4 \| Zstd; vertex layout id; версия компилятора ассета) |
| Data blocks | бинарные чанки; выравнивание смещений 16/256 байт (mmap + DMA); header-чанк и bulk-данные РАЗДЕЛЕНЫ |
| TOC / hash table | поиск по имени, проверка целостности |

#### Пространственная привязка паков

- Паки .fpf привязаны к участкам карты: один .fpf = регион/world-chunk (плюс глобальные паки: движковые ассеты, UI, общие материалы).
- Стример при движении открывает только пак текущего региона и соседей — поиск UUID идёт в маленькой index table одного пака.
- flux-scene задаёт границы чанков и streaming-радиусы; flux-asset держит реестр паков с их AABB-регионами.

#### Runtime-поведение

- UUID-хэндлы вместо путей, реестр с ref-counting.
- Асинхронная загрузка через job system, стриминг с приоритетами и eviction.
- Multi-pack: игра = набор .fpf (базовый + DLC + патчи). РЕЗОЛВИНГ ПЕРЕСЕЧЕНИЙ (основа моддинга): упорядоченный список источников VFS, ПОЗДНИЙ источник перекрывает РАННИЙ — параметр загрузчика.
- Горячая перезагрузка в dev-сборках (watcher исходников → перекомпиляция → reload).

## Геймплей

_anim, ai, physics, audio, ui, script, net_

### Анимационная система

**Крейт:** `flux-anim` · **ID:** `anim`

flux-asset хранит только статику: скелеты, меши, клипы. flux-anim — runtime-логика: state machine, blending, sampling, ретаргетинг, IK, синхронизация с физикой.

```text
flux-anim/src/
├── components/     # AnimPlayer (state, clip_offset, speed, blend_weights[4],
│                   # root_motion_accum, events_pending), AnimGraph, AnimEvents
├── systems/        # StateMachine → Sampling → Retargeting → Blending → IK
├── physics/        # RagdollSync (freeze/blend по силе удара), RootMotion
├── network/        # Replication: дельты состояния графа, квантование тайминга
└── render/         # Skinning: bone-буферы для bindless GPU-skinning
```

- Сеть: реплицируется СОСТОЯНИЕ ГРАФА (state, blend-веса, тайминг клипа, флаги событий) — единицы байт на сущность, не кадры.
- Рендер: pose → bone-матрицы для GPU-skinning (bindless-палитры).
- AnimGraph — data-driven ресурс в .fpf (states, transitions с условиями/exit-time/interruptibility, blend trees 1D/2D, параметры из скриптов/ввода). Рантайм — быстрый поиск по ID + кэш переходов.
- Анимационные события (footstep/attack/hit/sound): генерируются в Sampling по ключевым кадрам ДЕТЕРМИНИРОВАННО (по clip_offset), в конце тика сбрасываются в flux-script или аудио; в сеть — только флаги «событие в тике T». Порядок доставки внутри тика — детерминированный sort по UUID сущности.
- Root motion: накапливается в тик-пространстве, применяется к Transform в конце тика; к физическим телам — через канон/f64-интерфейс flux-physics.
- Ragdoll: blend между ragdoll и анимацией по силе удара/скорости; при восстановлении — плавный возврат контроля.
- Ретаргетинг: заранее подготовленные маппинги в .fpf per-скелет (human/quadruped/mech), scale/mirror — коэффициенты в ресурсе; runtime — O(N) костей линейно.
- IK (минимальный набор): foot placement (высота земли из flux-scene тайлов), ground alignment (по нормали поверхности), chain-IK (руки, ограничение углов). После блендинга, до root motion; IK-weight смешивает с анимационной позой.
- Репликация: current_state, clip_offset (16 бит на 4 сек), blend_weights (8/16 бит), events-флаги по тикам. Клиент локально интерполирует; сервер валидирует переходы и тайминги.
- GPU-skinning: bone palette (до 256 костей/меш), bone buffer — 3x4 матрицы (не 4x4 — 25% экономии; dual quaternion — опция). SkinningSystem в extract собирает матрицы из pose в bindless-буферы.
- Dirty-tracking: AnimPlayer dirty при смене state/весов; диффы → сеть и SaveState.
- Анимационный LOD: сущности в ярусах Delta/Coarse получают пониженную частоту sampling (каждые 2–4 тика) или скип — связь с релевантностью, не отдельная система.
- Детерминизм: все операции в фиксированных тиках, без зависимости от framerate.

### AI и навигация

**Крейт:** `flux-ai` · **ID:** `ai`

Генерация navmesh — в asset-пайплайне: офлайн, тяжёлая (voxelization, marching), запечена в .fpf посекционно на тайл. Детерминизм бейка обязателен.

#### Navmesh × world partition

- Тайлы те же 512 м; неиграющий тайл не грузит свой navmesh (бесплатный стриминг, инкрементальность).
- Генерация с overlap-полосой (8 м по периметру): тайл бейкается с полосой соседей, результат обрезается — швы сшиваются детерминированно.
- Устойчивые ID полигонов: UUID полигона (отдельное пространство имён); neighbor-ссылки между тайлами переживают ре-бейк соседа.
- Динамические запреты (разрушаемость): runtime НЕ редактирует navmesh — tile-локальный слой запрещённых областей; слой сериализуется в SaveState по контракту eviction.
- Высотный слой отдельно: foot placement в flux-anim читает те же данные — один источник, два потребителя.

#### Pathfinding: иерархия, не плоский A*

1. Coarse-граф: тайлы как узлы, связи через пограничные порталы — метаданные РЕЗИДЕНТНЫ всегда (~458 узлов).
2. Маршрут по coarse-графу.
3. A* по полигонам внутри тайла (navmesh по релевантности). JPS НЕ применим к полигонам (только grids) — A* + портальный полиш (string pulling).
4. Стриминг пути: агенту нужен путь до портала; достройка по мере загрузки тайлов. NPC вне релевантности путь не вычисляет (AI-LOD).

- Детерминизм: тай-брейк A* по UUID полигона (не по порядку указателей/HashMap).
- Все навигационные решения — в канонических координатах, не во float.

#### Поведение: BT/HFSM

- BehaviorRunner { tree: TreeUuid, node_state (continuation для восстановления после выгрузки), blackboard (#[save]), budget: TicksBudget }.
- Data-driven ресурсы в .fpf (та же философия, что AnimGraph); HFSM — тот же раннер, другой формат дерева.
- Тикирование по AI-LOD: NPC в ярусе 1 — 5 Гц, ярусе 2 — 1 Гц; AI-LOD — потребитель RelevanceSystem.
- Perception — бюджетируемая подсистема: LOS через flux-physics, проверки — явный лимит на тик (самое дорогое в AI).
- BT-узлы — декларативные примитивы (move-to, wait, play-anim, emit-event); специфичная логика — вызовы flux-script.

#### Crowd (ярусная, симметрично релевантности)

| Ярус | Механика | Стоимость |
|---|---|---|
| Полный | RVO-подобное локальное избегание | высокая, десятки агентов рядом |
| Средний | separation force без планирования | низкая |
| Coarse | нет avoidance — flow fields на POI | копейки |

Flow fields к статичным POI — запекаются в .fpf; динамичные POI — runtime-вычисление на coarse-графе. Полный pathfinding — десяткам лидеров, толпа — по полю.

#### Сеть и детерминизм

- AI — серверная подсистема: клиент не тикает BT чужих NPC, получает дельты (позиция, стейт); интерполяция достаточна.
- AI-решения — функция (state, seeded-RNG); perception без jitter — иначе реплеи и детерминизм-гейт падают.
- SaveState: положение в BT, blackboard с #[save], динамические запреты navmesh — по схеме eviction-контракта.

```text
flux-ai/src/
├── nav/            # mesh.rs (запросы), tiles.rs (world partition), dynamic.rs
│                   # (запреты → SaveState), coarse.rs (порталы, резидентные)
├── path/           # astar.rs (детерминированный тай-брейк), planner.rs
│                   # (coarse→tile→polish), streaming.rs (достройка пути)
├── behavior/       # bt.rs (continuation, LOD-тикирование), hfsm.rs,
│                   # perception.rs (LOS через physics, бюджет)
├── crowd/          # avoidance.rs (RVO), flow.rs (flow fields)
└── lod.rs          # AI-LOD: потребитель RelevanceSystem
```

### Собственная физика на f64

**Крейт:** `flux-physics` · **ID:** `physics`

Rapier НЕ используется. f64 относительно канона, вне sim-region rebase; симуляция детерминированная (fixed tick, seeded-RNG).

> **Два слоя**
>
> Collision-слой (свипы/касты капсул и лучей по запечённой геометрии тайлов, без solver) обслуживает KCC и PIE. Полное физическое ядро (broadphase/narrowphase, solver, CCD, joints) — отдельный слой поверх того же интерфейса.

#### Kinematic Character Controller

Позиционирование в тике (явный порядок в schedule): Input (симуляционный снапшот Intention, не raw-эвенты) → ControllerSystem (стейт-машина движения) → Physics (симуляция с учётом запросов контроллера) → ControllerPost (ground-state, анимационные флаги, root-motion reconciliation).

Контракт Intention: контроллер получает от flux-platform абстрактную Intention (move-vector, jump, crouch, look) — одну для клавиатуры, геймпада и AI-ботов (боты детерминизм-харнесса водят персонажа тем же интерфейсом — требование, не совпадение).

| Блок | Контракт |
|---|---|
| Коллизия | капсула (единственная базовая форма), запросы через physics-канал «character» |
| Ground detection | grounded/airborne по лучам/свипам С ГИСТЕРЕЗИСОМ; высота+нормаль — читает flux-anim для foot placement |
| Steps & slopes | шаг до H (конфиг), наклон до θ, сверх — скольжение (параметр) |
| Root motion × controller | ЕДИНОВЛАДЕНИЕ: анимация двигает — контроллер нет, и наоборот; переход флагом из flux-anim; конфликт = ошибка конфигурации |
| Ragdoll-переход | kinematic capsule → dynamic ragdoll; обратный — getup-анимация с blend, позиция капсулы восстанавливается детерминированно |
| Подвижные платформы | движение «относительно платформы» через parent-frame в тике |

#### Сеть: персонаж client-owned

- контроллер — ЧИСТАЯ функция (state, intention, dt) БЕЗ скрытого состояния: всё в компонентах (сериализуемых и реплицируемых) — rewind-and-replay механически возможен;
- симуляция на фиксированном тике, не dt-кадра: клиент реплеит те же тики, что сервер; фрейм-зависимый контроллер ломает сверку flux-net.

#### Детерминизм и границы

- Свипы/касты детерминированы при детерминированном состоянии; СВОЁ ядро детерминировано на одной платформе, cross-platform — общий детерминизм-гейт.
- Один контроллер на сущность — гарантия borrow-модели планировщика.
- Скрипты меняют параметры (скорость, прыжок) через компоненты, не прямым доступом (иначе ломается репликация и сейв).
- Координаты: sim-region f32, канон — на входе/выходе тика.
- За пределами контроллера: лестницы через special-mesh, транспортная иерархия — геймплейный слой; контроллер даёт примитивы (grounded, attach-point).

### Звук

**Крейт:** `flux-audio` · **ID:** `audio`

cpal — платформенный слой (аналог winit для окон), symphonia + opus — декодеры. Микшер-граф свой. Свой Vorbis-декодер не пишется никогда.

```text
flux-audio/src/
├── device/       # вывод: cpal как платформенный бэкенд (измерительный слой)
├── mixer/        # свой микшер: голоса, граф, шины, приоритеты
├── spatial/      # 3D-позиционирование: pan, дистанция, HRTF, реверб-зоны
├── dsp/          # фильтры, delay, convolution reverb, compressor
├── codec/        # декодеры: symphonia (Vorbis/FLAC/PCM), opus
├── streaming/    # потоковая загрузка длинных звуков из .fpf чанками
├── events/       # sound events: параметры, рандомизация, ducking
└── lib.rs
```

- Своё: микшер-граф (голоса с приоритетами и виртуализацией — голос за пределами слышимости не декодируется), шины music/sfx/voice с независимой громкостью и ducking, spatialization, sound events с рандомизацией (pitch/громкость/варианты), стриминг из .fpf.
- Потоковая модель: аудио-поток реального времени отделён от основного — lock-free ring buffer + команды из game thread.
- Voice stealing: конфигурируемая политика per-шину (самый тихий/дальний/старейший).
- Sample-accurate scheduling: команды с таймстемпом в сэмплах аудио-часов (свой монотонный счётчик сэмплов, синхронизация с FrameTime раз в кадр) — рассинхрон музыки и геймплея исключён архитектурно.
- Master limiter (soft-clip) на шине master — защита от клиппинга при пиках голосов.
- Реверб-зоны — ECS-компоненты в flux-scene; flux-audio только исполняет параметры.
- Аудио-эмиссеры берут позицию из f64-мира; рендер и слушатель — camera-relative.

#### Сигнальный граф — ABI контента

- Два уровня: декларативный (что/куда играет — шины, события) и сигнальный (как звук течёт по DSP-цепочке).
- Консольная модель: голоса → шины → мастер; Routing = Direct(BusId) | Send { bus, level } | Return(BusId); send/return — единственный механизм общих эффектов (реверб помещения — на return-шине, send-уровни по зонам — preset per-tile).
- Шины статичные, задаются контентом (music, sfx, dialog, ambience — четыре базовые); голоса динамичные. Ацикличность графа проверяется при загрузке ассетов.
- Sidechain — control-rate путь (раз на аудио-блок, не на сэмпл): dialog-шина подаёт envelope на компрессоры music/sfx; переиспользуется как дакинг «важное/неважное».
- Голосовой пул фиксированный (256 voices); приоритет вытеснения = дистанция × важность × возраст; диалоги — приоритет выше любых SFX.
- Виртуализация: голос вне ярусов релевантности (2/3) не смешивается — доигрывается виртуально (таймер), «вступает» при возврате. Аудио-LOD — ПЯТЫЙ потребитель RelevanceSystem.
- Голоса — НЕ ECS-сущности: внутренний пул + VoiceHandle.
- Контракт AudioNode::process (RT-поток): без аллокаций, без локов, параметры через control-rate; паника → silent-output.
- Граница детерминизма: генерация событий детерминирована (footstep в тике T, seeded-RNG); МИКС — не обязан. Правило записано явно, чтобы не детерминировали компрессоры.

### Интерфейс: два слоя, один текстовый движок

**Крейт:** `flux-ui` · **ID:** `ui`

In-game UI — свой immediate-mode; редактор — egui. Общий актив — текстовый рендер: SDF-атласы, shaping, кэш глифов пишутся один раз.

- In-game UI — свой immediate-mode: HUD неразрывно связан с render graph (свой pass, свои блендинги, шрифты через общий текстовый пайплайн), с input actions (фокус-навигация геймпадом — это действия), с DPI-логикой flux-platform.
- Editor — egui: docking, tree views, инспекторы писать своё — самоубийственно; редактор не грузится в игру.
- Общий актив — текстовый рендер: egui-бэкенд редактора использует те же SDF-атласы (кастомные font textures). Renderer egui — через наш render graph (отдельный pass).

```text
flux-ui/src/
├── immediate/    # ядро: widget tree за кадр, id, layout (flex/grid)
├── draw/         # батчинг в render pass: rect, text, image, rounded
├── text/         # шейпинг (rustybuzz), SDF-атласы, кэш глифов
├── input/        # фокус-навигация из flux-platform actions
├── widgets/      # кнопка, слайдер, dropdown, tooltip...
├── localization/ # строки по UUID-ключам, IME-стык с platform
└── egui_bridge/  # интеграция для редактора
```

- Retain-гибридность: НЕ чистый egui-style «пересчитываем всё каждый кадр» — layout-дерево пересчитывается только по dirty-флагам на виджетах, draw-список батчится; пересборка батчей только для изменённых. Чистый immediate-mode для сложного HUD в 4K не тянет.
- Анимации (tween-ы на виджетах) — в ядре; дообучить потом почти невозможно.
- Локализация: строки по ключам-UUID как ассеты в .fpf (снимает локализационные паки DLC); литералы строк в коде запрещены.
- RTL-языки (арабский) влияют на layout с самого начала (rustybuzz обрабатывает bidi).
- UI-координаты — logical pixels (DPI-модель из flux-platform).

### Скриптинг: Lua 5.4

**Крейт:** `flux-script` · **ID:** `script`

Итерация без пересборки, дизайнеры ≠ Rust-программисты, сериализация состояния в сейв, песочница (ошибка скрипта не роняет кадр), детерминизм.

| Вопрос | Решение |
|---|---|
| Язык геймплейных скриптов | Lua 5.4 через mlua (LuaJIT отдельно не обещаем) |
| Rust hot-reload | НЕ скриптинг; отдельная тема для системного кода |
| Visual scripting | отдельный frontend к тому же VM |
| Доступ к миру | события и команды, НЕ прямое чтение ECS |
| Состояние | явные поля с маркером сейва; UUID-стабильность |
| Стриминг | скрипт = компонент ScriptRef { uuid, state }, живёт в тайле, усыплён при выгрузке |
| Детерминизм | только seeded-RNG + SimTime из flux-core; системное время и global random запрещены |
| Ошибки | catch в script-VM: dev — лог + debug overlay, скрипт продолжает; release — дизейбл + telemetry; кадр живёт |
| WASM | не для геймплейного скриптинга; кандидат на пользовательские моды |

```text
flux-script/src/
├── core/        # ScriptVM trait: load, update, handle_event, serialize_state
├── lua/         # бэкенд: mlua, API-биндинги
├── api/         # движковый API для скриптов: события, запросы, команды
├── registry/    # реестр скриптов: UUID → ассет, hot-reload, приоритеты
└── graph/       # visual scripting frontend к тому же VM
```

#### Песочница-политики

- Instruction limit: hook-based счётчик инструкций на скрипт за шаг симуляции; превышение → прерывание с ошибкой (защита от вечного цикла, вешающего 60 Гц).
- GC-бюджет: инкрементальный GC с ограниченным шагом в fixed step (порция < 1 мс); полная сборка — в паузах/между тайлами.
- Скрипты — ассеты: UUID в .fpf, hot-reload в dev по перечитыванию файла.
- API-слой: узкий набор операций (spawn entity, read transform, play sound, set state), не весь мир — перформанс, песочница, сейвы.

Инварианты: событийная шина ECS допускает доставку события в скрипт (on_collision, on_trigger_enter, on_custom); скриптовое состояние сериализуется по SchemaRegistry — та же схема полей, что для инспектора и сейвов.

### Сетевой слой

**Крейт:** `flux-net` · **ID:** `net`

Репликация между headless-серверами и клиентами; потребляет dirty-tracking flux-ecs, дифф-сериализацию flux-scene и SimTick flux-core — не дублирует их.

> **Запрещено тянуть**
>
> flux-graphics, flux-audio; flux-script — через событийную шину. Headless-сборка — отдельная цель сборки.

```text
flux-net/src/
├── relevance/     # RelevanceSystem: ярусы 0-3, капы, гистерезис, adaptive-сжатие
├── replication/   # delta (по dirty-tracking), quantize (позиция i32 fixed-point,
│                  # ориентация 8/16 бит, стейт-маски; НЕ f32/f64 на проводе),
│                  # priority (per-клиентский байтовый бюджет), schema (#[replicated])
├── ownership/     # Server | Client(id) | Orphan; request_control, миграция,
│                  # handoff между region-нодами по SimTick
├── transport/     # trait NetBackend (Loopback | UDP); свой слой поверх UDP:
│                  # каналы ReliableOrdered / UnreliableLatest / Events, ACK,
│                  # фрагментация; clock sync, RTT, input-delay-регион
├── session/       # client/server стороны
├── topology/      # region-нода (~6 км², ~20 нод/мир), hub маршрутизация,
│                  # LoadDegrader — зональная деградация тика
├── persistence/   # ИНТЕРФЕЙСЫ: снапшоты зон + event sourcing journal
└── loopback.rs    # вся репликация в одном процессе
```

RelevanceSystem — один расчёт, ПЯТЬ потребителей (сеть: частоты репликации 15-20 / 2-5 / 0.5-1 / по событиям Гц; flux-scene: приоритет стриминга тайлов; симуляция: simulation LOD частота физического тика; flux-anim: анимационный LOD; flux-audio: виртуализация голосов).

| Ярус | Кап | Содержимое |
|---|---|---|
| Full | 64 | Полный стейт + предсказание |
| Delta | 250 | Полный стейт без предсказания |
| Coarse | 300 | Маркер без ECS-сущности |
| Aggregate | ячейка карты | События |

- Гистерезис на границах (вход дальше выхода), adaptive-сжатие ТОЛЬКО в кластерах плотности (POI), вне POI Delta держит 500 м.
- Цикл зависимостей разорван контрактом: trait RelevanceProvider в flux-core/contracts, реализация в flux-net; flux-scene потребляет через фасад.
- Репликация: дельты по dirty-tracking ECS (не полные снапшоты); пер-клиентский байтовый бюджет; ярус 0 не режется никогда, режутся 1→2→3 по PriorityRanker.
- Квантизация обязательна (детерминизм). NetworkingSettings — по паттерну GraphicsSettings: цели и капы в конфиге, лимиты — производные.
- Детерминизм — проверяемый инвариант: CI-тест «две headless-ноды с одним вводом → один стейт».
- Интеграции: скрипты как компоненты — состояние попадает в репликацию автоматически; spatial events с радиусом доставки; coarse-маркеры игроков — рендер обязан уметь рисовать без ECS-сущности.

## Инструменты

_editor, flux-build, flux-release_

### Build-оркестрация

**Крейт:** `tools/flux-build` · **ID:** `build`

Оркестратор шире assetc: шейдеры (DXC), navmesh-бейк, asset-компиляция, компоновка паков, манифесты, билд кода. PIE и релиз идут через ОДИН оркестратор — ноль расхождений пайплайнов.

#### Ядро: DAG на content-hash

- Узел = (входные файлы + хеши, ВЕРСИЯ инструмента, конфиг ступени) — ключ кэша из всех трёх; обновление DXC пересобирает все шейдеры — правильно.
- Зависимости — декларативно из заголовков ассетов (сцена перечисляет ассеты, материал — шейдеры); глоб-паттерны ЗАПРЕЩЕНЫ (недетерминизм, мёртвые связи).
- Параллелизм из графа (job'ы); циклы = ошибка сборки; инвариант «сцена → ассеты» (ассеты не ссылаются на сцены) проверяется оркестратором.
- Кэш content-addressed: локальный + распределённый (S3-класс) — общий с CI: локально собранное не пересобирается на агенте.

#### Релизный билд

- Проекция на профиль (dev / editor / shipping) от корневых сцен через DAG; недостижимые ассеты не пакуются (заодно — отчёты «чистильщика»).
- Пакинг: блобы в .fpf (квота ≤ 256 МБ/пак), манифест UUID-центричный, сортировка по частоте совместного доступа из runtime-трассировки.
- ТРАССИРОВКА — ВЕРСИОНИРОВАННЫЙ ВХОД (коммит в репо, hash в ключе кэша узла пакинга) — иначе детерминизм и дельта-патчи ломаются; обновление трассировки = осознанный коммит.
- Группировка паков по тайлам world partition + общие ассеты отдельно — стриминг тайла читает 1–2 пака.
- Дельта-патчи: детерминированный билд (байт-в-байт на любом агенте) — требование, не пожелание.
- Hot-reload dev-профиля: перепаковка изменённых паков, рантайм перегружает по UUID-манифесту.
- CLI: flux build <profile>, flux rebuild <asset>, flux why <asset> («почему ассет в билде и кто его тянет»).

```text
flux-build/src/
├── graph.rs       # DAG: узлы, декларативные зависимости, проверка циклов
├── cache.rs       # content-addressed кэш (локальный + удалённый)
├── steps/         # адаптеры ступеней: assetc, shaders, navmesh, packs, manifests
├── packing.rs     # компоновка .fpf, сортировка, манифесты, дельты
├── profiles.rs    # dev/editor/shipping-проекции
└── cli.rs         # CLI-интерфейс
```

### Релиз

**Крейт:** `tools/flux-release` · **ID:** `release`

Один инструмент на весь релизный цикл: Conventional Commits → semver bump → версия в `Cargo.toml` → `CHANGELOG.md` → commit + annotated tag. Без Node и `semantic-release`: только `git`, `cargo` и опциональные `gh` / `cargo-semver-checks` (ADR-0040).

#### Контракты

- **Коммиты** — Conventional Commits 1.0: `type(scope)!: subject`, футеры `BREAKING CHANGE:`/`BREAKING-CHANGE:`. Merge-коммиты, `chore(release)` и заголовки с `[skip release]` исключаются.
- **Версии** — `feat` → minor, `fix`/`perf`/`revert` → patch, breaking → major; для `0.x` breaking по умолчанию даёт minor (major включает `bump_major_on_breaking_below_1_0`). Pre-release каналы `alpha`/`beta`/`rc` инкрементят номер внутри канала, смена канала начинает с `.1`.
- **Changelog** — Keep a Changelog: секции по типу (`Breaking/Features/Fixes/Performance/Refactoring/Reverts/Documentation/Internal`), внутри — модуль (`scope`), затем пункты; ссылки на коммиты, локализация en/ru. Внешний шаблон получает `{version}`/`{date}`/`{repo_url}`/`{sections}`. Вставка перед первой `## ` идемпотентна: повторный запуск не дублирует версию.
- **Манифесты** — `workspace.package.version` или `[package].version` через `toml_edit` без потери форматирования; path-зависимости с `version` обновляются только по имени пакета. Режимы `fixed` (одна версия/тег) и `independent` (версия на пакет).
- **Идемпотентность** — существующий тег делает `release` no-op; `--dry-run` не пишет ни файлов, ни коммитов/тегов.
- **Дисциплина CI** — `check --commits from..to --strict` на PR-диапазоне; `release` только на разрешённой ветке; `--package`, публикация, GitHub Release (gh или API) и `semver-checks` — opt-in.

```text
tools/flux-release/src/
├── config.rs     # .releaserc.toml/.json, fixed/independent, locale, commit_types
├── commit.rs     # парсер Conventional Commits: scope, !, BREAKING CHANGE
├── bump.rs       # semver, 0.x-политика, pre-release каналы
├── changelog.rs  # Keep a Changelog, секции, ссылки, en/ru, идемпотентная вставка
├── manifest.rs   # workspace.package/[package], path-зависимости, toml_edit
├── git.rs        # обёртка log/status/tag/commit/push, URL коммита
├── release.rs    # preview/apply/check/init/doctor, opt-in publish/gh
└── main.rs       # clap CLI
```

### Редактор

**Крейт:** `flux-editor` · **ID:** `editor`

Отдельное приложение поверх flux-engine (как игра). Работает над тем же ECS-миром, не над копией: Selected/Hidden/EditorOnly/Locked — обычные компоненты.

```text
flux-editor/src/
├── app/            # главный цикл редактора: своя версия game loop
├── viewport/       # рендер сцены в окно редактора, камеры, debug-draw
├── gizmo/          # манипуляторы: translate/rotate/scale, snapping
├── selection/      # picking (CPU/GPU), outline, multi-select, иерархия
├── inspect/        # инспектор компонентов через reflection/schema
├── undo/           # command stack, транзакции, диффы сцены
├── browser/        # asset browser: реестр, импорт, валидация
├── mode/           # edit <-> play, PIE, пауза, стейты мира
└── collaboration/  # точки расширения: блокировки ассетов, диффы
```

#### Зависимости редактора от рантайма

1. Reflection/schema-система — SchemaRegistry в flux-ecs: спроектирована при создании первого компонента, инспектор рендерится универсально по схеме.
2. Редактор работает над тем же ECS-миром, не над копией: компоненты Selected/Hidden/EditorOnly/Locked — обычные компоненты, игнорируемые в play-режиме. Стриминг в edit-режиме форсирован: StreamingPolicy::Edit.
3. Разделение play/edit состояния: edit-мир — авторитет; PIE создаёт клон мира, выход — восстановление edit-состояния. Механизмы PIE — reload-режим и клон мира (World::clone). Action-контекст переключается.

#### Viewport и гизмо

- Render-to-texture: сцена в offscreen target, текстура в egui-виджет — та же техника для camera preview, picture-in-picture, preview материалов.
- Editor-камера (fly/walk/orbit) через action layer flux-platform.
- Debug-draw — рантаймовая подсистема render/debug, не редакторский код; Quality::Editor профиль render graph.
- Translate/rotate/scale, локальные/мировые оси, snapping с настраиваемым шагом (10 см/1 м/10 м) — snapping критичен для художников.
- Экранное пространство: колба строится на CPU по камере, overlay-pass поверх сцены, пиктинг рейкастами по геометрии гизмо.
- Расширяемость: тип гизмо — поле в schema компонента (свет — кольцо направления, декаль — бокс, аудио-эмиссер — сфера слышимости).

#### Selection и undo

- Picking: CPU-рейкаст по мешам (BVH из flux-math + вершины из меш-ассетов); GPU-picking — опция для very dense сцен.
- Иерархия: клик по инстансу префаба выбирает инстанс; кнопка select root/prefab.
- Outline — стенсил/post-pass в render graph, включаемый флагом.
- Undo: command stack НАД операциями (MoveEntities, AddComponent, ApplyPrefabOverrides — do/undo пишут диффы (entity_uuid, component, value)), через UUID, не индексы. Drag гизмо = одна транзакция. Undo охватывает префаб-операции.

#### Undo × PIE × стриминг

- Edit-мир и PIE-мир — два состояния: PIE-мутации живут в клоне (isolated Engine-инстанс), выход из PIE = выброс клона; edit-состояние восстанавливается ПО ПОСТРОЕНИЮ. НЕ «снапшот перед PIE и восстановление».
- Undo-стек ОДИН — на edit-мир. Пока PIE активен, edit-undo заблокирован на уровне UX: правки в невидимый мир — источник «куда пропал мой ящик».
- «Оставить изменения из PIE» — ЯВНОЕ действие: PIE-сессия накапливает собственный дифф-журнал; применение = весь журнал атомарно ОДНОЙ undo-записью; по умолчанию — НЕ применяется.
- Undo × eviction: undo-записи ссылаются на дельты в SaveState, SaveState переживает выгрузку тайла → откат правки на выгруженном тайле работает на уровне SaveState БЕЗ загрузки тайла.
- Камера редактора — НЕ undoable (undo двигает мир, не вьюпорт).
- Откат создания/удаления — через UUID + generational slots flux-ecs (revived-сущность не спутается с новой на том же слоте).
- Глубина стека — на диффах canonical-пространства, не снапшотах; сжатие старых записей сверх N — политика.

#### Катсцены / секвенсор

- Отдельной подсистемы не требуется: база даёт секвенсору всё — camera-as-entity, render graph per-viewport (PiP, превью, split-screen), события AnimGraph → скрипты, SimTime, PIE-клон мира, дифф-формат + SaveState.
- ЕДИНСТВЕННОЕ обязательное правило: треки, влияющие на мир (триггеры, движение акторов, события) — исполняются в СИМУЛЯЦИИ по SimTime: детерминированы, реплицируются, попадают в реплеи.
- Presentation-треки (монтаж камеры, музыка, титры) — wall-clock, но НЕ мутируют мир.
- Смешанный трек (двигает и кадр, и дверь) = источник рассинхрона клиентов и реплеев — ЗАПРЕЩЁН. Разделение — маркер трека в формате секвенсора.
- Катсцена × стриминг: перемещение камеры через полкарты — priority-запросы стриминга.

#### Asset browser и DX

- Браузер — клиент над реестром flux-asset: превью (мини-рендер мешей, миниатюры, waveform), импорт, валидация битых ссылок (красный маркер).
- Import settings: per-asset конфиг рядом с исходником в репо (<source>.import), переживает реимпорт; артефакты — в .fpf.
- egui + общий шрифт-пайплайн с flux-ui; docking (egui-dock) — опционально.

## Сквозные

_паники, pacing, пермутации, тесты, координаты_

### Моддинг: точки расширения

**ID:** `modding`

Моддинг не вводит новых подсистем — опирается на существующие:

- multi-pack VFS = мод как ещё один пак;
- UUID-центричный registry = мод переопределяет ассет по UUID;
- content-hash = верификация и совместимость версий;
- orphaned-политика = чужие UUID не роняют игру;
- event sourcing = модификации геймплейного состояния не ломают сейвы;
- BT/AnimGraph как data-ресурсы = моды правят поведение/анимации без кода;
- скрипты как компоненты = база логических модов;
- детерминизм-харнесс = проверка «мод не ломает симуляцию».

#### Рантайм-контракты

1. Упорядоченное разрешение UUID между паками в VFS (поздний источник перекрывает ранний — мод поверх ванилы) — параметр загрузчика.
2. Слот подписи в манифесте паков — резерв поля БЕЗ смены формата.
3. Правило модифицируемости: мод может менять ТОЛЬКО данные, помеченные разметкой в SchemaRegistry (#[replicated]/#[save]-классы); WIRE-ПРОТОКОЛ модами НЕ расширяется — иначе любой мод ломает сетевую совместимость.

#### Точки расширения

- Sandbox стороннего кода (WASM): изолированный runtime — отдельная подсистема; точка расширения — единый скриптовый ABI в flux-script (внутренние и внешние скрипты для ECS выглядят одинаково).
- Подпись и валидация модов: требует инфраструктуры распространения; точка расширения — слот подписи в манифесте.
- Политика совместимости версий модов — продуктовая проблема, решается через orphaned-политику.

### Ошибки и паники (ADR-0012)

**ID:** `panics`

Ожидаемый отказ → Result. Нарушение инварианта (баг) → паника. Не смешивать.

#### Типы ошибок между крейтами

- thiserror, пер-крейтовые enum-ы (AssetError, GraphicsError, AudioError); единого FluxError НЕТ — ошибки не создают рёбра между крейтами.
- Кросс-крейтовые: конкретные типы нижележащего крейта (GraphicsError::AssetDecoded(AssetError)).
- anyhow — только в бинарниках (examples, editor, assetc); в lib-крейтах запрещён.
- unwrap без контекста — lint-ошибка (clippy::unwrap_used в CI на lib); expect только с комментарием-инвариантом.
- debug_assert! для дорогих проверок (биндлесс-индексы, generation-слоты, инварианты архетипов) — в release нулевая цена.

#### Матрица паник по режимам

| Режим | Поведение |
|---|---|
| Dev-игра | catch на границе кадра: красный экран + бектрейс + лог; кадр сброшен, состояние suspect |
| Editor PIE | catch: остановка play-режима, откат к edit-состоянию, отчёт — редактор обязан выжить |
| Editor edit | паника редактора = crash dump + выход |
| Release-игра | фатально: дамп через CrashHandler, аккуратный выход без OS-диалога |

- panic = "unwind" везде (abort ломает unwind-границы job system и дампы в процессе); двойная паника → abort (fallback).
- FrameAborted: кадр отбрасывается, мир помечен suspect; в dev допустимо, в шипе ведёт к падению.
- GPU при aborted-кадре: записанные-не-отправленные CmdBuffers отбрасываются; уже отправленные (transfer-батчи prepare-фазы) НЕ откатываются — fence-ы идут, результат валиден; биндлесс-слоты не откатываются (прокси делают aborted-кадр безопасным для GPU).

#### Job system — механика

- Каждый джоб в catch_unwind(AssertUnwindSafe) на воркере; воркер НЕ умирает, записывает статус Panicked(payload) и продолжает очередь.
- Статус поднимается по графу: JobHandle → Ok | Panicked; зависимые системы через then() не исполняются при панике предка; корневой хэндл кадра агрегирует: хотя бы одна паника → FrameAborted.
- Payload: &'static str/String из Box<dyn Any>, иначе unknown panic; в лог — тип, сообщение, бектрейс (паник-хук), job-метаданные.
- Poison-семантика минимальна: scratch-память джоба — frame arena (сбрасывается раз в кадр); общая память — только через scheduler с явной зависимостью.
- Замки: parking_lot без poisoning; инварианты — debug_assert! при захвате.
- Единый паник-хук на весь движок (ставится из flux-engine): лог + breadcrumbs (lock-free ring) + счётчик diagnostics. Больше одного хука нельзя.

#### FFI-границы и RT-audio

- Unwind через extern "C"/"system" — UB; любой callback в чужом стеке вызовов оборачивается в catch_unwind + конверсия в error-код.
- Аудио RT-поток: панить нельзя даже внутри Rust (дроп-глитчи); flux-audio/device перехватывает всё на входе → silent-output + error state.
- Drop не паникует (flush-ошибки через явные методы).
- Vulkan/DX12 вызовы через ash/D3D12 безопасны (panic внутри Rust-кода).

### Frame pacing и input latency (ADR-0013)

**ID:** `pacing`

Swapchain image count vs frames-in-flight — РАЗНЫЕ числа, два независимых параметра RHI.

Дефолт frames-in-flight = 2, 3 — конфигурируемо через rebuild (перераспределение per-frame пулов, учёт в VramBudget targets_reserve).

- Все per-frame ресурсы — кольцо по индексу кадра: FrameArena (N арен), staging-кольцо, uniform-данные (N копий), RenderScene-снапшоты (очередь глубины N), per-frame fences.
- Все ожидания GPU — ТОЛЬКО в acquire-точке начала кадра, никогда посреди записи команд.

#### Синхронизация sim/рендер

- sim — fixed 60 Гц детерминированно, рендер — uncapped (до cap fps); рендер берёт два последних снапшота симуляции и интерполирует: pos = lerp(prev, curr, alpha), вращения — slerp.
- ИНТЕРПОЛЯЦИЯ В f64: lerp в DVec3 / slerp в DQuat, cast в f32 camera-relative ПОСЛЕ (интерполяция в f32 размазала бы выгоду origin rebasing).
- Late-latch камеры: позиция камеры — интерполяция из снапшотов; локальная ОРИЕНТАЦИЯ взгляда — свежее значение из последнего инпута, переписывается в record-стадии. view-матрица — единственный параметр, переписываемый в record.

#### Present-режимы

| Режим | Latency | Комментарий |
|---|---|---|
| FIFO (VSync) | +1–2 кадра | дефолт «тихой» игры |
| FIFO Relaxed | как FIFO + tearing при опоздании | компромисс |
| Mailbox | низкая | ДЕФОЛТ: заменяет последний кадр, нет backlog при просадке |
| Immediate | минимальная | tearing; competitive/бенчи |

Vulkan: VK_PRESENT_MODE_*; DX12: Present() + SwapInterval. Переключение через configure(); user-friendly «VSync: On/Off/Adaptive» — маппинг UI-задача. DX12 present-телеметрия: DXGI_FRAME_STATISTICS.

#### Frame pacing — цель: низкий JITTER frametime, не максимальный FPS

- Burstы CPU → time-box систем в scheduler (бюджет per-stage).
- Burstы GPU → вариативность качества (VramBudget + presets).
- OS-планировщик → frame limiter (целевой FPS с полкадром задержки).
- Presentation timing: VK_GOOGLE_display_timing / present statistics, DXGI_FRAME_STATISTICS — «когда кадр реально показан»; без этого не отличить медленный рендер от медленной очереди презентации.
- Метрики профайлера: percentiles frametime (P99), пропущенные интервалы present, delta запрошенного/фактического present-времени — в dev-overlay.

> **Input latency**
>
> raw mouse + таймстемпы, input buffering между шагами sim; late-latch — главная экономия; Mailbox + 2 frames-in-flight не копят latency в очереди (FIFO при опозданиях +2 кадра ~33 мс); vendor low-latency API (Reflex/Anti-Lag) — ячейка в FeatureSet, не блокер.

### Шейдерные пермутации (ADR-0014)

**ID:** `permutations`

Два уровня, не смешивать: функциональные пермутации (что шейдер делает) и keywords материала (что шейдер умеет).

- Функциональные пермутации: deferred_gbuffer / forward / deferred_lighting — варианты пайплайна.
- Keywords материала: NORMAL_MAP, PARALLAX, SKINNING...
- PSO = шейдерная пермутация × vertex layout × blend/raster state × pass форматы. Шейдерных вариантов — тысячи, PSO — сотни; кэш раздельный.

Модель: шейдер = ассет с декларацией (shader.json рядом с pbr.frag, модуль flux-graphics/shaders/declarations): stages, pipeline_variants, keywords (три набора с явными источниками), vertex_inputs, биндинги. НЕ #ifdef-инклуды.

- Наборы keywords: pipeline (render graph/FeatureSet: CLUSTERED_LIGHTING, SHADOWS_CSM), mesh (SKINNING, MORPH_TARGET), material (NORMAL_MAP, PARALLAX, EMISSIVE).
- Полный ключ = pipeline × mesh × material — вычисляется в extract, без ECS и блокировок.
- Валидация биндингов при компиляции: шейдер требует биндинг, которого пермутация не получит — ошибка сборки, не чёрный экран в проде.
- UUID варианта детерминированный: hash(base shader UUID + keyword-маска).

> **Runtime-ядро (ADR-0036)**
>
> `ShaderDeclaration` из `shader.json` (stages, pipeline_variants, три набора keywords, vertex_inputs, биндинги), `KeywordMask` (бит = индекс keyword в декларации), `PermutationRequest` с FNV-1a-ключом, `ShaderLibrary` с прекомпилированным ядром fallback-ов и `PermutationCache` (байткод/PSO/материал). DXC-компиляция HLSL → SPIR-V — путь flux-assetc.

#### Компиляция off-demand

- Прекомпилируется узкое ядро: варианты без material-keywords для каждого pipeline-варианта (десятки) — покрывают первые кадры и fallback.
- Остальное по требованию: miss в кэше → job в job system с приоритетом (видимые в кадре > спекулятивные) → компиляция → публикация с барьером.
- Fallback-шейдер (упрощённый fragment, тот же vertex layout) на время компиляции — та же философия graceful degradation, что прокси-текстуры.
- Пайплайн: HLSL + keyword-подстановка на препроцессоре → DXC → SPIR-V/DXIL. Своего IR нет.
- Параллельная компиляция: CreatePipelineState (DX12) и vkCreatePipeline (Vulkan, отдельный VkPipelineCache на поток) — thread-safe, пачка на jobs.
- Связь с rebuild_graph: смена пресета меняет pipeline-набор keywords; «старый PSO живёт до готовности нового».

| Уровень кэша | Ключ | Хранение |
|---|---|---|
| Bytecode SPIR-V/DXIL | hash(исходник + keywords) | память + диск (dev) |
| PSO | hash(bytecodes + state + pass форматы) | диск, per-driver cache |
| Материал | (shader UUID, keyword-маска) | в .fpf рядом с материалом |

Путь материал → шейдер: MaterialAsset { shader: ShaderUuid, keywords: KeywordMask, textures, params } → DrawKey { pipeline_variant, mesh_flags, material_keywords } вычисляется в extract → PSO lookup по hash(DrawKey), miss → job + fallback. Draw-списки сортируются по PSO-ключу перед записью команд; вторичная сортировка по биндлесс-дескрипторам.

> **Бюджет пермутаций**
>
> Пример PBR — 2 pipeline × 2 mesh × 16 material = 64 варианта (норма). Лимит material-keywords на шейдер (≤ 6 bool, иначе дробление через ubo-branching); счётчик живых пермутаций в dev-логе и asset-compiler — рост сверх лимита = красный флаг.

### Тестирование и CI (ADR-0021)

**ID:** `testing`

Тесты как контракты (инвариант-центричные): код тестируется не «что делает», а «что не имеет права делать». Большинство багов движка — регрессии инвариантов.

| Уровень | Частота |
|---|---|
| Юнит-тесты крейтов (math, serialization, ECS-квери, диффы) | каждый PR |
| Интеграция подсистем (asset→render, стриминг, job-зависимости) | каждый PR |
| Headless-рендер (offscreen, софтверный бэкенд) | каждый PR |
| Golden-image (короткий набор) | PR + полный nightly |
| Детерминизм (двухмашинная симуляция, replay) | nightly |
| Fuzzing (.fpf, wire, десериализация) | непрерывно + nightly |
| Perf-регрессии (бюджетные гейты) | nightly |
| Soak 24–48 ч с ботами | еженедельно |

> **Headless-рендер — фундамент**
>
> swapchain подменяется offscreen-таргетом (PresentTarget-абстракция это позволяет); lavapipe/Mesa в PR-гейтах (256×256 контрольных кадров достаточно), реальные GPU — nightly-матрица; validation layers ВСЕГДА в CI; leak-трекинг «выделено N — освободилось N» после каждого рендер-теста.

#### Golden-image

- Контролируемый захват: фиксированная камера/время/seed; TAA-jitter — детерминированная последовательность; захват до и после тонмаппинга (два класса багов).
- Сравнение с допусками: PSNR/SSIM на весь кадр + пиксельный диф-канал (процент пикселей > порога — ловит локальные артефакты); артефакты PR — тепловые карты диффов.
- Бейзлайны — ассеты в репозитории; изменение бейзлайна = PR «visual change» с ревью диффов. ПО РАННЕР-КЛАССАМ (lavapipe отдельно, каждый vendor nightly отдельно).
- Покрытие матрицы пермутаций: deferred/forward × static/skinned × основные keyword-комбинации + проверка fallback-шейдеров.

Fuzzing: .fpf-парсер (cargo-fuzz/libFuzzer, coverage-guided), десериализаторы ассетных форматов, wire-протокол flux-net. Инвариант: НЕ паникует никогда на недоверенном вводе; panic → минимальный кейс → regression-корпус навсегда. Property-based (proptest) для ECS. Sanitizer-сборки — weekly.

Детерминизм — гейт: same-binary, cross-binary (dev vs release), cross-platform (linux x86 vs windows x86, nightly), replay-тесты баг-репортов. Без fast-math, без CPU-специфичных инструкций в геймплее.

Perf — бюджетные гейты: шаг симуляции 16,6 мс, extract 1 мс, компиляция пермутации, загрузка тайла, применение дельты тика; criterion-класс бенчей; регрессия > 5% = красный nightly; выделенные раннеры без виртуализации, фиксация частоты CPU.

Soak: 24–48 ч headless-симуляции с ботами; метрики: RSS-тренд, frametime-тренд, расхождения дельт. Единственный тест, ловящий «работает день — начинает тормозить».

| Раннер | Нагрузка | Частота |
|---|---|---|
| CPU-only PR | юнит, интеграция, headless, короткий golden, clippy + unwrap-lint, headless-зависимостный линтер | каждый push |
| GPU-матрица | полный golden на NVIDIA+AMD+Intel, validation | nightly |
| Fuzz-фарм | непрерывный прогон, крэши в regression | непрерывно |
| Perf-раннер | выделенное железо | nightly |
| Soak-раннер | headless-кластер с ботами | еженедельно |

- Артефакты падений автоматичны (диф-картинки, фаззер-кейсы, perf-тренды, soak-графики).
- Кэш PSO-кэша и ассетов между ранами.
- Красный nightly блокирует мерж в main (red-green дисциплина).
- Один пайплайн для всех платформ — только флаги.

### Платформы (ADR-0004)

**ID:** `platforms`

Release-цель: Windows. Linux — dev/CI (не release); macOS/Switch/консоли — осознанный отказ на текущем горизонте.

Консоли НЕ удалены из архитектуры навсегда: контракт RHI позволяет добавить бэкенд (PS AGC/GNM — приватный крейт за NDA, Xbox — GDK-расширения DX12 за feature-gate) без изменения trait-набора.

- RHI-бэкенды: DX12 — primary release-бэкенд (Windows); Vulkan — dev/CI (lavapipe, отладка) и PRIMARY dev-бэкенд: CI-гейты (headless, golden PR-тесты) живут на нём.
- Память: только дискретные GPU и iGPU Windows — формула VramBudget без консольных профилей.
- Toolchain: один публичный CI (linux/windows), приватные консольные раннеры не требуются.
- Crash: Windows-реализация (MiniDumpWriteDump) — базовая; контракт CrashHandler сохраняет точку расширения.
- Input: Xbox-контроллер через gilrs (XInput) — основная поддержка; DualSense через HID — опционально.
- Детерминизм: cross-platform CI-тест (linux x86 vs windows x86) остаётся инвариантом дисциплины флоатов.
- Ассеты .fpf — платформенно-нейтральны; один пайплайн — только флаги.
- Headless-сервер: Windows/Linux.

### Три пространства координат (ADR-0008)

**ID:** `coordinates`

Проблема: не конечная ошибка позиции (1,7 мм на краю 120 км²), а потеря относительной точности внутри меша при умножении f32-матрицы с большой трансляцией + z-fighting дальней геометрии.

| Пространство | Тип | Назначение |
|---|---|---|
| Канон | WorldPos(i64 x3, нанометры) | ЕДИНСТВЕННЫЙ источник истины: сейвы, сеть, стриминг, extract. Покрывает ±2147 км. Float — только вычислительное представление, НИКОГДА хранилище мировой позиции. |
| Сим | f32, sim-region floating origin | origin = центр масс активных сущностей или позиция первого игрока — детерминированное правило. Rebase ТОЛЬКО на границах тиков по детерминированному триггеру. |
| Рендер | f32, camera-relative | origin = позиция камеры, snap к канону; одно вычитание в f64 до приведения к f32. Вершины внутри меша — идеальная точность на любом расстоянии. |

#### Стыковки

- Физика: ИСКЛЮЧЕНИЕ из sim-region — собственная физика работает на f64 относительно канона (конверсия без потерь), rebase её не касается. Связь sim-пространства с физикой: конверсия в точках взаимодействия, НЕ через общий rebase.
- Иерархия ECS: GlobalTransform = канонический WorldPos(i64) + Quat + scale; локальная иерархия аккумулируется в f32 внутри sim-региона.
- Ассеты: квантизация мешей к origin чанка стыкуется без пересчёта: тайл-инстанс локален тайлу, тайл локален sim-региону, регион — канону.
- Стриминг: потоковая математика (какой тайл грузить) — ТОЛЬКО в каноне, целочисленно.
- Сеть: позиция на проводе = ChunkId + i32-смещение, дельты в целых числах; клиент восстанавливает camera-relative.
- Редактор: камера — f64 из канона (плавание к дальним объектам без джиттера).
- Аудио: слушатель — одна точка конверсии; spatialization локальна.
- Частицы/декали — тайл-локальное пространство; пересечение границ тайлов — одна политика, выбираемая конфигом.
- Тени/cascades — камера-центричны, точность бесплатно.

> **Reversed-Z — обязательный компонент**
>
> D32 reversed-Z (near=1, far=0, float depth) + бесконечная far-plane матрица; в RHI — конфиг depth-stencil state.

Затраты: одна конверсия на трансформ в extract (микросекунды на кадр), один детерминированный rebase в симуляции, один канонический тип. Альтернатива «f32 везде» — плавающий мир с трещинами чанков на 8-м км; «f64 везде» — отклонено (память, скорость, GPU-непригодность).
