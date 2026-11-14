# FluxEngine — Требования (MoSCoW)

Формат: Must / Should / Could / Won't. Для каждого пункта — критерий «готово»
(Definition of Done, DoD). Приоритеты соответствуют этапам
[Roadmap.md](Roadmap.md); изменение приоритета — через ADR.

## Must (без этого v1.0 не существует)

| # | Требование | DoD |
|---|---|---|
| M1 | Workspace + flux-math + flux-core | Майлстоун M1 + гейт G1: окно, 2 параллельных job, паника в job не убивает процесс, breadcrumbs, 0 unwrap в lib, 0 аллокаций в кадре |
| M2 | Архетипная ECS + SchemaRegistry | Гейт G2: 10k сущностей × 3 системы в бюджете (порог — Roadmap); UUID полей; SchemaRef в формате |
| M3 | Platform: окно, DPI, input abstraction (4 слоя) | G3: AZERTY-тест, raw mouse, deadzone/rescale, rebinding roundtrip |
| M4 | Детерминизм-харнесс (боты, сверка хешей) | G3+: 2 headless-ноды, 10k тиков, хеш совпадает (CI) |
| M5 | KCC-минимум + collision-слой | Капсула ходит по плоскости детерминированно (бот) |
| M6 | Graphics: RHI Vulkan, Render Graph, PBR, G-buffer ABI | G4: golden на lavapipe (порог — Roadmap), validation чистые, leak 0 |
| M7 | Asset-пайплайн .fpf + flux-asset + стриминг + SaveState | G5: тайлы стримятся, eviction не теряет дельты, fuzz без крэшей (длительность — Roadmap) |
| M8 | flux-scene: манифест, prefab, world partition, миграции сейвов | Golden-сейвы в CI; roundtrip-миграции |
| M9 | flux-net loopback + relevance + дельты | 2 ноды, 1k сущностей — хеш сходится |
| M10 | Паник-политика по режимам (матрица №10) | Тест: паника в геймплее dev → красный экран, процесс жив; release → дамп и выход |
| M11 | CI: fmt/clippy/tests/линты зависимостей | Красный CI блокирует мерж |

## Should (нужно для полноценного ощущения игры, допускается v1.0 без отдельных пунктов)

| # | Требование | DoD |
|---|---|---|
| S1 | Physics ядро (broadphase/solver/CCD/joints) | 1000 динамических тел в бюджете тика |
| S2 | KCC полный (steps/slopes/платформы/ragdoll/сеть) | Rewind-replay сходится с сервером |
| S3 | flux-anim (FSM→sampling→blend→IK→skinning) | Персонаж ходит/бежит/прыгает, foot placement на уклонах |
| S4 | flux-ai (navmesh, A*, BT, crowd) | NPC патрулирует 2 тайла, толпа 200 через flow fields |
| S5 | flux-audio (шины, 3D, send/return, sidechain) | Диалог приглушает музыку; реверб зон |
| S6 | flux-ui (immediate, текст, HUD) | HUD с локализацией (2 языка, RTL-тест layout) |
| S7 | flux-script (Lua, песочница) | Instruction limit гасит вечный цикл; ошибка не роняет кадр |
| S8 | DX12-бэкенд | Та же сцена, golden per-бэкенд nightly |
| S9 | flux-build (DAG, кэш, пакинг) | Изменение 1 текстуры — минимальный набор узлов DAG (порог — Roadmap, G5) |
| S10 | flux-editor (viewport, инспектор, gizmo, undo, PIE, browser) | M7: уровень из префабов, PIE с ходьбой, undo через eviction |
| S11 | Environment/SimTime/weather | Время суток меняет свет; golden фиксирует SimTime |
| S12 | Saves с миграциями | Сейв v1 грузится в v2 (golden-сейвы) |

## Could (полезно, приоритет ниже)

- Golden-image полная матрица пермутаций на всех GPU nightly;
- Дельта-патчи релизных паков, распределённый кэш сборки;
- Мульти-регион loopback-топология, handoff;
- Аудио-occlusion, батчинг DSP;
- Visual scripting (этап 7+);
- WASM-моддинг (по требованию);
- Collaboration в редакторе;
- Секвенсор/катсцены.

## Won't (в v1 сознательно не делаем)

Консоли, macOS/Linux release, VR/мобилки, ray tracing, meshlets,
virtual textures, full SSS, iridescence, кроссплатформенный bit-exact
детерминизм за пределами CI-гейта (x86 linux/windows).

## Правила изменения требований

- Понижение Must → Should: только ADR с обоснованием и обновлением Roadmap.
- Новый Must: ADR + влияние на гейты и Roadmap в том же PR.
- Критерий «готово» обязан быть измеримым (тест/гейт/демо), иначе пункт
  не считается выполненным.
