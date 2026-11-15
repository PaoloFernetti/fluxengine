# FluxEngine — канон имён и терминов (canon.md)

Этот документ — **единственный источник истины** по именам крейтов и терминам.
Любой другой документ (architecture.md, contracts.md, README крейтов) ссылается
сюда, а не повторяет имена своими словами.

## Правило ревью

Каждый термин из договорённостей обязан иметь:
1. запись в глоссарии ниже,
2. крейт-владелец,
3. место в структуре workspace.

Документ, использующий имя крейта, обязан соответствовать канон-таблице.

## Канон-таблица крейтов

| Крейт | Ответственность |
|---|---|
| `flux-math` | Математика (re-export glam), типы f32/f64, camera-relative утилиты |
| `flux-core` | time, jobs, memory, events, logging, profiling, config, platform, crash (контракт), contracts |
| `flux-ecs` | Архетипная ECS, SchemaRegistry, scheduler |
| `flux-graphics` | rhi (trait) + rhi::vulkan + rhi::dx12 (cfg-модули) + render (scene, graph, pipeline, culling, lighting, post, debug) |
| `flux-platform` | window, display, input (raw/mapping/curves/devices), cursor, clipboard, text, paths, power, crash (per-OS реализации) |
| `flux-asset` | ContainerReader .fpf, реестр, Handle, стриминг, eviction-решения |
| `flux-scene` | scene, partition, streaming, prefab, entity, serialize, save |
| `flux-physics` | Собственная физика на f64 вне sim-region rebase; минимальный collision-слой + KCC-минимум — этап 3 (PIE); ядро (broadphase/solver/CCD/joints) — этап 6 |
| `flux-anim` | components (AnimPlayer/AnimGraph/AnimEvents), systems (FSM→sampling→retarget→blend→IK), physics (ragdoll, root motion), network, render (GPU-skinning) |
| `flux-ai` | nav (navmesh по тайлам, coarse-граф, динамические запреты), path (иерархический A*), behavior (BT/HFSM, perception), crowd (RVO, flow fields), AI-LOD |
| `flux-script` | core (ScriptVM trait), lua (mlua 5.4), api, registry, graph (этап 7+) |
| `flux-net` | relevance, replication, ownership, transport (UDP/Loopback), session, topology, persistence (интерфейсы), headless CI |
| `flux-audio` | device (cpal), mixer, spatial, dsp, codec (symphonia/opus), streaming, events |
| `flux-ui` | immediate, draw, text, input, widgets, localization, egui_bridge |
| `flux-editor` | app, viewport, gizmo, selection, inspect, undo, browser, mode, collaboration |
| `flux-engine` | bootstrap, runtime, plugins, context, settings (фасад) |
| `flux-macros` | Служебный proc-macro-пакет: `#[derive(Reflect)]`, `#[save]`, `#[replicated]`, `#[reflect(...)]` (ADR-0028) |
| `tools/flux-assetc` | Компилятор ассетов: KTX2, меши, Opus, .fpf |
| `tools/flux-build` | Оркестратор сборки: DAG на content-hash, кэш (локальный+CI), профили dev/editor/shipping, пакинг, дельта-патчи, flux why |
| `tools/flux-release` | Релиз-инструмент: Conventional Commits → semver bump, CHANGELOG, версия Cargo.toml, commit/tag, pre-release, монорепо, publish (ADR-0040) |

**Итого: 16 крейтов движка + 3 инструмента + flux-macros — служебный
proc-macro-пакет (не единица канона: не движок и не инструмент, только
дерайвы). Workspace members — 20; приёмка — по именованному перечню,
не по числу (ADR-0028).**

Уточнение: flux-rhi / flux-rhi-vulkan / flux-rhi-dx12 / flux-render как
отдельные крейты НЕ существуют — это вложенные модули flux-graphics.
Если позже решим выделить RHI в отдельный крейт — правка делается
только здесь.

Платформы (release-цель): Windows. Linux — dev/CI (не release); macOS/Switch/
консоли — осознанный отказ на текущем горизонте. Консольные бэкенды RHI
(PS — приватный крейт за NDA, Xbox — GDK-расширения DX12) добавляются за
feature-gate без изменения trait-набора, если появится бизнес-цель.
Vulkan — primary dev-бэкенд (CI-гейты headless/golden живут на нём).

## Глоссарий терминов

| Термин | Значение | Владелец |
|---|---|---|
| `RhiDevice` | Графическое устройство (Vulkan/DX12 за trait); создаёт flux-engine, живут дольше рендереров | flux-graphics/rhi |
| `RenderScene` | SoA-снапшот ECS-кадра для рендера (camera-relative f32 + alpha) | flux-graphics/render/scene |
| `PresentTarget` | Выход кадра: swapchain или RTT-текстура (editor) | flux-graphics/render |
| `UI-stub` | Панель-заглушка финального прохода `frame.ui`: проверяет, что оверлей попадает в presented-кадр (ADR-0048) | flux-graphics/render/frame |
| `RenderBackend` | Per-viewport рендерер: configure / rebuild_graph / render_frame | flux-graphics/render |
| `Render Graph` | Декларативный граф пассов кадра; barrier-и, aliasing, async compute | flux-graphics/render/graph |
| `Pass-фабрика` | Регистрируемый провайдер пассов (UI, egui, debug-draw) | flux-graphics/render/graph |
| `TransferSystem` | Upload-подсистема RHI: staging-кольцо, батчи, барьеры | flux-graphics/rhi |
| `VramBudget` | Единственный владелец VRAM-бюджета; pressure-лестница | flux-graphics/rhi |
| `ResourceRegistry` | Handle ↔ GPU-ресурс, VramLease, биндлесс-слоты | flux-graphics/rhi |
| `Proxy-ресурс` | Прокси-текстуры/геометрия для незагруженных ассетов | flux-graphics/render |
| `Quality::Editor` | Упрощённый профиль render graph для редактора | flux-graphics/render/graph |
| `GraphicsSettings` | Настройки графики: HardwareTier → QualityPreset → гранулярность | flux-graphics |
| `GraphicsFeatureRegistry` | Список параметров для авто-меню настроек игры | flux-graphics |
| `.fpf` | Контейнер паков: header/index table/chunks, LZ4|Zstd | flux-asset (чтение), tools/flux-assetc (запись) |
| `Handle<T>` | Generational-хэндл ассета (UUID) | flux-asset |
| `StreamingPolicy` | Стратегия стриминга: Play / Edit (форсированная загрузка) | flux-scene |
| `ScriptVM` | Trait скриптового рантайма: load/update/handle_event/serialize_state | flux-script/core |
| `ScriptRef` | Компонент скрипта: UUID + состояние, живёт в тайле (усыплён при выгрузке) | flux-script + flux-ecs |
| `RelevanceSystem` / `RelevanceProvider` | Один расчёт релевантности — сеть, стриминг, simulation LOD; контракт в core, реализация в net | flux-net + flux-core/contracts |
| `SimTick` | Детерминированный тик симуляции; сетевая синхронизация и handoff | flux-core/time |
| `SchemaRegistry` | Схемы компонентов как данные: сериализация, инспектор, #[replicated] wire-схемы | flux-ecs |
| `WorldPos` | Каноническая позиция: i64 fixed-point (нанометры), единственный источник истины | flux-math |
| `SimRegion` | Sim floating origin: f32-пространство симуляции, rebase детерминирован | flux-math + flux-ecs |
| `World Partition` | Грид-тайлы мира, паковые регионы .fpf, HLOD | flux-scene/partition |
| `Prefab` | Ассет-шаблон сущности, override-слой | flux-scene/prefab |
| `SaveState` | Динамический слой мира (диффы сейва поверх сцены) | flux-scene/save |
| `World` | ECS-мир (один на игру + клоны для PIE/тестов) | flux-ecs/world |
| `Архетип` | Группа сущностей с идентичным составом типов компонентов; ключ — `Signature` | flux-ecs/archetypes |
| `Chunk` | Непрерывный блок памяти внутри архетипа: N слотов, SoA-колонки; единица аллокации/итерации/синхронизации | flux-ecs/archetypes |
| `Schedule` | Детерминированное упорядочение стадий и систем с тай-брейком по имени | flux-ecs/scheduler |
| `ComponentDelta` | Единица изменения: `entity_uuid + SchemaRef + поля`; один формат для SaveState, сети и undo | flux-ecs/save |
| `Scancode` | Физический код клавиши (не символ раскладки); база raw-слоя ввода | flux-platform/input/raw |
| `ActionValue` | Значение действия: `Bool/Axis1/Axis2/Axis3`; единица mapping-слоя | flux-platform/input/mapping |
| `Intention` | Снапшот намерения на тик (`move_axis`, `look`, `jump`, `crouch`) — единственный вход симуляции; общий для игрока, бота и AI | flux-platform/input/intention |
| `Collision-минимум` | Тайловый grid + свипы капсулы/луча без solver; этап 3, ядро физики — этап 6 | flux-physics/collision |
| `CharacterState` | Сериализуемое состояние KCC (позиция, скорость, grounded, нормаль, гистерезис); скрытых полей контроллера нет | flux-physics/kcc |
| `DeterminismHarness` | Headless-прогон бота: N тиков, канонический хеш состояния по `SaveJournal`-диффу | flux-engine/harness |
| `Headless-профиль` | Сборка/цикл без окна и GPU: `windowing` выключен, только `Engine::tick` | flux-engine |
| `AnimPlayer` / `AnimGraph` | Компонент анимации и data-driven граф в .fpf | flux-anim |
| `RootMotion` | Накопление движения анимации за тик → Transform → физика через канон/f64 | flux-anim/physics |
| `Navmesh` / `Coarse-граф` | Запечённый по тайлам навмеш с UUID-полигонами; резидентные порталы тайлов | flux-ai + tools/flux-assetc |
| `AI-LOD` / `Animation LOD` | Частота тиков BT/sampling по ярусам RelevanceSystem | flux-ai + flux-anim |
| `BehaviorRunner` | BT/HFSM-раннер: continuation, blackboard #[save], TicksBudget | flux-ai/behavior |
| `G-buffer layout` | RT0 albedo RGBA8 sRGB, RT1 oct-normal RGBA16F, RT2 ORM + bindless-индекс расширений, D32 reversed-Z — ABI рендера | flux-graphics/render/pipeline |
| `PBR-модель` | Lambert + GGX (Smith height-correlated, multiscatter), metallic/roughness glTF; расширения keyword-ами (clearcoat/sheen/SSS-cheap/aniso) | flux-graphics/render/pipeline |
| `Тон-маппинг` | Семейство ACES-fitted / AgX — художественный ABI, фиксируется с этапа 4 | flux-graphics/render/post |
| `Build DAG` | Узел = входы + версия инструмента + конфиг; глоб-паттерны и циклы запрещены | tools/flux-build |
| `Профиль сборки` | dev / editor / shipping — проекция от корневых сцен; недостижимое не пакуется | tools/flux-build/profiles |
| `Трассировка доступа` | Версионированный вход пакинга (коммит в репо) для детерминированной сортировки паков и дельта-патчей | tools/flux-build/packing |
| `AudioNode` / `Routing` | Сигнальный граф: голоса → шины → мастер, send/return, sidechain control-rate; RT-контракт без аллокаций | flux-audio/mixer |
| `Intention` / `KCC` | Абстрактное намерение движения (клава=геймпад=боты); Character Controller — чистая функция (state, intention, dt) на фиксированном тике, client-owned | flux-physics + flux-platform |
| `Root motion × KCC` | Единовладение движением: флаг из flux-anim, конфликт = ошибка конфигурации | flux-anim + flux-physics |
| `Моддинг` | Отложенная фаза; задел: UUID-резолвинг VFS (поздний перекрывает ранний), слот подписи в манифесте, правило модифицируемости через SchemaRegistry; wire-протокол модами не расширяется | flux-asset + flux-script (точка расширения WASM) |
| `Секвенсор / треки` | Отложено (эт. 6), задел не требуется; sim-треки (влияют на мир) по SimTime, presentation-треки на wall-clock без мутации мира; маркер трека в формате; стриминг за катсценой — priority-запросы | flux-editor + flux-anim (этап 6) |
| `SimTime` | Детерминированный компонент времени суток мира: реплицируемый, сейвимый, 16 бит/сутки | flux-scene + flux-core/time |
| `Environment` / `SunSky` / `WeatherVolume` | Компоненты окружения (факт); вычисления — у потребителей | flux-scene (данные) + flux-graphics/lighting |
| `Weather state machine` | Data-driven ресурс с seeded-RNG, в SaveState и репликации | flux-scene (симметрия AnimGraph/BT) |
| `SchemaRef` / `schema_rev` | Версия схемы типа на каждой записи (диск и провод); рев типа ≠ версия игры | flux-ecs (SchemaRegistry) |
| `Миграция схем` | Read-time по умолчанию, цепочки чистых функций, UUID полей — идентификатор совместимости; golden-сейвы в CI | flux-scene/save + flux-ecs |
| `Аудио-LOD` | Виртуализация голосов по ярусам RelevanceSystem — пятый потребитель | flux-audio/mixer |
| `Commands` | Отложенные ECS-операции, применяются в конце фазы | flux-ecs/world |
| `Tick` | Метка кадра для change detection компонентов | flux-ecs/change |
| `JobHandle` / `JobCounter` | Задача job system с DAG зависимостями | flux-core/jobs |
| `FrameArena` | Frame allocator: очистка раз в кадр | flux-core/memory |
| `GenerationalIndex` | Entity/Handle-семантика (index + generation) | flux-core/memory |
| `FixedTimestep` | Фиксированный шаг симуляции с аккумулятором | flux-core/time |
| `TimeContext` | Пауза, time scale, slow-mo | flux-core/time |
| `MainThreadTaskQueue` | Задачи, обязательные на главном потоке | flux-core/jobs |
| `CrashHandler` | Контракт краш-дампов; реализации per-OS | flux-core/crash + flux-platform/crash |
| `Origin rebasing` | Camera-relative координаты: f64 мир → f32 рендер | flux-math |
| `HLOD` | Hierarchical LOD: запечённые импосторы дальних тайлов | flux-scene/partition |
| `pressure` | Уровень давления VRAM: Ok/Warning/Critical/Overflow | flux-graphics/rhi |
| `InputAction` / `ActionValue` | Действия input abstraction layer (Bool/Axis1–3) | flux-platform/input |
| `AAMode` / `Upscaler` | Сглаживание (FXAA/MSAA/TAA/DLAA) / апскейлинг (DLSS/FSR) | flux-graphics |

## Формат решений

Все изменения канона фиксируются здесь; в [architecture.md](architecture.md)
остаются архитектурные решения. При расхождении приоритет — у этого файла.
