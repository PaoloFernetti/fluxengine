# FluxEngine — API (реализовано)

Документ фиксирует **публичный API, который уже реализован и покрыт тестами**.
Источник истины — код; здесь — контракты, инварианты и границы.

Обозначения:

- ✅ реализовано и проверено тестами;
- 🟡 типы/каркас есть, backend или полная интеграция — позже;
- ⏳ запланировано, в коде только заглушка.

> **Правило пополнения**: после каждой завершённой итерации плана
> (`plans/stage-*.md`) этот документ обновляется тем же PR: новые типы,
> методы, feature-гейты и изменения контрактов. SPA импортирует файл
> напрямую, копий нет.

## Формат записей

Для каждой функции: сигнатура, назначение, контракт/инвариант и пример под
описанием. Примеры компилируемы по смыслу; в тестовом коде допускаются
`unwrap`/`expect` (линты разрешают их только в тестах).

## Статус

| Этап | Итерация | Что вошло в API |
|---|---|---|
| 1 | — | `flux-math`, `flux-core`, каркас `flux-engine` |
| 2 | I0–I5 | `flux-ecs`, `flux-macros::Reflect`, schema/save |
| 3 | I0 | `flux-platform` (контракты), `flux-physics` (collision/KCC) |
| 3 | I1 | input: чорды, property-контракты, `Intention` |
| 3 | I2 | `flux-engine::simulation`: ECS-системы контроллера, step-up |
| 3 | I3 | `flux-engine::determinism`: харнесс, хеш/дифф, CI-шаг |
| 3 | I4 | `winit`/`gilrs` backend-ы, `Engine::overlay`, M3-пример |
| 4 | I0 | `flux-graphics::rhi`: контракты, capabilities, headless-null backend |
| 4 | I1 | `flux-graphics::rhi::vulkan` (feature `vulkan`): device/queues, память, offscreen swapchain, validation |
| 4 | I2 | `flux-graphics::render`: `TransferSystem`, `ResourceRegistry`, `VramBudget` |
| 4 | I3 | `flux-graphics::render::graph`: `RenderGraph`, автоматические барьеры, aliasing, resize |
| 4 | I4 | `flux-graphics::render::scene`: `Camera`, `RenderScene`; `flux-engine::render_extract`: ECS→сцена, alpha, camera-relative |
| 4 | I5 | `flux-graphics::rhi`: SPIR-V `PipelineDesc`, `PrimitiveTopology`, реальные graphics/compute PSO; `render::pbr/gbuffer/tonemap/post`: PBR-математика, G-buffer ABI, тон-маппинг, bloom/TAA/SSAO |
| 4 | I6 | `flux-graphics::shaders`: `shader.json`, keyword-маски, off-demand fallback, кэш байткод/PSO/материал |
| 4 | I7 | `flux-graphics::settings`: `HardwareTier`, `QualityPreset`, `GraphicsSettings`, `SettingsChange`, `GraphicsFeatureRegistry` |
| 4 | I8 | `flux-graphics::golden`: PSNR/SSIM, per-runner baselines, heatmap; `flux-graphics::profile`: `GpuZone`, `GpuProfiler` |
| 4 | I6b | `tools/flux-assetc shader` (DXC → SPIR-V); RHI query pools (`QueryPoolDesc`, `write_timestamp`, `read_query_pool`); G4-гейт |
| 4 | I9 | RHI для GPU-проходов: дескрипторные наборы (`BindingKind`, `DescriptorSetDesc`, `DescriptorWrite`), `bind_descriptor_set`, `copy_buffer_to_image`/`copy_image_to_buffer`, `write_buffer`/`read_buffer` (ADR-0039) |
| 4 | I9b | поверхности (`VkSurfaceKHR`) и present-swapchain (`VkSwapchainKHR`): `SurfaceTarget`/`WindowSurface`, headless WSI, `Swapchain::image_count` |
| — | release | `tools/flux-release`: Conventional Commits → semver, версия `Cargo.toml`, `CHANGELOG.md`, commit/annotated tag; pre-release, монорепо fixed/independent, opt-in publish/gh/semver-checks (ADR-0040) |

## Общие конвенции

- Симуляция — `f64`; `WorldPos` — `i64` нанометры, конверсии только на границах.
- Порядок, влияющий на результат, — детерминированный: UUID, имена систем,
  индексы архетипов. `HashMap` не участвует в порядке.
- Публичный API не паникует на ожидаемых отказах: `Result`/`Option`.
- Raw-события ввода не входят в тик: единственный вход симуляции — `Intention`.
- `flux-ecs`, `flux-physics`, `flux-core`, `flux-math` не зависят от
  `flux-platform` (проверяется `tools/ci/check-headless-deps.mjs`).

---

## flux-math

Крейт математики: свой API поверх glam, три пространства координат.

Импорт:

```rust
use flux_math::{
    sim_f32_to_world, Aabb, Bvh, DVec3, Frustum, Mat4, Obb, PositionError, Quat, SimRegion,
    Sphere, Vec3, WorldPos, DEFAULT_MAX_SIM_RADIUS_M, NANOMETERS_PER_METER,
};
```

### Типы ✅

`Vec2`, `Vec3`, `Vec3A`, `Vec4` — `f32`-векторы; `Vec3A` имеет align 16 и
предназначен для SoA/SIMD. `DVec2`, `DVec3`, `DVec4` — `f64`-векторы для
физики и канона. `Mat3`, `Mat4`, `DMat4`, `Quat`, `DQuat` — матрицы и
кватернионы. `SIM_EPSILON`, `CANON_EPSILON_NM` — допуски сравнения.

```rust
let v = Vec3::new(1.0, 2.0, 3.0);
let d = DVec3::new(1.0, 2.0, 3.0);
assert!((v.length() - 3.741_657_4).abs() < 1e-5);
assert_eq!(d, DVec3::ONE * 1.0 + DVec3::new(0.0, 1.0, 2.0));
```

### `NANOMETERS_PER_METER`

Нанометров в метре: `1_000_000_000`. Базовая единица канонической позиции.

```rust
assert_eq!(NANOMETERS_PER_METER, 1_000_000_000);
assert_eq!(1.5_f64 * NANOMETERS_PER_METER as f64, 1_500_000_000.0);
```

### `WorldPos::ORIGIN`

Начало координат `(0, 0, 0)` в нанометрах. `const`, пригоден для константных
выражений и `Default`.

```rust
let origin = WorldPos::ORIGIN;
assert_eq!(origin, WorldPos::default());
assert_eq!(origin.to_meters(), DVec3::ZERO);
```

### `WorldPos::from_nanometers(x, y, z) -> Self`

Создаёт позицию напрямую из нанометров. `const`-функция; не проверяет
диапазон, потому что `i64` уже является границей канона.

```rust
let pos = WorldPos::from_nanometers(1_000_000_000, 0, -500_000_000);
assert_eq!(pos.x, NANOMETERS_PER_METER);
assert_eq!(pos.z, -500_000_000);
```

### `WorldPos::from_meters(x, y, z) -> Result<Self, PositionError>`

Конвертирует метры в нанометры с округлением к ближайшему и проверкой
диапазона `i64`. Не-конечное значение — `PositionError::NotFinite`,
выход за диапазон — `PositionError::OutOfRange`.

```rust
let pos = WorldPos::from_meters(1.5, 0.0, -2.0)?;
assert_eq!(pos.x, 1_500_000_000);

assert_eq!(WorldPos::from_meters(f64::NAN, 0.0, 0.0), Err(PositionError::NotFinite));
assert!(WorldPos::from_meters(1.0e30, 0.0, 0.0).is_err());
# Ok::<(), PositionError>(())
```

### `WorldPos::saturating_from_meters(x, y, z) -> Self`

Как [`from_meters`](#worldposfrom_metersx-y-z---resultself-positionerror), но
насыщает на границах `i64` и заменяет `NaN` на `0`. Для рендера/интерполяции,
где отказ недопустим.

```rust
let pos = WorldPos::saturating_from_meters(1.0e30, f64::NAN, -1.0e30);
assert_eq!(pos.x, i64::MAX);
assert_eq!(pos.y, 0);
assert_eq!(pos.z, i64::MIN);
```

### `WorldPos::to_meters(self) -> DVec3`

Обратная конверсия в метры (`f64`). Точность ограничена `f64`, поэтому
использовать только на границе симуляции.

```rust
let pos = WorldPos::from_nanometers(1_500_000_000, 0, 0);
assert_eq!(pos.to_meters(), DVec3::new(1.5, 0.0, 0.0));
```

### `WorldPos::delta_meters(self, other) -> DVec3`

Вектор от `self` к `other` в метрах. Считается в `i64` до деления, поэтому
не теряет точность на больших координатах.

```rust
let a = WorldPos::from_meters(0.0, 0.0, 0.0)?;
let b = WorldPos::from_meters(2.0, 3.0, 0.0)?;
assert_eq!(a.delta_meters(b), DVec3::new(2.0, 3.0, 0.0));
# Ok::<(), PositionError>(())
```

### `WorldPos::checked_add(self, other) -> Option<Self>`

Сложение с проверкой переполнения по каждой оси. `None` — переполнение хотя
бы одной координаты.

```rust
let a = WorldPos::from_nanometers(i64::MAX, 0, 0);
let b = WorldPos::from_nanometers(1, 0, 0);
assert_eq!(a.checked_add(b), None);
assert!(WorldPos::ORIGIN.checked_add(WorldPos::from_nanometers(1, 2, 3)).is_some());
```

### `WorldPos::to_sim_f32(self, origin) -> Vec3`

Переводит канон в sim-пространство `f32` относительно `origin`. Дельта
считается точно, затем приводится к `f32`.

```rust
let origin = WorldPos::from_meters(100.0, 0.0, 0.0)?;
let pos = WorldPos::from_meters(101.5, 0.0, 0.0)?;
assert_eq!(pos.to_sim_f32(origin), Vec3::new(1.5, 0.0, 0.0));
# Ok::<(), PositionError>(())
```

### `sim_f32_to_world(origin, sim) -> WorldPos`

Свободная функция: sim-координата `f32` (метры) → канон относительно
`origin`. На переполнении насыщает, а не паникует.

```rust
let origin = WorldPos::from_meters(10.0, 0.0, 0.0)?;
let world = sim_f32_to_world(origin, Vec3::new(1.0, 2.0, 0.0));
assert_eq!(world.to_meters(), DVec3::new(11.0, 2.0, 0.0));
# Ok::<(), PositionError>(())
```

### `PositionError`

Ошибка конверсии координаты: `NotFinite` (не конечное число) и
`OutOfRange(String)` (не помещается в `i64` нанометров).

```rust
let error = WorldPos::from_meters(f64::INFINITY, 0.0, 0.0).unwrap_err();
assert_eq!(error, PositionError::NotFinite);
```

### `SimRegion::new(origin) -> Self`

Создаёт sim-регион с заданным началом. `const`; сам регион — просто
`WorldPos`, поэтому дешёв в копировании.

```rust
let region = SimRegion::new(WorldPos::ORIGIN);
assert_eq!(region.origin(), WorldPos::ORIGIN);
```

### `SimRegion::origin(&self) -> WorldPos`

Текущее начало sim-пространства в каноне.

```rust
let region = SimRegion::new(WorldPos::from_nanometers(5, 0, 0));
assert_eq!(region.origin().x, 5);
```

### `SimRegion::rebase(&mut self, origin) -> WorldPos`

Переносит начало и возвращает прежнее значение. Вызывается только на
границе тиков; ECS-компоненты пересчитываются отдельно.

```rust
let mut region = SimRegion::new(WorldPos::ORIGIN);
let previous = region.rebase(WorldPos::from_nanometers(1_000, 0, 0));
assert_eq!(previous, WorldPos::ORIGIN);
assert_eq!(region.origin().x, 1_000);
```

### `SimRegion::to_sim(&self, world) -> Vec3`

Канон → sim (`f32`, метры относительно начала). Делегирует
[`WorldPos::to_sim_f32`](#worldposto_sim_f32self-origin---vec3).

```rust
let region = SimRegion::new(WorldPos::from_meters(10.0, 0.0, 0.0)?);
let sim = region.to_sim(WorldPos::from_meters(12.0, 0.0, 0.0)?);
assert_eq!(sim, Vec3::new(2.0, 0.0, 0.0));
# Ok::<(), PositionError>(())
```

### `SimRegion::to_world(&self, sim) -> WorldPos`

Sim → канон. На переполнении насыщает; результат всегда валиден.

```rust
let region = SimRegion::new(WorldPos::from_meters(10.0, 0.0, 0.0)?);
let world = region.to_world(Vec3::new(-2.0, 1.0, 0.0));
assert_eq!(world.to_meters(), DVec3::new(8.0, 1.0, 0.0));
# Ok::<(), PositionError>(())
```

### `SimRegion::needs_rebase(&self, world, radius_m) -> bool`

`true`, если позиция дальше `radius_m` от начала. Порог по умолчанию —
`DEFAULT_MAX_SIM_RADIUS_M = 8192.0` м.

```rust
let region = SimRegion::new(WorldPos::ORIGIN);
let far = WorldPos::from_meters(DEFAULT_MAX_SIM_RADIUS_M + 1.0, 0.0, 0.0)?;
assert!(region.needs_rebase(far, DEFAULT_MAX_SIM_RADIUS_M));
# Ok::<(), PositionError>(())
```

### `SimRegion::distance_m(&self, world) -> f64`

Расстояние от начала до позиции в метрах (`f64`). Используется для
диагностики и решений о rebase.

```rust
let region = SimRegion::new(WorldPos::ORIGIN);
let point = WorldPos::from_meters(3.0, 4.0, 0.0)?;
assert_eq!(region.distance_m(point), 5.0);
# Ok::<(), PositionError>(())
```

### `Aabb::new(a, b) -> Self`

Осевой bounding box; углы нормализуются (`min`/`max`), порядок аргументов
не важен.

```rust
let aabb = Aabb::new(Vec3::splat(2.0), Vec3::splat(1.0));
assert_eq!(aabb.min, Vec3::splat(1.0));
assert_eq!(aabb.max, Vec3::splat(2.0));
```

### `Aabb::from_center_half_extents(center, half_extents) -> Self`

Строит AABB по центру и половине размеров.

```rust
let aabb = Aabb::from_center_half_extents(Vec3::ZERO, Vec3::splat(1.0));
assert_eq!(aabb.min, -Vec3::ONE);
assert_eq!(aabb.max, Vec3::ONE);
```

### `Aabb::center(self) -> Vec3`

Центр AABB.

```rust
let aabb = Aabb::new(Vec3::ZERO, Vec3::splat(2.0));
assert_eq!(aabb.center(), Vec3::ONE);
```

### `Aabb::half_extents(self) -> Vec3`

Половина размеров AABB по каждой оси.

```rust
let aabb = Aabb::new(Vec3::ZERO, Vec3::splat(2.0));
assert_eq!(aabb.half_extents(), Vec3::ONE);
```

### `Aabb::contains(self, point) -> bool`

Содержит ли точку, включая границы.

```rust
let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
assert!(aabb.contains(Vec3::splat(0.5)));
assert!(!aabb.contains(Vec3::splat(1.5)));
```

### `Aabb::intersects(self, other) -> bool`

Пересекаются ли два AABB (касание считается пересечением).

```rust
let a = Aabb::new(Vec3::ZERO, Vec3::ONE);
let b = Aabb::new(Vec3::splat(0.5), Vec3::splat(2.0));
assert!(a.intersects(b));
```

### `Aabb::union(self, other) -> Self`

Наименьший AABB, содержащий оба.

```rust
let a = Aabb::new(Vec3::ZERO, Vec3::ONE);
let b = Aabb::new(Vec3::splat(3.0), Vec3::splat(4.0));
assert_eq!(a.union(b).max, Vec3::splat(4.0));
```

### `Aabb::expand(&mut self, amount) -> &mut Self`

Расширяет AABB на `amount` по всем осям. Возвращает `&mut self` для цепочки.

```rust
let mut aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);
aabb.expand(0.5);
assert_eq!(aabb.min, Vec3::splat(-0.5));
```

### `Aabb::is_empty(self) -> bool`

`true`, если AABB вырожден (`min > max` хотя бы по одной оси).

```rust
let empty = Aabb { min: Vec3::ONE, max: Vec3::ZERO };
assert!(empty.is_empty());
```

### `Sphere::intersects_sphere(self, other) -> bool`

Пересечение двух сфер по сумме радиусов.

```rust
let a = Sphere { center: Vec3::ZERO, radius: 1.0 };
let b = Sphere { center: Vec3::new(1.5, 0.0, 0.0), radius: 1.0 };
assert!(a.intersects_sphere(b));
```

### `Sphere::intersects_aabb(self, aabb) -> bool`

Пересечение сферы с AABB через ближайшую точку.

```rust
let sphere = Sphere { center: Vec3::ZERO, radius: 1.0 };
let aabb = Aabb::new(Vec3::splat(0.5), Vec3::splat(2.0));
assert!(sphere.intersects_aabb(aabb));
```

### `Obb::contains(self, point) -> bool`

Содержит ли точка ориентированный box: точка переводится в локальные оси
обратным поворотом.

```rust
let obb = Obb {
    center: Vec3::ZERO,
    half_extents: Vec3::splat(1.0),
    rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_4),
};
assert!(obb.contains(Vec3::new(0.5, 0.0, 0.0)));
```

### `Obb::aabb(self) -> Aabb`

Приблизительный AABB по восьми повёрнутым углам. Используется для broadphase.

```rust
let obb = Obb {
    center: Vec3::ZERO,
    half_extents: Vec3::ONE,
    rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_4),
};
let bounds = obb.aabb();
assert!(bounds.max.x > 1.0);
```

### `Frustum::from_view_projection(matrix) -> Self`

Строит шесть плоскостей фрустума из view-projection матрицы и нормирует их.

```rust
let frustum = Frustum::from_view_projection(Mat4::IDENTITY);
assert_eq!(frustum.planes.len(), 6);
```

### `Frustum::intersects_aabb(&self, aabb) -> bool`

Консервативный тест AABB против фрустума: `false` только если AABB целиком
снаружи хотя бы одной плоскости.

```rust
let frustum = Frustum::from_view_projection(Mat4::IDENTITY);
let inside = Aabb::new(Vec3::splat(-0.5), Vec3::splat(0.5));
assert!(frustum.intersects_aabb(inside));
```

### `Bvh::build(items) -> Self` 🟡

Заготовка BVH: принимает список AABB. Сейчас возвращает пустую структуру
(реальное построение — этап 7), но контракт `is_empty` уже определён.

```rust
let bvh = Bvh::build(&[Aabb::new(Vec3::ZERO, Vec3::ONE)]);
assert!(bvh.is_empty());
```

### `Bvh::is_empty(&self) -> bool` 🟡

`true`, если в BVH нет узлов.

```rust
assert!(Bvh::default().is_empty());
```

## flux-core

Ядро: время, память, события, job system, логи, краши, конфиг, контракты,
схемные идентификаторы и `Intention`.

Импорт:

```rust
use std::time::Duration;
use flux_core::{
    config::{ConfigSection, ConfigStore},
    contracts::{AudioDevice, NetBackend, Platform, RelevanceProvider, Renderer, ScriptVM},
    crash::{breadcrumbs, Breadcrumbs, CrashHandler},
    events::EventRegistry,
    input::Intention,
    jobs::{JobHandle, JobStatus, JobSystem, MainThreadTaskQueue},
    logging::{init_logging, FrameTelemetry},
    memory::{
        reset_tracking, track_alloc, track_free, tracked, FrameArena, MemoryTag, OomPolicy,
        PoolAllocator, SlotMap,
    },
    platform::{available_parallelism, Priority},
    schema::{uuid_from_u128, ComponentLayout, FieldFlags, FieldId, SchemaRef, TypeId},
    time::{Clock, FixedTimestep, SimTick, SimTime, Stopwatch, TimeContext},
};
```

### Время ✅

#### `Clock::new() -> Self`

Создаёт монотонные часы, запоминая текущий `Instant`.

```rust
let mut clock = Clock::new();
let delta = clock.tick();
assert!(delta >= Duration::ZERO);
```

#### `Clock::tick(&mut self) -> Duration`

Возвращает дельту с прошлого вызова и накапливает `elapsed`.

```rust
let mut clock = Clock::new();
let _ = clock.tick();
std::thread::sleep(Duration::from_millis(1));
assert!(clock.tick() >= Duration::from_millis(1));
```

#### `Clock::delta(&self) -> Duration`

Последняя дельта без нового замера.

```rust
let mut clock = Clock::new();
let delta = clock.tick();
assert_eq!(clock.delta(), delta);
```

#### `Clock::elapsed(&self) -> Duration`

Суммарное время с создания часов.

```rust
let mut clock = Clock::new();
let _ = clock.tick();
assert!(clock.elapsed() >= Duration::ZERO);
```

#### `SimTick::ZERO`

Нулевой тик; `SimTick` — `Copy + Ord + Hash + Serialize`.

```rust
assert_eq!(SimTick::ZERO.get(), 0);
```

#### `SimTick::get(self) -> u64`

Значение тика.

```rust
let mut tick = SimTick::ZERO;
tick.advance(5);
assert_eq!(tick.get(), 5);
```

#### `SimTick::advance(&mut self, steps)`

Продвигает тик с насыщением на `u64::MAX`.

```rust
let mut tick = SimTick::ZERO;
tick.advance(60);
assert_eq!(tick.get(), 60);
```

#### `FixedTimestep::new(step, max_steps) -> Self`

Создаёт аккумулятор с фиксированным шагом и анти-spiral лимитом
(`max_steps.max(1)`).

```rust
let fixed = FixedTimestep::new(Duration::from_millis(16), 8);
assert_eq!(fixed.step(), Duration::from_millis(16));
```

#### `FixedTimestep::step(&self) -> Duration`

Шаг симуляции.

```rust
let fixed = FixedTimestep::new(Duration::from_millis(10), 4);
assert_eq!(fixed.step(), Duration::from_millis(10));
```

#### `FixedTimestep::ticks(&self) -> u64`

Число выполненных шагов.

```rust
let mut fixed = FixedTimestep::new(Duration::from_millis(10), 4);
fixed.advance(Duration::from_millis(25));
assert_eq!(fixed.ticks(), 2);
```

#### `FixedTimestep::dropped_steps(&self) -> u64`

Шаги, отброшенные анти-spiral ограничителем.

```rust
let mut fixed = FixedTimestep::new(Duration::from_millis(10), 2);
fixed.advance(Duration::from_millis(100));
assert!(fixed.dropped_steps() > 0);
```

#### `FixedTimestep::alpha(&self) -> f32`

Доля до следующего шага (`0.0..1.0`) для интерполяции рендера.

```rust
let mut fixed = FixedTimestep::new(Duration::from_millis(10), 4);
fixed.advance(Duration::from_millis(5));
assert!((fixed.alpha() - 0.5).abs() < 1e-6);
```

#### `FixedTimestep::advance(&mut self, delta) -> u32`

Добавляет дельту и возвращает число выполненных шагов; лишние шаги
отбрасываются и учитываются в `dropped_steps`.

```rust
let mut fixed = FixedTimestep::new(Duration::from_millis(10), 8);
assert_eq!(fixed.advance(Duration::from_millis(35)), 3);
```

#### `FixedTimestep::reset(&mut self)`

Сбрасывает аккумулятор, счётчики тиков и отброшенных шагов.

```rust
let mut fixed = FixedTimestep::default();
fixed.advance(Duration::from_millis(100));
fixed.reset();
assert_eq!(fixed.ticks(), 0);
```

#### `TimeContext::new(step, max_steps) -> Self`

Контекст времени: часы, фиксированный шаг, пауза, масштаб, `SimTick`.

```rust
let mut time = TimeContext::new(Duration::from_millis(16), 8);
let update = time.update();
assert_eq!(update.sim_tick, SimTick::ZERO);
```

#### `TimeContext::update(&mut self) -> TimeUpdate`

Обновляет время и возвращает `TimeUpdate { delta, steps, alpha, sim_tick,
sim_elapsed }`. На паузе `steps = 0`.

```rust
let mut time = TimeContext::new(Duration::from_millis(16), 8);
std::thread::sleep(Duration::from_millis(20));
let update = time.update();
assert!(update.steps >= 1);
```

#### `TimeContext::set_paused(&mut self, paused)`

Ставит/снимает паузу: шаги не выполняются, `sim_tick` не растёт.

```rust
let mut time = TimeContext::default();
time.set_paused(true);
assert_eq!(time.update().steps, 0);
```

#### `TimeContext::is_paused(&self) -> bool`

Признак паузы.

```rust
let mut time = TimeContext::default();
time.set_paused(true);
assert!(time.is_paused());
```

#### `TimeContext::set_scale(&mut self, scale)`

Масштаб времени; отрицательные значения трактуются как `0.0`.

```rust
let mut time = TimeContext::default();
time.set_scale(-1.0);
assert_eq!(time.scale(), 0.0);
```

#### `TimeContext::scale(&self) -> f32`

Текущий масштаб времени.

```rust
let mut time = TimeContext::default();
time.set_scale(0.5);
assert_eq!(time.scale(), 0.5);
```

#### `TimeContext::sim_tick(&self) -> SimTick`

Текущий тик симуляции.

```rust
let time = TimeContext::default();
assert_eq!(time.sim_tick(), SimTick::ZERO);
```

#### `TimeContext::sim_elapsed(&self) -> Duration`

Суммарное время симуляции.

```rust
let time = TimeContext::default();
assert_eq!(time.sim_elapsed(), Duration::ZERO);
```

#### `TimeContext::fixed(&self) -> &FixedTimestep`

Доступ к аккумулятору: `step`, `alpha`, `dropped_steps`.

```rust
let time = TimeContext::default();
assert_eq!(time.fixed().step(), Duration::from_nanos(16_666_667));
```

#### `SimTime::TICKS_PER_DAY`

Тиков в сутках при 60 Гц: `86_400 * 60`.

```rust
assert_eq!(SimTime::TICKS_PER_DAY, 5_184_000);
```

#### `SimTime::new() -> Self`

Время в начале суток.

```rust
let time = SimTime::new();
assert_eq!(time.ticks(), 0);
assert_eq!(time.day_fraction(), 0);
```

#### `SimTime::tick(&mut self)`

Продвигает время на один тик и пересчитывает квантованную долю суток.

```rust
let mut time = SimTime::new();
time.tick();
assert_eq!(time.ticks(), 1);
```

#### `SimTime::ticks(self) -> u64`

Номер тика.

```rust
let mut time = SimTime::new();
time.tick();
assert_eq!(time.ticks(), 1);
```

#### `SimTime::day_fraction(self) -> u16`

Доля суток `0..=65535`.

```rust
let time = SimTime::new();
assert_eq!(time.day_fraction(), 0);
```

#### `Stopwatch::new() -> Self`

Остановленный секундомер.

```rust
let watch = Stopwatch::new();
assert!(!watch.is_running());
```

#### `Stopwatch::start(&mut self)`

Запускает секундомер; повторный вызов не перезапускает.

```rust
let mut watch = Stopwatch::new();
watch.start();
assert!(watch.is_running());
```

#### `Stopwatch::stop(&mut self)`

Останавливает и накапливает прошедшее время.

```rust
let mut watch = Stopwatch::new();
watch.start();
watch.stop();
assert!(!watch.is_running());
```

#### `Stopwatch::elapsed(&self) -> Duration`

Накопленное время плюс текущий интервал, если идёт.

```rust
let mut watch = Stopwatch::new();
watch.start();
assert!(watch.elapsed() >= Duration::ZERO);
```

#### `Stopwatch::is_running(&self) -> bool`

Идёт ли сейчас.

```rust
let mut watch = Stopwatch::new();
watch.start();
assert!(watch.is_running());
```

#### `Stopwatch::reset(&mut self)`

Останавливает и обнуляет накопленное время.

```rust
let mut watch = Stopwatch::new();
watch.start();
watch.reset();
assert_eq!(watch.elapsed(), Duration::ZERO);
```

### Память ✅

#### `FrameArena::new(capacity) -> Self`

Линейный аллокатор на фиксированную ёмкость. Буфер выделяется один раз;
после прогрева `alloc`/`reset` не делают системных аллокаций.

```rust
let arena = FrameArena::new(1024);
assert_eq!(arena.capacity(), 1024);
assert_eq!(arena.used(), 0);
```

#### `FrameArena::alloc(&mut self, size, align) -> Option<&mut [u8]>`

Выделяет `size` байт с выравниванием `align`. `None` — не хватает места или
`align` не степень двойки.

```rust
let mut arena = FrameArena::new(64);
let bytes = arena.alloc(16, 8).unwrap();
assert_eq!(bytes.len(), 16);
assert!(arena.alloc(8, 3).is_none());
```

#### `FrameArena::reset(&mut self)`

Сбрасывает арену (раз в кадр); память переиспользуется.

```rust
let mut arena = FrameArena::new(64);
let _ = arena.alloc(16, 1);
arena.reset();
assert_eq!(arena.used(), 0);
```

#### `FrameArena::used(&self) -> usize`

Занято байт.

```rust
let mut arena = FrameArena::new(64);
let _ = arena.alloc(10, 1);
assert_eq!(arena.used(), 10);
```

#### `FrameArena::capacity(&self) -> usize`

Ёмкость арены.

```rust
assert_eq!(FrameArena::new(128).capacity(), 128);
```

#### `FrameArena::allocations(&self) -> u64`

Число логических выделений (диагностика, не сбрасывается `reset`).

```rust
let mut arena = FrameArena::new(64);
let _ = arena.alloc(8, 1);
assert_eq!(arena.allocations(), 1);
```

#### `FrameArena::bytes_allocated(&self) -> u64`

Суммарно выделено байт за жизнь арены (диагностика).

```rust
let mut arena = FrameArena::new(64);
let _ = arena.alloc(8, 1);
assert_eq!(arena.bytes_allocated(), 8);
```

#### `SlotMap::new() -> Self`

Пустой generational-контейнер.

```rust
let map: SlotMap<i32> = SlotMap::new();
assert!(map.is_empty());
```

#### `SlotMap::insert(&mut self, value) -> GenerationalIndex`

Вставляет значение и возвращает индекс с поколением.

```rust
let mut map = SlotMap::new();
let index = map.insert(42);
assert_eq!(map.get(index), Some(&42));
```

#### `SlotMap::remove(&mut self, index) -> Option<T>`

Удаляет значение; устаревший индекс возвращает `None`.

```rust
let mut map = SlotMap::new();
let index = map.insert(42);
assert_eq!(map.remove(index), Some(42));
assert_eq!(map.remove(index), None);
```

#### `SlotMap::get(&self, index) -> Option<&T>`

Ссылка по живому индексу.

```rust
let mut map = SlotMap::new();
let index = map.insert("x");
assert_eq!(map.get(index), Some(&"x"));
```

#### `SlotMap::get_mut(&mut self, index) -> Option<&mut T>`

Мутабельная ссылка по живому индексу.

```rust
let mut map = SlotMap::new();
let index = map.insert(1);
*map.get_mut(index).unwrap() = 2;
assert_eq!(map.get(index), Some(&2));
```

#### `SlotMap::contains(&self, index) -> bool`

Есть ли живое значение по индексу.

```rust
let mut map = SlotMap::new();
let index = map.insert(1);
assert!(map.contains(index));
```

#### `SlotMap::len(&self) -> usize`

Число живых значений.

```rust
let mut map = SlotMap::new();
let _ = map.insert(1);
assert_eq!(map.len(), 1);
```

#### `SlotMap::is_empty(&self) -> bool`

Пуст ли контейнер.

```rust
assert!(SlotMap::<u8>::new().is_empty());
```

#### `SlotMap::iter(&self) -> impl Iterator<Item = (GenerationalIndex, &T)>`

Итератор по живым значениям; удалённые слоты пропускаются.

```rust
let mut map = SlotMap::new();
let a = map.insert(1);
let _b = map.insert(2);
map.remove(a);
let values: Vec<i32> = map.iter().map(|(_, value)| *value).collect();
assert_eq!(values, vec![2]);
```

#### `GenerationalIndex::index(self) -> u32`

Индекс слота.

```rust
let mut map = SlotMap::new();
let index = map.insert(1);
assert_eq!(index.index(), 0);
```

#### `GenerationalIndex::generation(self) -> u32`

Поколение слота; после переиспользования растёт.

```rust
let mut map = SlotMap::new();
let first = map.insert(1);
map.remove(first);
let second = map.insert(2);
assert!(second.generation() > first.generation());
```

#### `PoolAllocator::with_capacity(capacity) -> Self`

Пул фиксированной ёмкости с повторным использованием слотов.

```rust
let pool: PoolAllocator<i32> = PoolAllocator::with_capacity(2);
assert_eq!(pool.capacity(), 2);
assert_eq!(pool.free(), 2);
```

#### `PoolAllocator::acquire(&mut self, value) -> Option<usize>`

Занимает слот; `None`, если пул полон.

```rust
let mut pool = PoolAllocator::with_capacity(1);
let slot = pool.acquire(10).unwrap();
assert!(pool.acquire(20).is_none());
assert_eq!(pool.get(slot), Some(&10));
```

#### `PoolAllocator::release(&mut self, index) -> Option<T>`

Освобождает слот и возвращает значение.

```rust
let mut pool = PoolAllocator::with_capacity(1);
let slot = pool.acquire(10).unwrap();
assert_eq!(pool.release(slot), Some(10));
assert_eq!(pool.free(), 1);
```

#### `PoolAllocator::get(&self, index) -> Option<&T>`

Ссылка на значение слота.

```rust
let mut pool = PoolAllocator::with_capacity(1);
let slot = pool.acquire(7).unwrap();
assert_eq!(pool.get(slot), Some(&7));
```

#### `PoolAllocator::get_mut(&mut self, index) -> Option<&mut T>`

Мутабельная ссылка на значение слота.

```rust
let mut pool = PoolAllocator::with_capacity(1);
let slot = pool.acquire(7).unwrap();
*pool.get_mut(slot).unwrap() = 8;
assert_eq!(pool.get(slot), Some(&8));
```

#### `PoolAllocator::capacity(&self) -> usize`

Ёмкость пула.

```rust
assert_eq!(PoolAllocator::<u8>::with_capacity(4).capacity(), 4);
```

#### `PoolAllocator::allocated(&self) -> usize`

Занято слотов.

```rust
let mut pool = PoolAllocator::with_capacity(2);
let _ = pool.acquire(1);
assert_eq!(pool.allocated(), 1);
```

#### `PoolAllocator::free(&self) -> usize`

Свободно слотов.

```rust
let mut pool = PoolAllocator::with_capacity(2);
let _ = pool.acquire(1);
assert_eq!(pool.free(), 1);
```

#### `track_alloc(tag, bytes)`

Учитывает выделение памяти по тегу (atomic-счётчик).

```rust
track_alloc(MemoryTag::Assets, 128);
assert!(tracked(MemoryTag::Assets) >= 128);
```

#### `track_free(tag, bytes)`

Учитывает освобождение; счётчик насыщается на нуле.

```rust
track_alloc(MemoryTag::Assets, 128);
track_free(MemoryTag::Assets, 1024);
assert_eq!(tracked(MemoryTag::Assets), 0);
```

#### `tracked(tag) -> usize`

Сколько байт учитывается по тегу.

```rust
reset_tracking();
track_alloc(MemoryTag::Rendering, 64);
assert_eq!(tracked(MemoryTag::Rendering), 64);
```

#### `reset_tracking()`

Сбрасывает все счётчики тегов.

```rust
track_alloc(MemoryTag::Other, 10);
reset_tracking();
assert_eq!(tracked(MemoryTag::Other), 0);
```

#### `MemoryTag::ALL`

Все восемь тегов в каноническом порядке: `Frame`, `Assets`, `Rendering`,
`Physics`, `Audio`, `Network`, `Editor`, `Other`.

```rust
assert_eq!(MemoryTag::ALL.len(), 8);
assert_eq!(MemoryTag::ALL[0], MemoryTag::Frame);
```

#### `OomPolicy`

Политика при исчерпании памяти: `Panic` или `ReturnNull` (по умолчанию).

```rust
assert_eq!(OomPolicy::default(), OomPolicy::ReturnNull);
```

### События ✅

#### `EventRegistry::new() -> Self`

Пустой реестр типизированных очередей.

```rust
let events = EventRegistry::new();
assert!(events.is_empty::<u32>());
```

#### `EventRegistry::writer<E>(&self) -> Writer<E>`

Создаёт/получает очередь типа `E` и возвращает отправителя.

```rust
#[derive(Clone)]
struct Hit(u32);
let events = EventRegistry::new();
let writer = events.writer::<Hit>();
writer.send(Hit(1));
```

#### `EventRegistry::reader<E>(&self) -> Reader<E>`

Создаёт читателя с собственным курсором.

```rust
#[derive(Clone)]
struct Hit(u32);
let events = EventRegistry::new();
events.writer::<Hit>().send(Hit(1));
events.update();
let mut reader = events.reader::<Hit>();
assert_eq!(reader.read().len(), 1);
```

#### `EventRegistry::update(&self)`

Публикует накопленные события (раз в кадр). `Reader` видит кадр,
`Writer` пишет в следующий.

```rust
#[derive(Clone)]
struct Tick;
let events = EventRegistry::new();
events.writer::<Tick>().send(Tick);
events.update();
assert_eq!(events.len::<Tick>(), 1);
```

#### `EventRegistry::clear_all(&self)`

Очищает все очереди.

```rust
let events = EventRegistry::new();
events.clear_all();
assert!(events.is_empty::<u8>());
```

#### `EventRegistry::len<E>(&self) -> usize`

Число опубликованных событий типа.

```rust
let events = EventRegistry::new();
assert_eq!(events.len::<u32>(), 0);
```

#### `EventRegistry::is_empty<E>(&self) -> bool`

Нет ли опубликованных событий типа.

```rust
let events = EventRegistry::new();
assert!(events.is_empty::<u32>());
```

#### `Writer::send(&self, event)`

Отправляет событие; оно станет видимым читателям после `update`.

```rust
#[derive(Clone)]
struct Ping;
let events = EventRegistry::new();
events.writer::<Ping>().send(Ping);
assert_eq!(events.len::<Ping>(), 0);
events.update();
assert_eq!(events.len::<Ping>(), 1);
```

#### `Reader::read(&mut self) -> Vec<E>`

Читает новые события с прошлого вызова (курсор хранится у читателя).

```rust
#[derive(Clone)]
struct Ping(u8);
let events = EventRegistry::new();
events.writer::<Ping>().send(Ping(1));
events.update();
let mut reader = events.reader::<Ping>();
assert_eq!(reader.read().len(), 1);
assert!(reader.read().is_empty());
```

#### `Reader::reset(&mut self)`

Сбрасывает курсор: следующий `read` вернёт всё опубликованное.

```rust
#[derive(Clone)]
struct Ping;
let events = EventRegistry::new();
events.writer::<Ping>().send(Ping);
events.update();
let mut reader = events.reader::<Ping>();
let _ = reader.read();
reader.reset();
assert_eq!(reader.read().len(), 1);
```

### Job system ✅

#### `JobSystem::new(worker_threads) -> Self`

Создаёт пул work-stealing воркеров (минимум один). Имена потоков —
`flux-job-N`.

```rust
let jobs = JobSystem::new(2);
assert_eq!(jobs.worker_count(), 2);
```

#### `JobSystem::worker_count(&self) -> usize`

Число реально запущенных воркеров.

```rust
let jobs = JobSystem::new(0);
assert!(jobs.worker_count() >= 1);
```

#### `JobSystem::spawn<F>(&self, job) -> JobHandle`

Ставит задачу в очередь. Паника внутри задачи перехватывается
(`catch_unwind`) и не убивает воркер.

```rust
let jobs = JobSystem::new(1);
let handle = jobs.spawn(|| panic!("boom"));
assert_eq!(handle.wait(), JobStatus::Panicked);
```

#### `JobHandle::status(&self) -> JobStatus`

Текущий статус без ожидания.

```rust
let jobs = JobSystem::new(1);
let handle = jobs.spawn(|| {});
assert!(matches!(handle.status(), JobStatus::Pending | JobStatus::Completed));
```

#### `JobHandle::wait(&self) -> JobStatus`

Ожидает завершения и возвращает `Completed`/`Panicked`.

```rust
let jobs = JobSystem::new(1);
let handle = jobs.spawn(|| {});
assert_eq!(handle.wait(), JobStatus::Completed);
```

#### `JobHandle::is_done(&self) -> bool`

Завершена ли задача (успешно или с паникой).

```rust
let jobs = JobSystem::new(1);
let handle = jobs.spawn(|| {});
let _ = handle.wait();
assert!(handle.is_done());
```

#### `JobStatus`

`Pending`, `Completed`, `Panicked`.

```rust
assert_ne!(JobStatus::Pending, JobStatus::Completed);
```

#### `MainThreadTaskQueue::new() -> Self`

FIFO-очередь задач, обязанных исполниться на главном потоке.

```rust
let queue = MainThreadTaskQueue::new();
assert!(queue.is_empty());
```

#### `MainThreadTaskQueue::push(&self, task)`

Добавляет задачу.

```rust
let queue = MainThreadTaskQueue::new();
queue.push(|| {});
assert_eq!(queue.len(), 1);
```

#### `MainThreadTaskQueue::run_all(&self) -> usize`

Выполняет все задачи по порядку и возвращает их число.

```rust
let queue = MainThreadTaskQueue::new();
queue.push(|| {});
queue.push(|| {});
assert_eq!(queue.run_all(), 2);
```

#### `MainThreadTaskQueue::len(&self) -> usize`

Число ожидающих задач.

```rust
let queue = MainThreadTaskQueue::new();
assert_eq!(queue.len(), 0);
```

#### `MainThreadTaskQueue::is_empty(&self) -> bool`

Пуста ли очередь.

```rust
let queue = MainThreadTaskQueue::new();
assert!(queue.is_empty());
```

### Логи и телеметрия ✅

#### `init_logging(filter) -> Result<(), LoggingError>`

Инициализирует глобальный `tracing` с `EnvFilter`. Ошибки:
`InvalidFilter`/`AlreadyInitialized`.

```rust
let _ = init_logging("info");
```

#### `FrameTelemetry::new(capacity) -> Self`

Окно телеметрии на `capacity` кадров (минимум 1).

```rust
let telemetry = FrameTelemetry::new(240);
assert_eq!(telemetry.fps(), 0.0);
```

#### `FrameTelemetry::push(&mut self, frame_ms)`

Добавляет frametime в миллисекундах, вытесняя старейший.

```rust
let mut telemetry = FrameTelemetry::new(2);
telemetry.push(16.0);
telemetry.push(16.0);
assert!((telemetry.fps() - 62.5).abs() < 0.1);
```

#### `FrameTelemetry::p50(&self) -> f32`

Медиана frametime.

```rust
let mut telemetry = FrameTelemetry::new(10);
telemetry.push(10.0);
telemetry.push(20.0);
assert!(telemetry.p50() > 0.0);
```

#### `FrameTelemetry::p99(&self) -> f32`

99-й перцентиль frametime.

```rust
let mut telemetry = FrameTelemetry::new(100);
for value in 1..=100 {
    telemetry.push(value as f32);
}
assert!(telemetry.p99() >= telemetry.p50());
```

#### `FrameTelemetry::fps(&self) -> f32`

Средний FPS по окну (`1000 / средний frametime`).

```rust
let mut telemetry = FrameTelemetry::new(2);
telemetry.push(16.0);
telemetry.push(16.0);
assert!((telemetry.fps() - 62.5).abs() < 0.1);
```

#### `verify!(condition, ...)`

Макрос проверки инварианта: логирует `error`, но не паникует.

```rust
verify!(1 + 1 == 2);
verify!(1 + 1 == 2, "math is broken");
```

### Краши ✅

#### `Breadcrumbs::new(capacity) -> Self`

Кольцо breadcrumbs на `capacity` записей.

```rust
let crumbs = Breadcrumbs::new(2);
assert!(crumbs.is_empty());
```

#### `Breadcrumbs::push(&self, message)`

Добавляет запись, вытесняя самую старую.

```rust
let crumbs = Breadcrumbs::new(2);
crumbs.push("a");
crumbs.push("b");
crumbs.push("c");
assert_eq!(crumbs.snapshot(), vec!["b", "c"]);
```

#### `Breadcrumbs::snapshot(&self) -> Vec<String>`

Снимок в порядке добавления.

```rust
let crumbs = Breadcrumbs::new(4);
crumbs.push("tick 1");
assert_eq!(crumbs.snapshot(), vec!["tick 1"]);
```

#### `Breadcrumbs::len(&self) -> usize`

Число записей.

```rust
let crumbs = Breadcrumbs::new(4);
crumbs.push("x");
assert_eq!(crumbs.len(), 1);
```

#### `Breadcrumbs::is_empty(&self) -> bool`

Пусто ли кольцо.

```rust
assert!(Breadcrumbs::new(4).is_empty());
```

#### `Breadcrumbs::clear(&self)`

Очищает кольцо.

```rust
let crumbs = Breadcrumbs::new(4);
crumbs.push("x");
crumbs.clear();
assert!(crumbs.is_empty());
```

#### `breadcrumbs() -> &'static Breadcrumbs`

Глобальное кольцо на 64 записи.

```rust
breadcrumbs().push("engine start");
assert!(!breadcrumbs().is_empty());
```

#### `CrashHandler::new(dump_dir) -> Self`

Обработчик крашей с каталогом дампов.

```rust
let handler = CrashHandler::new("crash-dumps");
assert_eq!(handler.dump_dir(), std::path::Path::new("crash-dumps"));
```

#### `CrashHandler::dump_dir(&self) -> &Path`

Каталог дампов.

```rust
let handler = CrashHandler::new("logs/dumps");
assert_eq!(handler.dump_dir(), std::path::Path::new("logs/dumps"));
```

#### `CrashHandler::write_dump(&self, message) -> io::Result<PathBuf>`

Пишет текстовый дамп с breadcrumbs; возвращает путь.

```rust
let handler = CrashHandler::new("crash-dumps");
let path = handler.write_dump("test panic").unwrap();
assert!(path.exists());
```

#### `install_panic_hook(handler)`

Ставит паник-хук: лог, breadcrumbs, дамп, затем штатный хук.

```rust
install_panic_hook(CrashHandler::new("crash-dumps"));
```

### Конфиг ✅

#### `ConfigSection::validate(&self) -> Result<(), String>`

Метод трейта с дефолтной реализацией `Ok(())`. Секция проверяет свои
инварианты.

```rust
#[derive(Default, serde::Deserialize)]
struct Volume { level: f32 }

impl ConfigSection for Volume {
    fn validate(&self) -> Result<(), String> {
        (0.0..=1.0).contains(&self.level).then_some(()).ok_or("bad level".into())
    }
}
assert!(Volume { level: 0.5 }.validate().is_ok());
```

#### `ConfigStore::new() -> Self`

Хранилище секции со значениями по умолчанию.

```rust
#[derive(Clone, Default, serde::Deserialize)]
struct Volume { level: f32 }
impl ConfigSection for Volume {}

let store: ConfigStore<Volume> = ConfigStore::new();
assert_eq!(store.get().level, 0.0);
```

#### `ConfigStore::load_ron(&self, text) -> Result<(), ConfigError>`

Парсит RON, валидирует и уведомляет подписчиков.

```rust
#[derive(Clone, Default, serde::Deserialize)]
struct Volume { level: f32 }
impl ConfigSection for Volume {}

let store: ConfigStore<Volume> = ConfigStore::new();
store.load_ron("(level: 0.5)").unwrap();
assert_eq!(store.get().level, 0.5);
```

#### `ConfigStore::get(&self) -> T`

Текущее значение (копия).

```rust
#[derive(Clone, Default, serde::Deserialize)]
struct Volume { level: f32 }
impl ConfigSection for Volume {}

let store: ConfigStore<Volume> = ConfigStore::new();
assert_eq!(store.get().level, 0.0);
```

#### `ConfigStore::set(&self, value) -> Result<(), ConfigError>`

Валидирует и заменяет значение, уведомляя подписчиков.

```rust
#[derive(Clone, Default, serde::Deserialize)]
struct Volume { level: f32 }
impl ConfigSection for Volume {}

let store: ConfigStore<Volume> = ConfigStore::new();
store.set(Volume { level: 1.0 }).unwrap();
assert_eq!(store.get().level, 1.0);
```

#### `ConfigStore::on_change(&self, listener)`

Подписка на изменения; слушатель получает `&T`.

```rust
#[derive(Clone, Default, serde::Deserialize)]
struct Volume { level: f32 }
impl ConfigSection for Volume {}

let store: ConfigStore<Volume> = ConfigStore::new();
store.on_change(|value| println!("volume = {}", value.level));
store.set(Volume { level: 0.7 }).unwrap();
```

### Платформенные сервисы ядра ✅

#### `available_parallelism() -> usize`

Доступный параллелизм машины, минимум 1.

```rust
assert!(available_parallelism() >= 1);
```

#### `Priority`

Приоритет потока: `Low`, `Normal`, `High`, `Critical`.

```rust
assert_ne!(Priority::Low, Priority::Critical);
```

#### `set_current_thread_priority(priority) -> Result<(), PlatformError>`

Запрашивает приоритет текущего потока. MVP фиксирует намерение (no-op) и
возвращает `Ok`.

```rust
set_current_thread_priority(Priority::High).unwrap();
```

### Контракты подсистем ✅

#### `Platform::name(&self) -> &'static str`

Имя платформы для логов/диагностики.

```rust
struct Headless;
impl Platform for Headless {
    fn name(&self) -> &'static str { "headless" }
}
assert_eq!(Headless.name(), "headless");
```

#### `Platform::worker_threads(&self) -> usize`

Число фоновых воркеров (по умолчанию `available_parallelism - 1`).

```rust
struct Headless;
impl Platform for Headless {
    fn name(&self) -> &'static str { "headless" }
}
assert_eq!(Headless.worker_threads(), available_parallelism().saturating_sub(1));
```

#### `Renderer::backend_name(&self) -> &'static str`

Имя рендер-бэкенда (`vulkan`, `dx12`).

```rust
struct NullRenderer;
impl Renderer for NullRenderer {
    fn backend_name(&self) -> &'static str { "null" }
}
assert_eq!(NullRenderer.backend_name(), "null");
```

#### `AudioDevice::sample_rate(&self) -> u32`

Частота дискретизации аудио-устройства.

```rust
struct NullAudio;
impl AudioDevice for NullAudio {
    fn sample_rate(&self) -> u32 { 48_000 }
}
assert_eq!(NullAudio.sample_rate(), 48_000);
```

#### `NetBackend::name(&self) -> &'static str`

Имя сетевого транспорта (`loopback`, `udp`).

```rust
struct Loopback;
impl NetBackend for Loopback {
    fn name(&self) -> &'static str { "loopback" }
}
assert_eq!(Loopback.name(), "loopback");
```

#### `ScriptVM::name(&self) -> &'static str`

Имя скриптовой VM.

```rust
struct Lua;
impl ScriptVM for Lua {
    fn name(&self) -> &'static str { "lua" }
}
assert_eq!(Lua.name(), "lua");
```

#### `RelevanceProvider::relevance_tier(&self, entity) -> u8`

Ярус релевантности сущности (`0..=3`); контракт разрывает цикл scene ↔ net.

```rust
struct AllRelevant;
impl RelevanceProvider for AllRelevant {
    fn relevance_tier(&self, _entity: u64) -> u8 { 0 }
}
assert_eq!(AllRelevant.relevance_tier(42), 0);
```

### Схемные идентификаторы ✅

#### `uuid_from_u128(value) -> Uuid`

`const`-конструктор UUID для сгенерированного кода дерайвов.

```rust
let uuid = uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0001);
assert_eq!(uuid.as_u128(), 0x0000_0000_0000_4000_8000_0000_0000_0001);
```

#### `TypeId::new_v4() -> Self`

Новый случайный идентификатор типа.

```rust
let id = TypeId::new_v4();
assert_ne!(id, TypeId::new_v4());
```

#### `TypeId::from_uuid(uuid) -> Self`

`const`-конструктор из существующего UUID.

```rust
let uuid = uuid_from_u128(1);
assert_eq!(TypeId::from_uuid(uuid).0, uuid);
```

#### `FieldId::new() -> Self`

Новый случайный идентификатор поля.

```rust
assert_ne!(FieldId::new(), FieldId::new());
```

#### `SchemaRef::new(type_uuid, schema_rev) -> Self`

Ссылка на ревизию схемы типа.

```rust
let schema = SchemaRef::new(TypeId::new_v4(), 1);
assert_eq!(schema.schema_rev, 1);
```

#### `SchemaRef::bumped(self) -> Self`

Следующая ревизия того же типа.

```rust
let schema = SchemaRef::new(TypeId::new_v4(), 1).bumped();
assert_eq!(schema.schema_rev, 2);
```

#### `FieldFlags::transient() -> Self`

Поле не сохраняется и не реплицируется.

```rust
let flags = FieldFlags::transient();
assert!(!flags.save && !flags.replicated);
```

#### `FieldFlags::saved() -> Self`

Поле сохраняется, но не реплицируется.

```rust
let flags = FieldFlags::saved();
assert!(flags.save && !flags.replicated);
```

#### `FieldFlags::replicated() -> Self`

Поле реплицируется и по умолчанию сохраняется.

```rust
let flags = FieldFlags::replicated();
assert!(flags.save && flags.replicated);
```

#### `ComponentLayout::new(schema) -> Self`

Схема компонента как данные: ревизия плюс список полей.

```rust
let schema = SchemaRef::new(TypeId::new_v4(), 1);
let layout = ComponentLayout::new(schema);
assert!(layout.fields.is_empty());
```

#### `ComponentLayout::with_field(self, id, flags) -> Self`

Добавляет поле с разметкой.

```rust
let schema = SchemaRef::new(TypeId::new_v4(), 1);
let layout = ComponentLayout::new(schema)
    .with_field(FieldId::new(), FieldFlags::saved());
assert_eq!(layout.fields.len(), 1);
```

#### `ComponentLayout::save_fields(&self) -> impl Iterator<Item = FieldId>`

Поля, попадающие в сейв.

```rust
let schema = SchemaRef::new(TypeId::new_v4(), 1);
let layout = ComponentLayout::new(schema)
    .with_field(FieldId::new(), FieldFlags::saved());
assert_eq!(layout.save_fields().count(), 1);
```

#### `ComponentLayout::replicated_fields(&self) -> impl Iterator<Item = FieldId>`

Поля, попадающие в wire-схему.

```rust
let schema = SchemaRef::new(TypeId::new_v4(), 1);
let layout = ComponentLayout::new(schema)
    .with_field(FieldId::new(), FieldFlags::replicated());
assert_eq!(layout.replicated_fields().count(), 1);
```

#### `ComponentLayout::same_revision(&self, other) -> bool`

Совместимы ли схемы по `SchemaRef`.

```rust
let schema = SchemaRef::new(TypeId::new_v4(), 1);
let a = ComponentLayout::new(schema);
let b = ComponentLayout::new(schema);
assert!(a.same_revision(&b));
```

### Ввод: Intention ✅

#### `Intention::IDLE`

Нейтральное намерение: нули, без прыжка и приседа.

```rust
assert!(Intention::IDLE.is_idle());
```

#### `Intention::clamped(self) -> Self`

Ограничивает оси диапазоном `-1..1` (защита от ботов и сети).

```rust
let intention = Intention {
    move_axis: [2.0, -3.0],
    look: [0.0, 5.0],
    jump: false,
    crouch: false,
};
let clamped = intention.clamped();
assert_eq!(clamped.move_axis, [1.0, -1.0]);
assert_eq!(clamped.look, [0.0, 1.0]);
```

#### `Intention::is_idle(self) -> bool`

Нет ли активных действий.

```rust
assert!(Intention::default().is_idle());
assert!(!Intention { jump: true, ..Intention::IDLE }.is_idle());
```

#### `Intention::move_length(self) -> f64`

Длина вектора движения (для анимации/скорости).

```rust
let intention = Intention { move_axis: [3.0, 4.0], ..Intention::IDLE };
assert_eq!(intention.move_length(), 5.0);
```

## flux-ecs

Архетипная ECS: SoA-чанки, change detection, планировщик, schema/save.

Все примеры ниже используют два компонента:

```rust
use flux_core::schema::{uuid_from_u128, TypeId};
use flux_core::time::SimTick;
use flux_ecs::{
    archetypes::{
        ArchetypeId, ArchetypeLayout, ComponentIndex, ComponentRegistry, Signature, MAX_COMPONENTS,
        TARGET_CHUNK_BYTES,
    },
    change::{ComponentTick, LastRun},
    commands::{CommandTarget, Commands, CommandsReport},
    component::{Column, ColumnVec, Component},
    entity::{Entity, EntityAllocator, EntityUuid, Location},
    query::{QueryCache, QueryKey, QuerySpec},
    resources::Resources,
    save::{ComponentDelta, FieldValue, JournalEntry, OrphanedField, SaveJournal, Value},
    scheduler::{
        Access, Conflict, ConflictKind, Schedule, ScheduleError, ScheduleReport, Stage,
        StageReport, SystemDescriptor, TickRate,
    },
    schema::{
        ComponentSchema, EditorMeta, FieldSchema, FieldType, OrphanPolicy, SchemaError,
        SchemaRegistry,
    },
    sim_time::{SimTimeComponent, TICKS_PER_DAY},
    world::{EcsError, World},
};

#[derive(Clone, Debug, Default, PartialEq)]
struct Position { x: f32, y: f32 }

impl Component for Position {
    const TYPE_ID: TypeId = TypeId::from_uuid(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0100));
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Velocity { x: f32, y: f32 }

impl Component for Velocity {
    const TYPE_ID: TypeId = TypeId::from_uuid(uuid_from_u128(0x0000_0000_0000_4000_8000_0000_0000_0101));
}
```

### Компоненты ✅

#### `Component::TYPE_ID`

Стабильный UUID типа компонента. Задаётся дерайвом `Reflect` или вручную;
участвует в `Signature` и в схемах сейвов.

```rust
assert_ne!(Position::TYPE_ID, Velocity::TYPE_ID);
```

#### `Component::schema() -> ComponentSchema`

Схема компонента для инспектора и сериализации. Дефолтная реализация
объявляет тип без полей; `#[derive(Reflect)]` переопределяет её.

```rust
let schema = Position::schema();
assert_eq!(schema.schema.type_uuid, Position::TYPE_ID);
```

#### `Column::len(&self) -> usize`

Число значений в type-erased столбце.

```rust
let column = ColumnVec::<Position>::new();
assert_eq!(Column::len(&column), 0);
```

#### `Column::is_empty(&self) -> bool`

Пуст ли столбец.

```rust
let column = ColumnVec::<Position>::new();
assert!(Column::is_empty(&column));
```

#### `Column::reserve(&mut self, additional)`

Резервирует место под `additional` значений.

```rust
let mut column = ColumnVec::<Position>::new();
Column::reserve(&mut column, 8);
assert_eq!(Column::len(&column), 0);
```

#### `Column::swap_remove(&mut self, index)`

Удаляет значение, перемещая последнее на его место (порядок не сохраняется).

```rust
let mut column = ColumnVec::<Position>::new();
column.push(Position { x: 1.0, y: 0.0 });
column.push(Position { x: 2.0, y: 0.0 });
Column::swap_remove(&mut column, 0);
assert_eq!(Column::len(&column), 1);
```

#### `Column::clear(&mut self)`

Очищает столбец.

```rust
let mut column = ColumnVec::<Position>::new();
column.push(Position::default());
Column::clear(&mut column);
assert!(Column::is_empty(&column));
```

#### `Column::take(&mut self, index) -> Box<dyn Any>`

Забирает значение как `Any` (используется миграцией `remove`).

```rust
let mut column = ColumnVec::<Position>::new();
column.push(Position { x: 1.0, y: 2.0 });
let taken = Column::take(&mut column, 0);
assert!(taken.downcast::<Position>().is_ok());
```

#### `Column::push_default(&mut self)`

Добавляет `T::default()` (миграция `add` компонента).

```rust
let mut column = ColumnVec::<Position>::new();
Column::push_default(&mut column);
assert_eq!(Column::len(&column), 1);
```

#### `Column::push_from_erased(&mut self, source, index)`

Перемещает значение из другого столбца того же типа.

```rust
let mut source = ColumnVec::<Position>::new();
source.push(Position { x: 5.0, y: 0.0 });
let mut target = ColumnVec::<Position>::new();
Column::push_from_erased(&mut target, &mut source, 0);
assert_eq!(Column::len(&target), 1);
assert_eq!(Column::len(&source), 0);
```

#### `Column::as_any(&self) -> &dyn Any`

Downcast только для чтения (один на чанк, не на элемент).

```rust
let column = ColumnVec::<Position>::new();
assert!(Column::as_any(&column).is::<ColumnVec<Position>>());
```

#### `Column::as_any_mut(&mut self) -> &mut dyn Any`

Downcast с изменением.

```rust
let mut column = ColumnVec::<Position>::new();
assert!(Column::as_any_mut(&mut column).is::<ColumnVec<Position>>());
```

#### `ColumnVec::new() -> Self`

Пустой плотный столбец.

```rust
let column = ColumnVec::<Position>::new();
assert!(column.as_slice().is_empty());
```

#### `ColumnVec::with_capacity(capacity) -> Self`

Столбец с резервом.

```rust
let column = ColumnVec::<Position>::with_capacity(16);
assert_eq!(column.as_slice().len(), 0);
```

#### `ColumnVec::as_slice(&self) -> &[T]`

Плотный срез значений для итерации запроса.

```rust
let mut column = ColumnVec::<Position>::new();
column.push(Position::default());
assert_eq!(column.as_slice().len(), 1);
```

#### `ColumnVec::as_mut_slice(&mut self) -> &mut [T]`

Мутабельный плотный срез.

```rust
let mut column = ColumnVec::<Position>::new();
column.push(Position::default());
column.as_mut_slice()[0].x = 1.0;
assert_eq!(column.get(0).unwrap().x, 1.0);
```

#### `ColumnVec::get(&self, index) -> Option<&T>`

Значение по индексу.

```rust
let mut column = ColumnVec::<Position>::new();
column.push(Position { x: 1.0, y: 2.0 });
assert_eq!(column.get(0).unwrap().x, 1.0);
```

#### `ColumnVec::get_mut(&mut self, index) -> Option<&mut T>`

Мутабельное значение по индексу.

```rust
let mut column = ColumnVec::<Position>::new();
column.push(Position::default());
column.get_mut(0).unwrap().y = 3.0;
assert_eq!(column.get(0).unwrap().y, 3.0);
```

#### `ColumnVec::push(&mut self, value)`

Добавляет значение.

```rust
let mut column = ColumnVec::<Position>::new();
column.push(Position::default());
assert_eq!(column.as_slice().len(), 1);
```

### Сущности ✅

#### `Entity::index(self) -> u32`

Индекс слота сущности.

```rust
let mut world = World::new();
let entity = world.spawn();
assert_eq!(entity.index(), 0);
```

#### `Entity::generation(self) -> u32`

Поколение слота; после переиспользования растёт и инвалидирует старые
хэндлы.

```rust
let mut world = World::new();
let entity = world.spawn();
assert_eq!(entity.generation(), 0);
```

#### `EntityUuid::new() -> Self`

Новый случайный UUID сущности для сейвов/сети.

```rust
let a = EntityUuid::new();
let b = EntityUuid::new();
assert_ne!(a, b);
```

#### `Location`

Позиция сущности в хранилище: `{ archetype: ArchetypeId, chunk: u32, slot: u32 }`.

```rust
let mut world = World::new();
let entity = world.spawn();
assert_eq!(world.archetype_count(), 1);
assert_eq!(entity.index(), 0);
```

#### `EntityAllocator::new() -> Self`

Пустой аллокатор слотов.

```rust
let allocator = EntityAllocator::new();
assert!(allocator.is_empty());
```

#### `EntityAllocator::allocate(&mut self, uuid) -> Entity`

Выделяет слот; при переиспользовании поколение инкрементируется. Слот с
исчерпанным `u32::MAX` выводится из оборота навсегда.

```rust
let mut allocator = EntityAllocator::new();
let entity = allocator.allocate(EntityUuid::new());
assert_eq!(entity.index(), 0);
```

#### `EntityAllocator::free(&mut self, entity) -> bool`

Освобождает слот. `false` — сущность уже мертва.

```rust
let mut allocator = EntityAllocator::new();
let entity = allocator.allocate(EntityUuid::new());
assert!(allocator.free(entity));
assert!(!allocator.free(entity));
```

#### `EntityAllocator::is_alive(&self, entity) -> bool`

Жива ли сущность (совпадают индекс и поколение).

```rust
let mut allocator = EntityAllocator::new();
let entity = allocator.allocate(EntityUuid::new());
assert!(allocator.is_alive(entity));
```

#### `EntityAllocator::location(&self, entity) -> Option<Location>`

Позиция сущности в архетипе/чанке/слоте.

```rust
let mut allocator = EntityAllocator::new();
let entity = allocator.allocate(EntityUuid::new());
assert!(allocator.location(entity).is_none());
```

#### `EntityAllocator::set_location(&mut self, entity, location)`

Записывает позицию сущности (внутренний контракт миграций).

```rust
let mut allocator = EntityAllocator::new();
let entity = allocator.allocate(EntityUuid::new());
allocator.set_location(entity, None);
assert!(allocator.location(entity).is_none());
```

#### `EntityAllocator::uuid(&self, entity) -> Option<EntityUuid>`

UUID сущности.

```rust
let mut allocator = EntityAllocator::new();
let uuid = EntityUuid::new();
let entity = allocator.allocate(uuid);
assert_eq!(allocator.uuid(entity), Some(uuid));
```

#### `EntityAllocator::len(&self) -> usize`

Число живых сущностей.

```rust
let mut allocator = EntityAllocator::new();
let _ = allocator.allocate(EntityUuid::new());
assert_eq!(allocator.len(), 1);
```

#### `EntityAllocator::is_empty(&self) -> bool`

Нет ли живых сущностей.

```rust
assert!(EntityAllocator::new().is_empty());
```

#### `EntityAllocator::capacity(&self) -> usize`

Число выделенных слотов, включая свободные.

```rust
let mut allocator = EntityAllocator::new();
let _ = allocator.allocate(EntityUuid::new());
assert_eq!(allocator.capacity(), 1);
```

### Архетипы и чанки ✅

#### `TARGET_CHUNK_BYTES`

Целевой размер чанка: `16 * 1024` байт (подбирается бенчем, ADR-0029).

```rust
assert_eq!(TARGET_CHUNK_BYTES, 16 * 1024);
```

#### `MAX_COMPONENTS`

Максимум компонентов в `Signature`: 256 (битсет 4×`u64`).

```rust
assert_eq!(MAX_COMPONENTS, 256);
```

#### `ComponentIndex`

Индекс компонента в `Signature` и layout архетипа (`ComponentIndex(pub u16)`).

```rust
let mut registry = ComponentRegistry::new();
let index = registry.register::<Position>();
assert_eq!(index.0, 0);
```

#### `Signature::EMPTY`

Пустой набор компонентов.

```rust
assert!(Signature::EMPTY.is_empty());
```

#### `Signature::insert(&mut self, index)`

Добавляет компонент в набор.

```rust
let mut registry = ComponentRegistry::new();
let index = registry.register::<Position>();
let mut signature = Signature::EMPTY;
signature.insert(index);
assert!(signature.contains(index));
```

#### `Signature::remove(&mut self, index)`

Удаляет компонент из набора.

```rust
let mut registry = ComponentRegistry::new();
let index = registry.register::<Position>();
let mut signature = Signature::EMPTY;
signature.insert(index);
signature.remove(index);
assert!(!signature.contains(index));
```

#### `Signature::contains(self, index) -> bool`

Содержит ли набор компонент.

```rust
let mut registry = ComponentRegistry::new();
let index = registry.register::<Position>();
let mut signature = Signature::EMPTY;
signature.insert(index);
assert!(signature.contains(index));
```

#### `Signature::contains_all(self, other) -> bool`

Содержит ли набор все компоненты `other` (для `With`).

```rust
let mut registry = ComponentRegistry::new();
let p = registry.register::<Position>();
let v = registry.register::<Velocity>();
let mut all = Signature::EMPTY;
all.insert(p);
all.insert(v);
let mut only_p = Signature::EMPTY;
only_p.insert(p);
assert!(all.contains_all(only_p));
```

#### `Signature::intersects(self, other) -> bool`

Есть ли общий компонент (для `Without`).

```rust
let mut registry = ComponentRegistry::new();
let p = registry.register::<Position>();
let v = registry.register::<Velocity>();
let mut a = Signature::EMPTY;
a.insert(p);
let mut b = Signature::EMPTY;
b.insert(v);
assert!(!a.intersects(b));
```

#### `Signature::union(self, other) -> Self`

Объединение наборов.

```rust
let mut registry = ComponentRegistry::new();
let p = registry.register::<Position>();
let v = registry.register::<Velocity>();
let mut a = Signature::EMPTY;
a.insert(p);
let mut b = Signature::EMPTY;
b.insert(v);
assert_eq!(a.union(b).len(), 2);
```

#### `Signature::without(self, other) -> Self`

Разность наборов.

```rust
let mut registry = ComponentRegistry::new();
let p = registry.register::<Position>();
let v = registry.register::<Velocity>();
let mut all = Signature::EMPTY;
all.insert(p);
all.insert(v);
let mut only_v = Signature::EMPTY;
only_v.insert(v);
assert_eq!(all.without(only_v).len(), 1);
```

#### `Signature::len(self) -> usize`

Число компонентов в наборе.

```rust
let mut registry = ComponentRegistry::new();
let p = registry.register::<Position>();
let mut signature = Signature::EMPTY;
signature.insert(p);
assert_eq!(signature.len(), 1);
```

#### `Signature::is_empty(self) -> bool`

Пуст ли набор.

```rust
assert!(Signature::EMPTY.is_empty());
```

#### `Signature::iter(self) -> impl Iterator<Item = ComponentIndex>`

Индексы компонентов по возрастанию.

```rust
let mut registry = ComponentRegistry::new();
let p = registry.register::<Position>();
let mut signature = Signature::EMPTY;
signature.insert(p);
let indices: Vec<u16> = signature.iter().map(|index| index.0).collect();
assert_eq!(indices, vec![p.0]);
```

#### `ArchetypeId`

Идентификатор архетипа: монотонный, не переиспользуется.

```rust
let id = ArchetypeId(0);
assert_eq!(id.0, 0);
```

#### `ArchetypeLayout::column_position(&self, index) -> Option<usize>`

Позиция компонента в чанке; `None` — компонента нет в архетипе.

```rust
let mut registry = ComponentRegistry::new();
let p = registry.register::<Position>();
let mut signature = Signature::EMPTY;
signature.insert(p);
let layout = registry.layout(signature);
assert_eq!(layout.column_position(p), Some(0));
```

#### `ComponentRegistry::new() -> Self`

Пустой реестр типов компонентов.

```rust
let registry = ComponentRegistry::new();
assert!(registry.index_of::<Position>().is_none());
```

#### `ComponentRegistry::register<T>(&mut self) -> ComponentIndex`

Регистрирует компонент идемпотентно и возвращает индекс. Паникует при
превышении `MAX_COMPONENTS` (дизайн `Signature`).

```rust
let mut registry = ComponentRegistry::new();
let first = registry.register::<Position>();
let second = registry.register::<Position>();
assert_eq!(first, second);
```

#### `ComponentRegistry::index_of<T>(&self) -> Option<ComponentIndex>`

Индекс зарегистрированного компонента.

```rust
let mut registry = ComponentRegistry::new();
let index = registry.register::<Position>();
assert_eq!(registry.index_of::<Position>(), Some(index));
```

#### `ComponentRegistry::info(&self, index) -> Option<ComponentInfo>`

Метаданные компонента: `type_id`, `size`, `align`, фабрика столбца.

```rust
let mut registry = ComponentRegistry::new();
let index = registry.register::<Position>();
let info = registry.info(index).unwrap();
assert_eq!(info.type_id, Position::TYPE_ID);
```

#### `ComponentRegistry::signature_of<T>(&self) -> Option<Signature>`

`Signature` из одного компонента.

```rust
let mut registry = ComponentRegistry::new();
let index = registry.register::<Position>();
let signature = registry.signature_of::<Position>().unwrap();
assert!(signature.contains(index));
```

#### `ComponentRegistry::layout(&self, signature) -> ArchetypeLayout`

Layout для набора компонентов: порядок столбцов, байт на слот, слотов в
чанке.

```rust
let mut registry = ComponentRegistry::new();
let index = registry.register::<Position>();
let mut signature = Signature::EMPTY;
signature.insert(index);
let layout = registry.layout(signature);
assert!(layout.slots_per_chunk >= 1);
```

### World ✅

#### `World::new() -> Self`

Создаёт пустой мир: реестр компонентов, архетипы, аллокатор сущностей,
ресурсы, кэш запросов, `SimTick = ZERO`.

```rust
let world = World::new();
assert_eq!(world.entity_count(), 0);
assert_eq!(world.tick(), SimTick::ZERO);
```

#### `World::register<T>(&mut self) -> ComponentIndex`

Регистрирует компонент идемпотентно и возвращает его индекс.

```rust
let mut world = World::new();
let index = world.register::<Position>();
assert_eq!(world.register::<Position>(), index);
```

#### `World::tick(&self) -> SimTick`

Текущий тик симуляции; используется метками `added/changed`.

```rust
let world = World::new();
assert_eq!(world.tick(), SimTick::ZERO);
```

#### `World::set_tick(&mut self, tick)`

Устанавливает тик вручную (загрузка сейва, детерминированный тест).

```rust
let mut world = World::new();
world.set_tick(SimTick::ZERO);
assert_eq!(world.tick(), SimTick::ZERO);
```

#### `World::advance_tick(&mut self)`

Продвигает тик на 1. Планировщик вызывает перед каждой стадией.

```rust
let mut world = World::new();
world.advance_tick();
assert_eq!(world.tick().get(), 1);
```

#### `World::spawn(&mut self) -> Entity`

Спавнит сущность со случайным UUID.

```rust
let mut world = World::new();
let entity = world.spawn();
assert!(world.is_alive(entity));
```

#### `World::spawn_with_uuid(&mut self, uuid) -> Entity`

Спавнит сущность с заданным UUID (сейвы, сеть, тесты).

```rust
let mut world = World::new();
let uuid = EntityUuid::new();
let entity = world.spawn_with_uuid(uuid);
assert_eq!(world.uuid(entity), Some(uuid));
```

#### `World::spawn_batch(&mut self, count) -> Vec<Entity>`

Спавнит `count` пустых сущностей.

```rust
let mut world = World::new();
let entities = world.spawn_batch(3);
assert_eq!(entities.len(), 3);
assert_eq!(world.entity_count(), 3);
```

#### `World::despawn(&mut self, entity) -> bool`

Удаляет сущность. UUID остаётся в карте для undo/сети; `false` — сущность
мертва.

```rust
let mut world = World::new();
let entity = world.spawn();
assert!(world.despawn(entity));
assert!(!world.despawn(entity));
```

#### `World::insert<T>(&mut self, entity, value) -> Result<(), EcsError>`

Вставляет компонент или заменяет существующий. Замена сохраняет `added` и
обновляет `changed`.

```rust
let mut world = World::new();
let entity = world.spawn();
world.insert(entity, Position { x: 1.0, y: 2.0 }).unwrap();
assert_eq!(world.get::<Position>(entity).unwrap().x, 1.0);
```

#### `World::remove<T>(&mut self, entity) -> Result<Option<T>, EcsError>`

Удаляет компонент и возвращает значение. `Ok(None)` — компонента не было.

```rust
let mut world = World::new();
let entity = world.spawn();
world.insert(entity, Position::default()).unwrap();
assert!(world.remove::<Position>(entity).unwrap().is_some());
assert!(world.remove::<Position>(entity).unwrap().is_none());
```

#### `World::get<T>(&self, entity) -> Option<&T>`

Компонент сущности.

```rust
let mut world = World::new();
let entity = world.spawn();
world.insert(entity, Position { x: 1.0, y: 0.0 }).unwrap();
assert_eq!(world.get::<Position>(entity).unwrap().x, 1.0);
```

#### `World::get_mut<T>(&mut self, entity) -> Option<&mut T>`

Мутабельный компонент сущности.

```rust
let mut world = World::new();
let entity = world.spawn();
world.insert(entity, Position::default()).unwrap();
world.get_mut::<Position>(entity).unwrap().x = 5.0;
assert_eq!(world.get::<Position>(entity).unwrap().x, 5.0);
```

#### `World::contains<T>(&self, entity) -> bool`

Есть ли у сущности компонент.

```rust
let mut world = World::new();
let entity = world.spawn();
assert!(!world.contains::<Position>(entity));
world.insert(entity, Position::default()).unwrap();
assert!(world.contains::<Position>(entity));
```

#### `World::component_tick<T>(&self, entity) -> Option<ComponentTick>`

Метка `added/changed` компонента сущности.

```rust
let mut world = World::new();
let entity = world.spawn();
world.advance_tick();
world.insert(entity, Position::default()).unwrap();
let tick = world.component_tick::<Position>(entity).unwrap();
assert_eq!(tick.added, world.tick());
```

#### `World::uuid(&self, entity) -> Option<EntityUuid>`

UUID сущности.

```rust
let mut world = World::new();
let entity = world.spawn();
assert!(world.uuid(entity).is_some());
```

#### `World::entity_by_uuid(&self, uuid) -> Option<Entity>`

Сущность по UUID; после `despawn` хэндл может быть мёртв — проверяйте
`is_alive`.

```rust
let mut world = World::new();
let uuid = EntityUuid::new();
let entity = world.spawn_with_uuid(uuid);
assert_eq!(world.entity_by_uuid(uuid), Some(entity));
```

#### `World::forget_uuid(&mut self, uuid)`

Убирает UUID из карты после подтверждённого удаления.

```rust
let mut world = World::new();
let uuid = EntityUuid::new();
world.spawn_with_uuid(uuid);
world.forget_uuid(uuid);
assert!(world.entity_by_uuid(uuid).is_none());
```

#### `World::is_alive(&self, entity) -> bool`

Жива ли сущность.

```rust
let mut world = World::new();
let entity = world.spawn();
assert!(world.is_alive(entity));
world.despawn(entity);
assert!(!world.is_alive(entity));
```

#### `World::entity_count(&self) -> usize`

Число живых сущностей.

```rust
let mut world = World::new();
let _ = world.spawn();
assert_eq!(world.entity_count(), 1);
```

#### `World::archetype_count(&self) -> usize`

Число непустых архетипов.

```rust
let mut world = World::new();
let entity = world.spawn();
world.insert(entity, Position::default()).unwrap();
assert!(world.archetype_count() >= 1);
```

#### `World::registry(&self) -> &ComponentRegistry`

Реестр компонентов (только чтение).

```rust
let mut world = World::new();
world.register::<Position>();
assert!(world.registry().index_of::<Position>().is_some());
```

#### `World::query_cache(&self) -> &QueryCache`

Кэш запросов (диагностика и тесты изоляции).

```rust
let world = World::new();
assert!(world.query_cache().is_empty());
```

#### `World::resources(&self) -> &Resources`

Контейнер ресурсов мира.

```rust
let world = World::new();
assert!(world.resources().is_empty());
```

#### `World::resources_mut(&mut self) -> &mut Resources`

Мутабельный контейнер ресурсов.

```rust
let mut world = World::new();
world.resources_mut().insert(42_u32);
assert_eq!(world.resource::<u32>(), Some(&42));
```

#### `World::insert_resource<T>(&mut self, value) -> Option<T>`

Вставляет ресурс, возвращая предыдущий того же типа.

```rust
let mut world = World::new();
assert_eq!(world.insert_resource(1_u32), None);
assert_eq!(world.insert_resource(2_u32), Some(1));
```

#### `World::resource<T>(&self) -> Option<&T>`

Ресурс по типу.

```rust
let mut world = World::new();
world.insert_resource(7_u32);
assert_eq!(world.resource::<u32>(), Some(&7));
```

#### `World::resource_mut<T>(&mut self) -> Option<&mut T>`

Мутабельный ресурс по типу.

```rust
let mut world = World::new();
world.insert_resource(7_u32);
*world.resource_mut::<u32>().unwrap() = 8;
assert_eq!(world.resource::<u32>(), Some(&8));
```

#### `World::remove_resource<T>(&mut self) -> Option<T>`

Удаляет ресурс и возвращает значение.

```rust
let mut world = World::new();
world.insert_resource(7_u32);
assert_eq!(world.remove_resource::<u32>(), Some(7));
assert!(world.resource::<u32>().is_none());
```

#### `World::query<T>(&mut self) -> Query<'_, T>`

Запрос всех сущностей с компонентом `T`; итератор `(Entity, &T)`.

```rust
let mut world = World::new();
let entity = world.spawn();
world.insert(entity, Position { x: 1.0, y: 0.0 }).unwrap();
let values: Vec<f32> = world.query::<Position>().map(|(_, p)| p.x).collect();
assert_eq!(values, vec![1.0]);
```

#### `World::query_filtered<T>(&mut self, spec) -> Query<'_, T>`

Запрос с фильтрами `With`/`Without`.

```rust
use flux_ecs::query::QuerySpec;
let mut world = World::new();
let entity = world.spawn();
world.insert(entity, Position::default()).unwrap();
let spec = QuerySpec::new().with::<Position>().without::<Velocity>();
assert_eq!(world.query_filtered::<Position>(&spec).count(), 1);
```

#### `World::query_mut<T>(&mut self) -> QueryMut<'_, T>`

Мутабельный запрос компонента `T`; батчи по слотам чанка.

```rust
let mut world = World::new();
let entity = world.spawn();
world.insert(entity, Position::default()).unwrap();
world.query_mut::<Position>().for_each(|_, p| p.x += 1.0);
assert_eq!(world.get::<Position>(entity).unwrap().x, 1.0);
```

#### `World::validate(&self)`

Проверка инвариантов архетипов (тесты и debug-режим).

```rust
let mut world = World::new();
let entity = world.spawn();
world.insert(entity, Position::default()).unwrap();
world.validate();
```

### Запросы ✅

#### `QuerySpec::new() -> Self`

Пустой фильтр `With`/`Without`.

```rust
use flux_ecs::query::QuerySpec;
let spec = QuerySpec::new();
let _ = spec;
```

#### `QuerySpec::with<T>(self) -> Self`

Требует наличие компонента `T`.

```rust
use flux_ecs::query::QuerySpec;
let spec = QuerySpec::new().with::<Position>();
let _ = spec;
```

#### `QuerySpec::without<T>(self) -> Self`

Требует отсутствие компонента `T`.

```rust
use flux_ecs::query::QuerySpec;
let spec = QuerySpec::new().with::<Position>().without::<Velocity>();
let _ = spec;
```

#### `QueryKey`

Ключ кэша запросов: `{ with: Signature, without: Signature }`.

```rust
use flux_ecs::query::QueryKey;
let key = QueryKey::default();
assert!(key.with.is_empty());
```

#### `Query::added_since(self, tick) -> Self`

Фильтр `Added<T>`: только компоненты, добавленные после `tick`.

```rust
let mut world = World::new();
let entity = world.spawn();
world.advance_tick();
world.insert(entity, Position::default()).unwrap();
let count = world.query::<Position>().added_since(SimTick::ZERO).count();
assert_eq!(count, 1);
```

#### `Query::changed_since(self, tick) -> Self`

Фильтр `Changed<T>`: только компоненты, изменённые после `tick`.

```rust
let mut world = World::new();
let entity = world.spawn();
world.advance_tick();
world.insert(entity, Position::default()).unwrap();
let count = world.query::<Position>().changed_since(SimTick::ZERO).count();
assert_eq!(count, 1);
```

#### `Iterator for Query<T>`

Итератор `(Entity, &T)`; порядок — по `ArchetypeId`, затем по слотам чанка.

```rust
let mut world = World::new();
let entity = world.spawn();
world.insert(entity, Position::default()).unwrap();
let entities: Vec<Entity> = world.query::<Position>().map(|(entity, _)| entity).collect();
assert_eq!(entities, vec![entity]);
```

#### `QueryCache::new() -> Self`

Пустой кэш списков архетипов.

```rust
use flux_ecs::query::QueryCache;
let cache = QueryCache::new();
assert!(cache.is_empty());
```

#### `QueryCache::len(&self) -> usize`

Число закэшированных ключей.

```rust
use flux_ecs::query::QueryCache;
assert_eq!(QueryCache::new().len(), 0);
```

#### `QueryCache::is_empty(&self) -> bool`

Пуст ли кэш.

```rust
use flux_ecs::query::QueryCache;
assert!(QueryCache::new().is_empty());
```

#### `QueryCache::epoch(&self) -> u64`

Текущая эпоха архетипов; смена эпохи сбрасывает кэш.

```rust
use flux_ecs::query::QueryCache;
assert_eq!(QueryCache::new().epoch(), 0);
```

#### `QueryCache::clear(&mut self)`

Очищает кэш (например, при загрузке мира).

```rust
use flux_ecs::query::QueryCache;
let mut cache = QueryCache::new();
cache.clear();
assert!(cache.is_empty());
```

#### `QueryMut::for_each(&mut self, f)`

Обходит все значения `T` по одному: `f(Entity, &mut T)`.

```rust
let mut world = World::new();
let entity = world.spawn();
world.insert(entity, Position::default()).unwrap();
world.query_mut::<Position>().for_each(|_, p| p.x = 2.0);
assert_eq!(world.get::<Position>(entity).unwrap().x, 2.0);
```

#### `QueryMut::for_each_batch(&mut self, batch_size, f)`

Обходит значения батчами по слотам чанка: `f(&[Entity], &mut [T])`.

```rust
let mut world = World::new();
let entity = world.spawn();
world.insert(entity, Position::default()).unwrap();
world.query_mut::<Position>().for_each_batch(64, |_entities, values| {
    for value in values {
        value.y = 3.0;
    }
});
assert_eq!(world.get::<Position>(entity).unwrap().y, 3.0);
```

### Change detection ✅

#### `ComponentTick::new(tick) -> Self`

Метка нового компонента: `added == changed == tick`.

```rust
let tick = ComponentTick::new(SimTick::ZERO);
assert_eq!(tick.added, tick.changed);
```

#### `ComponentTick::replaced(self, tick) -> Self`

Метка замены значения: `added` сохраняется, `changed` обновляется.

```rust
let tick = ComponentTick::new(SimTick::ZERO).replaced(SimTick::ZERO);
assert_eq!(tick.added, SimTick::ZERO);
assert_eq!(tick.changed, SimTick::ZERO);
```

#### `ComponentTick::is_added_since(self, last_run) -> bool`

Компонент добавлен позже `last_run`.

```rust
let mut later = SimTick::ZERO;
later.advance(1);
let tick = ComponentTick::new(later);
assert!(tick.is_added_since(SimTick::ZERO));
```

#### `ComponentTick::is_changed_since(self, last_run) -> bool`

Компонент изменён позже `last_run`.

```rust
let mut later = SimTick::ZERO;
later.advance(1);
let tick = ComponentTick::new(later);
assert!(tick.is_changed_since(SimTick::ZERO));
```

#### `LastRun::new(tick) -> Self`

Порог чтения `Added`/`Changed` для системы.

```rust
let last = LastRun::new(SimTick::ZERO);
assert_eq!(last.get(), SimTick::ZERO);
```

#### `LastRun::get(self) -> SimTick`

Значение порога.

```rust
assert_eq!(LastRun::new(SimTick::ZERO).get(), SimTick::ZERO);
```

#### `LastRun::set(&mut self, tick)`

Сдвигает порог на текущий тик.

```rust
let mut last = LastRun::new(SimTick::ZERO);
last.set(SimTick::ZERO);
assert_eq!(last.get(), SimTick::ZERO);
```

#### `LastRun::is_added(self, tick) -> bool`

Компонент добавлен после порога.

```rust
let mut later = SimTick::ZERO;
later.advance(1);
let last = LastRun::new(SimTick::ZERO);
assert!(last.is_added(ComponentTick::new(later)));
```

#### `LastRun::is_changed(self, tick) -> bool`

Компонент изменён после порога.

```rust
let mut later = SimTick::ZERO;
later.advance(1);
let last = LastRun::new(SimTick::ZERO);
assert!(last.is_changed(ComponentTick::new(later)));
```

### Commands ✅

#### `CommandTarget`

Ссылка на сущность в команде: `Existing(Entity)` или `Deferred(u32)`.

```rust
let target = CommandTarget::Deferred(0);
assert_eq!(target, CommandTarget::Deferred(0));
```

#### `Commands::new() -> Self`

Пустая очередь отложенных команд.

```rust
let commands = Commands::new();
assert!(commands.is_empty());
```

#### `Commands::len(&self) -> usize`

Число команд в очереди.

```rust
let mut commands = Commands::new();
commands.spawn();
assert_eq!(commands.len(), 1);
```

#### `Commands::is_empty(&self) -> bool`

Пуста ли очередь.

```rust
assert!(Commands::new().is_empty());
```

#### `Commands::spawn(&mut self) -> CommandTarget`

Ставит спавн со случайным UUID; возвращает `Deferred`-хэндл.

```rust
let mut commands = Commands::new();
let target = commands.spawn();
assert_eq!(commands.len(), 1);
let _ = target;
```

#### `Commands::spawn_with_uuid(&mut self, uuid) -> CommandTarget`

Ставит спавн с заданным UUID.

```rust
let mut commands = Commands::new();
let uuid = EntityUuid::new();
let _target = commands.spawn_with_uuid(uuid);
assert_eq!(commands.len(), 1);
```

#### `Commands::despawn(&mut self, target)`

Ставит удаление сущности.

```rust
let mut world = World::new();
let entity = world.spawn();
let mut commands = Commands::new();
commands.despawn(CommandTarget::Existing(entity));
assert_eq!(commands.apply(&mut world).applied, 1);
```

#### `Commands::insert<T>(&mut self, target, value)`

Ставит вставку компонента.

```rust
let mut world = World::new();
let entity = world.spawn();
let mut commands = Commands::new();
commands.insert(CommandTarget::Existing(entity), Position::default());
commands.apply(&mut world);
assert!(world.contains::<Position>(entity));
```

#### `Commands::remove<T>(&mut self, target)`

Ставит удаление компонента; значение отбрасывается.

```rust
let mut world = World::new();
let entity = world.spawn();
world.insert(entity, Position::default()).unwrap();
let mut commands = Commands::new();
commands.remove::<Position>(CommandTarget::Existing(entity));
commands.apply(&mut world);
assert!(!world.contains::<Position>(entity));
```

#### `Commands::clear(&mut self)`

Очищает очередь без применения.

```rust
let mut commands = Commands::new();
commands.spawn();
commands.clear();
assert!(commands.is_empty());
```

#### `Commands::apply(&mut self, world) -> CommandsReport`

Применяет очередь FIFO. Ошибки не прерывают очередь и собираются в отчёте.

```rust
let mut world = World::new();
let mut commands = Commands::new();
let target = commands.spawn();
commands.insert(target, Position::default());
let report = commands.apply(&mut world);
assert_eq!(report.applied, 2);
assert_eq!(report.failed(), 0);
```

#### `CommandsReport::failed(&self) -> usize`

Число неуспешных команд.

```rust
let report = CommandsReport::default();
assert_eq!(report.failed(), 0);
```

### Ресурсы ✅

#### `Resources::new() -> Self`

Пустой контейнер ресурсов.

```rust
use flux_ecs::resources::Resources;
let resources = Resources::new();
assert!(resources.is_empty());
```

#### `Resources::insert<T>(&mut self, value) -> Option<T>`

Вставляет ресурс, возвращая предыдущий того же типа.

```rust
use flux_ecs::resources::Resources;
let mut resources = Resources::new();
assert_eq!(resources.insert(1_u32), None);
assert_eq!(resources.insert(2_u32), Some(1));
```

#### `Resources::get<T>(&self) -> Option<&T>`

Ресурс по типу.

```rust
use flux_ecs::resources::Resources;
let mut resources = Resources::new();
resources.insert(7_u32);
assert_eq!(resources.get::<u32>(), Some(&7));
```

#### `Resources::get_mut<T>(&mut self) -> Option<&mut T>`

Мутабельный ресурс по типу.

```rust
use flux_ecs::resources::Resources;
let mut resources = Resources::new();
resources.insert(7_u32);
*resources.get_mut::<u32>().unwrap() = 8;
assert_eq!(resources.get::<u32>(), Some(&8));
```

#### `Resources::remove<T>(&mut self) -> Option<T>`

Удаляет ресурс и возвращает его.

```rust
use flux_ecs::resources::Resources;
let mut resources = Resources::new();
resources.insert(7_u32);
assert_eq!(resources.remove::<u32>(), Some(7));
```

#### `Resources::contains<T>(&self) -> bool`

Есть ли ресурс типа.

```rust
use flux_ecs::resources::Resources;
let mut resources = Resources::new();
resources.insert(7_u32);
assert!(resources.contains::<u32>());
```

#### `Resources::len(&self) -> usize`

Число ресурсов.

```rust
use flux_ecs::resources::Resources;
let mut resources = Resources::new();
resources.insert(7_u32);
assert_eq!(resources.len(), 1);
```

#### `Resources::is_empty(&self) -> bool`

Нет ли ресурсов.

```rust
use flux_ecs::resources::Resources;
assert!(Resources::new().is_empty());
```

#### `Resources::clear(&mut self)`

Очищает контейнер.

```rust
use flux_ecs::resources::Resources;
let mut resources = Resources::new();
resources.insert(7_u32);
resources.clear();
assert!(resources.is_empty());
```

### Планировщик ✅

#### `Stage::INPUT`

Стадия ввода и подготовки данных (`"input"`).

```rust
assert_eq!(Stage::INPUT.name(), "input");
```

#### `Stage::UPDATE`

Основная стадия симуляции (`"update"`).

```rust
assert_eq!(Stage::UPDATE.name(), "update");
```

#### `Stage::POST`

Пост-обработка: применение команд, очистка (`"post"`).

```rust
assert_eq!(Stage::POST.name(), "post");
```

#### `Stage::name(self) -> &'static str`

Имя стадии.

```rust
assert_eq!(Stage::UPDATE.name(), "update");
```

#### `Access::new() -> Self`

Пустой объявленный доступ системы к компонентам.

```rust
let access = Access::new();
assert!(access.reads.is_empty() && access.writes.is_empty());
```

#### `Access::read<T>(self) -> Self`

Добавляет чтение компонента `T`.

```rust
let access = Access::new().read::<Position>();
assert_eq!(access.reads, vec![Position::TYPE_ID]);
```

#### `Access::write<T>(self) -> Self`

Добавляет запись компонента `T`.

```rust
let access = Access::new().write::<Position>();
assert_eq!(access.writes, vec![Position::TYPE_ID]);
```

#### `TickRate::Every`

Система выполняется каждый тик.

```rust
let rate = TickRate::Every;
assert_eq!(rate, TickRate::default());
```

#### `TickRate::EveryN(n)`

Система выполняется каждый `n`-й тик (`n = 0` трактуется как 1).

```rust
let rate = TickRate::EveryN(10);
assert_ne!(rate, TickRate::Every);
```

#### `TickRate::Once`

Система выполняется один раз за жизнь расписания.

```rust
let rate = TickRate::Once;
assert_ne!(rate, TickRate::Every);
```

#### `SystemDescriptor::new(name, stage) -> Self`

Описание системы в стадии; доступ и зависимости пустые, частота — `Every`.

```rust
let descriptor = SystemDescriptor::new("noop", Stage::UPDATE);
assert_eq!(descriptor.name, "noop");
```

#### `SystemDescriptor::access(self, access) -> Self`

Задаёт объявленный доступ к компонентам.

```rust
let descriptor = SystemDescriptor::new("noop", Stage::UPDATE)
    .access(Access::new().write::<Position>());
assert_eq!(descriptor.access.writes.len(), 1);
```

#### `SystemDescriptor::before(self, names) -> Self`

Системы, которые должны выполниться после этой.

```rust
let descriptor = SystemDescriptor::new("a", Stage::UPDATE).before(&["b"]);
assert_eq!(descriptor.before, vec!["b"]);
```

#### `SystemDescriptor::after(self, names) -> Self`

Системы, которые должны выполниться до этой.

```rust
let descriptor = SystemDescriptor::new("b", Stage::UPDATE).after(&["a"]);
assert_eq!(descriptor.after, vec!["a"]);
```

#### `SystemDescriptor::tick_rate(self, rate) -> Self`

Частота запуска системы.

```rust
let descriptor = SystemDescriptor::new("ai", Stage::UPDATE).tick_rate(TickRate::EveryN(5));
assert_eq!(descriptor.tick_rate, TickRate::EveryN(5));
```

#### `ConflictKind`

Тип конфликта доступа: `WriteWrite`, `WriteRead`, `ReadWrite`.

```rust
assert_ne!(ConflictKind::WriteWrite, ConflictKind::ReadWrite);
```

#### `Conflict`

Диагностированный конфликт: `{ first, second, kind }` в порядке исполнения.

```rust
let conflict = Conflict {
    first: "a",
    second: "b",
    kind: ConflictKind::WriteWrite,
};
assert_eq!(conflict.first, "a");
```

#### `ScheduleError`

Ошибка планирования: `DuplicateName`, `UnknownDependency`, `Cycle`.

```rust
let error = ScheduleError::DuplicateName("a");
assert!(error.to_string().contains("duplicate"));
```

#### `StageReport`

Отчёт по стадии: `stage`, `order`, `executed`, `skipped`, `conflicts`,
`commands`.

```rust
let report = StageReport {
    stage: Stage::UPDATE,
    order: vec!["a"],
    executed: 1,
    skipped: 0,
    conflicts: Vec::new(),
    commands: CommandsReport::default(),
};
assert_eq!(report.executed, 1);
```

#### `ScheduleReport`

Отчёт по проходу: `stages: Vec<StageReport>` и `errors: Vec<ScheduleError>`.

```rust
let report = ScheduleReport::default();
assert!(report.stages.is_empty() && report.errors.is_empty());
```

#### `Schedule::new() -> Self`

Пустое расписание без систем и стадий.

```rust
let schedule = Schedule::new();
assert!(schedule.is_empty());
```

#### `Schedule::with_stages(stages) -> Self`

Расписание с явным порядком стадий.

```rust
let schedule = Schedule::with_stages(&[Stage::INPUT, Stage::UPDATE, Stage::POST]);
assert_eq!(schedule.stages().len(), 3);
```

#### `Schedule::stages(&self) -> &[Stage]`

Порядок стадий.

```rust
let schedule = Schedule::with_stages(&[Stage::UPDATE]);
assert_eq!(schedule.stages(), &[Stage::UPDATE]);
```

#### `Schedule::len(&self) -> usize`

Число систем.

```rust
let mut schedule = Schedule::new();
schedule.add(SystemDescriptor::new("noop", Stage::UPDATE), |_, _| {});
assert_eq!(schedule.len(), 1);
```

#### `Schedule::is_empty(&self) -> bool`

Нет ли систем.

```rust
assert!(Schedule::new().is_empty());
```

#### `Schedule::add<F>(&mut self, descriptor, run)`

Добавляет систему. Стадия добавляется в конец списка, если её ещё нет.
Порядок систем определяется `before`/`after` и именем, а не порядком
регистрации.

```rust
let mut schedule = Schedule::new();
schedule.add(SystemDescriptor::new("noop", Stage::UPDATE), |_, _| {});
assert_eq!(schedule.stages(), &[Stage::UPDATE]);
```

#### `Schedule::plan(&self, stage) -> Result<Vec<&'static str>, ScheduleError>`

Порядок имён систем в стадии: топосортировка Кана, тай-брейк по имени.

```rust
let mut schedule = Schedule::with_stages(&[Stage::UPDATE]);
schedule.add(SystemDescriptor::new("b", Stage::UPDATE), |_, _| {});
schedule.add(SystemDescriptor::new("a", Stage::UPDATE).before(&["b"]), |_, _| {});
assert_eq!(schedule.plan(Stage::UPDATE).unwrap(), vec!["a", "b"]);
```

#### `Schedule::conflicts(&self, stage) -> Result<Vec<Conflict>, ScheduleError>`

Диагностирует конфликты read/write в порядке исполнения стадии; порядок
систем не меняет.

```rust
let mut schedule = Schedule::with_stages(&[Stage::UPDATE]);
schedule.add(
    SystemDescriptor::new("a", Stage::UPDATE).access(Access::new().write::<Position>()),
    |_, _| {},
);
schedule.add(
    SystemDescriptor::new("b", Stage::UPDATE).access(Access::new().read::<Position>()),
    |_, _| {},
);
assert_eq!(schedule.conflicts(Stage::UPDATE).unwrap().len(), 1);
```

#### `Schedule::run(&mut self, world, commands) -> ScheduleReport`

Исполняет один тик: продвигает `SimTick`, проходит стадии, применяет
`Commands` на границе каждой стадии. Стадия с ошибкой планирования не
исполняется и попадает в `errors`.

```rust
let mut world = World::new();
let mut commands = Commands::new();
let mut schedule = Schedule::with_stages(&[Stage::UPDATE]);
schedule.add(SystemDescriptor::new("noop", Stage::UPDATE), |_, _| {});
let report = schedule.run(&mut world, &mut commands);
assert_eq!(report.stages[0].executed, 1);
assert_eq!(world.tick().get(), 1);
```

### SchemaRegistry ✅

#### `FieldType`

Закрытое перечисление типов значений: `F32`, `F64`, `I32`, `U32`, `I64`,
`U64`, `Bool`, `String`, `Vec3`, `DVec3`, `WorldPos`, `Uuid`, `Enum`,
`Bytes`, `Other`.

```rust
use flux_ecs::schema::FieldType;
assert_eq!(FieldType::F32, FieldType::F32);
```

#### `EditorMeta::visible() -> Self`

Метаданные видимого поля без ограничений.

```rust
use flux_ecs::schema::EditorMeta;
assert!(EditorMeta::visible().visible);
```

#### `OrphanPolicy`

Политика осиротевших записей: `Log` (по умолчанию), `Respawn`, `Drop`.

```rust
use flux_ecs::schema::OrphanPolicy;
assert_eq!(OrphanPolicy::default(), OrphanPolicy::Log);
```

#### `FieldSchema::new(field_uuid, name, ty, flags) -> Self`

Поле схемы с флагами и типом; метаданные — `EditorMeta::visible()`.

```rust
use flux_ecs::schema::{FieldSchema, FieldType};
let field = FieldSchema::new(
    FieldId::new(),
    "x",
    FieldType::F32,
    FieldFlags::saved(),
);
assert_eq!(field.name, "x");
```

#### `FieldSchema::with_editor(self, editor) -> Self`

Задаёт метаданные инспектора.

```rust
use flux_ecs::schema::{EditorMeta, FieldSchema, FieldType};
let field = FieldSchema::new(FieldId::new(), "x", FieldType::F32, FieldFlags::saved())
    .with_editor(EditorMeta::visible());
assert!(field.editor.visible);
```

#### `ComponentSchema::new(schema, name) -> Self`

Пустая схема типа: ревизия, имя, без полей, `OrphanPolicy::Log`.

```rust
use flux_ecs::schema::ComponentSchema;
let schema = ComponentSchema::new(
    SchemaRef::new(TypeId::new_v4(), 1),
    "Position",
);
assert!(schema.fields.is_empty());
```

#### `ComponentSchema::with_field(self, field) -> Self`

Добавляет поле в схему.

```rust
use flux_ecs::schema::{ComponentSchema, FieldSchema, FieldType};
let schema = ComponentSchema::new(SchemaRef::new(TypeId::new_v4(), 1), "Position")
    .with_field(FieldSchema::new(FieldId::new(), "x", FieldType::F32, FieldFlags::saved()));
assert_eq!(schema.fields.len(), 1);
```

#### `ComponentSchema::field(&self, field_uuid) -> Option<&FieldSchema>`

Поле по UUID.

```rust
use flux_ecs::schema::{ComponentSchema, FieldSchema, FieldType};
let id = FieldId::new();
let schema = ComponentSchema::new(SchemaRef::new(TypeId::new_v4(), 1), "Position")
    .with_field(FieldSchema::new(id, "x", FieldType::F32, FieldFlags::saved()));
assert_eq!(schema.field(id).unwrap().name, "x");
```

#### `ComponentSchema::save_fields(&self) -> impl Iterator<Item = &FieldSchema>`

Поля, попадающие в сейв.

```rust
use flux_ecs::schema::{ComponentSchema, FieldSchema, FieldType};
let schema = ComponentSchema::new(SchemaRef::new(TypeId::new_v4(), 1), "Position")
    .with_field(FieldSchema::new(FieldId::new(), "x", FieldType::F32, FieldFlags::saved()));
assert_eq!(schema.save_fields().count(), 1);
```

#### `ComponentSchema::replicated_fields(&self) -> impl Iterator<Item = &FieldSchema>`

Поля, попадающие в wire-схему.

```rust
use flux_ecs::schema::{ComponentSchema, FieldSchema, FieldType};
let schema = ComponentSchema::new(SchemaRef::new(TypeId::new_v4(), 1), "Position")
    .with_field(FieldSchema::new(FieldId::new(), "x", FieldType::F32, FieldFlags::replicated()));
assert_eq!(schema.replicated_fields().count(), 1);
```

#### `ComponentSchema::sort_fields(&mut self)`

Сортирует поля по `field_uuid` (канонический порядок).

```rust
use flux_ecs::schema::{ComponentSchema, FieldSchema, FieldType};
let mut schema = ComponentSchema::new(SchemaRef::new(TypeId::new_v4(), 1), "Position")
    .with_field(FieldSchema::new(FieldId::new(), "x", FieldType::F32, FieldFlags::saved()));
schema.sort_fields();
assert_eq!(schema.fields.len(), 1);
```

#### `ComponentSchema::validate(&self) -> Result<(), SchemaError>`

Проверяет уникальность UUID полей; дубликат — `SchemaError::DuplicateField`.

```rust
use flux_ecs::schema::{ComponentSchema, FieldSchema, FieldType};
let mut schema = ComponentSchema::new(SchemaRef::new(TypeId::new_v4(), 1), "Position");
schema.validate().unwrap();
```

#### `SchemaRegistry::new() -> Self`

Пустой реестр схем.

```rust
use flux_ecs::schema::SchemaRegistry;
let registry = SchemaRegistry::new();
assert!(registry.is_empty());
```

#### `SchemaRegistry::register(&mut self, schema) -> Result<(), SchemaError>`

Регистрирует схему: сортирует поля, проверяет дубликаты и конфликт ревизий.
Идемпотентно при совпадении содержимого.

```rust
use flux_ecs::schema::{ComponentSchema, SchemaRegistry};
let mut registry = SchemaRegistry::new();
let schema = ComponentSchema::new(SchemaRef::new(TypeId::new_v4(), 1), "Position");
registry.register(schema.clone()).unwrap();
registry.register(schema).unwrap();
assert_eq!(registry.len(), 1);
```

#### `SchemaRegistry::get(&self, type_uuid) -> Option<&ComponentSchema>`

Текущая (последняя) схема типа.

```rust
use flux_ecs::schema::{ComponentSchema, SchemaRegistry};
let type_uuid = TypeId::new_v4();
let mut registry = SchemaRegistry::new();
registry.register(ComponentSchema::new(SchemaRef::new(type_uuid, 1), "Position")).unwrap();
assert!(registry.get(type_uuid).is_some());
```

#### `SchemaRegistry::get_ref(&self, schema) -> Option<&ComponentSchema>`

Схема конкретной ревизии.

```rust
use flux_ecs::schema::{ComponentSchema, SchemaRegistry};
let schema_ref = SchemaRef::new(TypeId::new_v4(), 1);
let mut registry = SchemaRegistry::new();
registry.register(ComponentSchema::new(schema_ref, "Position")).unwrap();
assert!(registry.get_ref(schema_ref).is_some());
```

#### `SchemaRegistry::field(&self, schema, field_uuid) -> Option<&FieldSchema>`

Поле конкретной ревизии.

```rust
use flux_ecs::schema::{ComponentSchema, FieldSchema, FieldType, SchemaRegistry};
let schema_ref = SchemaRef::new(TypeId::new_v4(), 1);
let field_id = FieldId::new();
let mut registry = SchemaRegistry::new();
registry.register(
    ComponentSchema::new(schema_ref, "Position")
        .with_field(FieldSchema::new(field_id, "x", FieldType::F32, FieldFlags::saved())),
).unwrap();
assert!(registry.field(schema_ref, field_id).is_some());
```

#### `SchemaRegistry::len(&self) -> usize`

Число зарегистрированных ревизий.

```rust
use flux_ecs::schema::SchemaRegistry;
assert_eq!(SchemaRegistry::new().len(), 0);
```

#### `SchemaRegistry::is_empty(&self) -> bool`

Пуст ли реестр.

```rust
use flux_ecs::schema::SchemaRegistry;
assert!(SchemaRegistry::new().is_empty());
```

#### `SchemaRegistry::schemas(&self) -> impl Iterator<Item = &ComponentSchema>`

Обход ревизий в порядке `(type_uuid, schema_rev)`.

```rust
use flux_ecs::schema::{ComponentSchema, SchemaRegistry};
let mut registry = SchemaRegistry::new();
registry.register(ComponentSchema::new(SchemaRef::new(TypeId::new_v4(), 1), "Position")).unwrap();
assert_eq!(registry.schemas().count(), 1);
```

#### `SchemaError`

Ошибка реестра: `RevisionConflict { schema }` и `DuplicateField(FieldId)`.

```rust
use flux_ecs::schema::SchemaError;
let error = SchemaError::DuplicateField(FieldId::new());
assert!(error.to_string().contains("duplicate"));
```

### Save / ComponentDelta ✅

#### `Value`

Закрытое перечисление значений дельты: `F32`, `F64`, `I32`, `U32`, `I64`,
`U64`, `Bool`, `String`, `Vec3`, `DVec3`, `WorldPos`, `Uuid`,
`Enum { variant }`, `Bytes`.

```rust
use flux_ecs::save::Value;
assert_eq!(Value::Bool(true), Value::Bool(true));
```

#### `FieldValue::new(field, value) -> Self`

Пара «поле — значение» в дельте.

```rust
use flux_ecs::save::{FieldValue, Value};
let value = FieldValue::new(FieldId::new(), Value::F32(1.0));
assert_eq!(value.value, Value::F32(1.0));
```

#### `ComponentDelta::new(entity, schema) -> Self`

Пустая дельта компонента.

```rust
use flux_ecs::save::ComponentDelta;
let delta = ComponentDelta::new(EntityUuid::new(), SchemaRef::new(TypeId::new_v4(), 1));
assert!(delta.fields.is_empty());
```

#### `ComponentDelta::with_field(self, field) -> Self`

Добавляет поле в дельту.

```rust
use flux_ecs::save::{ComponentDelta, FieldValue, Value};
let delta = ComponentDelta::new(EntityUuid::new(), SchemaRef::new(TypeId::new_v4(), 1))
    .with_field(FieldValue::new(FieldId::new(), Value::Bool(true)));
assert_eq!(delta.fields.len(), 1);
```

#### `ComponentDelta::field(&self, field) -> Option<&Value>`

Значение поля по `FieldId`.

```rust
use flux_ecs::save::{ComponentDelta, FieldValue, Value};
let field = FieldId::new();
let delta = ComponentDelta::new(EntityUuid::new(), SchemaRef::new(TypeId::new_v4(), 1))
    .with_field(FieldValue::new(field, Value::F32(1.0)));
assert_eq!(delta.field(field), Some(&Value::F32(1.0)));
```

#### `ComponentDelta::sort(&mut self)`

Сортирует поля по `field_uuid` (канонический порядок).

```rust
use flux_ecs::save::{ComponentDelta, FieldValue, Value};
let mut delta = ComponentDelta::new(EntityUuid::new(), SchemaRef::new(TypeId::new_v4(), 1))
    .with_field(FieldValue::new(FieldId::new(), Value::Bool(true)));
delta.sort();
assert_eq!(delta.fields.len(), 1);
```

#### `OrphanedField`

Осиротевшее поле: `{ entity, schema, field }`. Пишется в журнал, не теряется
молча.

```rust
use flux_ecs::save::OrphanedField;
let orphan = OrphanedField {
    entity: EntityUuid::new(),
    schema: SchemaRef::new(TypeId::new_v4(), 1),
    field: FieldId::new(),
};
assert!(orphan.schema.schema_rev >= 1);
```

#### `JournalEntry`

Запись журнала: `{ seq, tick, deltas }`.

```rust
use flux_ecs::save::JournalEntry;
let entry = JournalEntry { seq: 1, tick: SimTick::ZERO, deltas: Vec::new() };
assert_eq!(entry.seq, 1);
```

#### `SaveJournal::new() -> Self`

Пустой append-only журнал.

```rust
use flux_ecs::save::SaveJournal;
let journal = SaveJournal::new();
assert!(journal.is_empty());
```

#### `SaveJournal::len(&self) -> usize`

Число записей.

```rust
use flux_ecs::save::SaveJournal;
assert_eq!(SaveJournal::new().len(), 0);
```

#### `SaveJournal::is_empty(&self) -> bool`

Пуст ли журнал.

```rust
use flux_ecs::save::SaveJournal;
assert!(SaveJournal::new().is_empty());
```

#### `SaveJournal::seq(&self) -> u64`

Последний выданный `seq`.

```rust
use flux_ecs::save::SaveJournal;
assert_eq!(SaveJournal::new().seq(), 0);
```

#### `SaveJournal::orphans(&self) -> &[OrphanedField]`

Осиротевшие поля, обнаруженные `sanitize`.

```rust
use flux_ecs::save::SaveJournal;
assert!(SaveJournal::new().orphans().is_empty());
```

#### `SaveJournal::append(&mut self, tick, deltas) -> u64`

Добавляет запись; дельты и поля приводятся к каноническому порядку
(дельта — по `(entity, type, rev)`, поле — по UUID). Возвращает `seq`.

```rust
use flux_ecs::save::{ComponentDelta, SaveJournal};
let mut journal = SaveJournal::new();
let seq = journal.append(SimTick::ZERO, vec![ComponentDelta::new(
    EntityUuid::new(),
    SchemaRef::new(TypeId::new_v4(), 1),
)]);
assert_eq!(seq, 1);
```

#### `SaveJournal::sanitize(&mut self, registry) -> usize`

Убирает поля, отсутствующие в реестре, и пишет их в `orphans`. Возвращает
число удалённых полей.

```rust
use flux_ecs::save::{ComponentDelta, FieldValue, SaveJournal, Value};
use flux_ecs::schema::SchemaRegistry;
let mut journal = SaveJournal::new();
let schema = SchemaRef::new(TypeId::new_v4(), 1);
journal.append(SimTick::ZERO, vec![ComponentDelta::new(EntityUuid::new(), schema)
    .with_field(FieldValue::new(FieldId::new(), Value::Bool(true)))]);
let removed = journal.sanitize(&SchemaRegistry::new());
assert_eq!(removed, 1);
```

### SimTime-компонент ✅

#### `TICKS_PER_DAY`

Тиков в игровых сутках при шаге 60 Гц: `86_400 * 60`.

```rust
use flux_ecs::sim_time::TICKS_PER_DAY;
assert_eq!(TICKS_PER_DAY, 5_184_000);
```

#### `SimTimeComponent::from_tick(tick) -> Self`

Снимок времени по тику: абсолютный тик и квантованная доля суток.

```rust
use flux_ecs::sim_time::SimTimeComponent;
let time = SimTimeComponent::from_tick(SimTick::ZERO);
assert_eq!(time.tick(), 0);
```

#### `SimTimeComponent::tick(&self) -> u64`

Абсолютный тик.

```rust
use flux_ecs::sim_time::SimTimeComponent;
assert_eq!(SimTimeComponent::default().tick(), 0);
```

#### `SimTimeComponent::day_fraction_q16(&self) -> u32`

Доля суток в 1/65536.

```rust
use flux_ecs::sim_time::SimTimeComponent;
assert_eq!(SimTimeComponent::default().day_fraction_q16(), 0);
```

#### `SimTimeComponent::advance(&mut self, steps)`

Продвигает время на `steps` тиков и пересчитывает долю суток.

```rust
use flux_ecs::sim_time::SimTimeComponent;
let mut time = SimTimeComponent::default();
time.advance(60);
assert_eq!(time.tick(), 60);
```

#### `SimTimeComponent::set_tick(&mut self, tick)`

Устанавливает время по тику.

```rust
use flux_ecs::sim_time::SimTimeComponent;
let mut time = SimTimeComponent::default();
time.set_tick(SimTick::ZERO);
assert_eq!(time.tick(), 0);
```

## flux-macros

Дерайв `Reflect` (✅): заполняет `Component::TYPE_ID` и `Component::schema()`,
включая поля, флаги сейва/репликации и метаданные инспектора.

```rust
use flux_macros::Reflect;

#[derive(Default, Reflect)]
#[reflect(type_uuid = "6b1c2f0e-9a4d-4c8b-9f2e-0a1b2c3d4e5f", schema_rev = 1, orphan = "log")]
struct Health {
    #[save]
    #[reflect(min = 0.0, max = 100.0, slider, category = "Stats")]
    current: f32,
    #[replicated]
    #[field(uuid = "6b1c2f0e-9a4d-4c8b-9f2e-0a1b2c3d4e60")]
    team: u32,
}
```

#### Атрибут `type_uuid = "..."`

Стабильный UUID типа компонента. Обязателен для дерайва; попадает в
`TYPE_ID` и `SchemaRef`.

```rust
#[derive(Default, flux_macros::Reflect)]
#[reflect(type_uuid = "6b1c2f0e-9a4d-4c8b-9f2e-0a1b2c3d4e5f")]
struct Marker;
```

#### Атрибут `schema_rev = N`

Ревизия схемы типа. При изменении набора/типов полей увеличивается, чтобы
сейвы могли мигрировать.

```rust
#[derive(Default, flux_macros::Reflect)]
#[reflect(type_uuid = "6b1c2f0e-9a4d-4c8b-9f2e-0a1b2c3d4e5f", schema_rev = 2)]
struct MarkerV2;
```

#### Атрибут `orphan = "log" | "respawn" | "drop"`

Политика осиротевших записей компонента.

```rust
#[derive(Default, flux_macros::Reflect)]
#[reflect(type_uuid = "6b1c2f0e-9a4d-4c8b-9f2e-0a1b2c3d4e5f", orphan = "drop")]
struct Droppable;
```

#### Атрибут поля `#[save]`

Помечает поле как сохраняемое (`FieldFlags::saved`).

```rust
#[derive(Default, flux_macros::Reflect)]
#[reflect(type_uuid = "6b1c2f0e-9a4d-4c8b-9f2e-0a1b2c3d4e5f")]
struct Saved {
    #[save]
    value: f32,
}
```

#### Атрибут поля `#[replicated]`

Помечает поле как реплицируемое (`FieldFlags::replicated`).

```rust
#[derive(Default, flux_macros::Reflect)]
#[reflect(type_uuid = "6b1c2f0e-9a4d-4c8b-9f2e-0a1b2c3d4e5f")]
struct Replicated {
    #[replicated]
    team: u32,
}
```

#### Атрибут поля `#[field(uuid = "...")]`

Стабильный UUID поля; без него дерайв выводит UUID детерминированно из
`type_uuid` и имени.

```rust
#[derive(Default, flux_macros::Reflect)]
#[reflect(type_uuid = "6b1c2f0e-9a4d-4c8b-9f2e-0a1b2c3d4e5f")]
struct WithFieldId {
    #[field(uuid = "6b1c2f0e-9a4d-4c8b-9f2e-0a1b2c3d4e60")]
    value: f32,
}
```

#### Атрибуты `#[reflect(min, max, slider, category, tooltip, visible)]`

Метаданные инспектора (`EditorMeta`): диапазон, слайдер, категория,
подсказка, видимость.

```rust
#[derive(Default, flux_macros::Reflect)]
#[reflect(type_uuid = "6b1c2f0e-9a4d-4c8b-9f2e-0a1b2c3d4e5f")]
struct Inspectable {
    #[reflect(min = 0.0, max = 1.0, slider, category = "Gameplay", visible)]
    value: f32,
}
```

#### `extern crate self as flux_ecs`

Внутри `flux-ecs` дерайв ссылается на `::flux_ecs`; алиас объявлен в
[`lib.rs`](crates/flux-ecs/src/lib.rs:19), поэтому сгенерированный код
одинаков для крейта и пользователей.

```rust
// В crates/flux-ecs/src/lib.rs
extern crate self as flux_ecs;
```

## flux-platform

Платформа: окно/display, ввод, системные сервисы. Headless по умолчанию.

Импорт:

```rust
use std::time::Duration;
use flux_platform::{
    clipboard::{Clipboard, MemoryClipboard},
    crash::{install, CrashConfig},
    cursor::{CursorMode, CursorPosition},
    devices::{gilrs_backend, DeviceEvent, DeviceRegistry, GamepadInfo},
    display::{DpiInfo, LogicalSize, MonitorInfo, PhysicalSize, VideoMode},
    input::{
        curves::{AxisSettings, Deadzone, ResponseCurve},
        intention::{
            intention_from_input, intention_from_values, IntentionActions, IntentionConfig,
        },
        mapping::{
            ActionKind, ActionValue, AxisComponent, Binding, BindingSource, Chord, ChordBinding,
            ContextStack, InputAction, InputContext, InputMap, InputState, PlayerId,
        },
        raw::{MouseButton, RawEvent, RawEventKind, RawQueue, Scancode},
        rebinding::{detect_conflicts, BindingProfile, Conflict, ProfileBinding},
    },
    paths::ProjectPaths,
    power::{PowerEvent, PowerInfo, ThermalPressure},
    text::{ImeEvent, TextEvent, TextInput},
    window::{
        apply_cursor_mode, cursor_grab_mode, map_key_code, map_mouse_button, CloseDecision,
        CloseRequest, WindowEvent, WindowEventQueue, WindowId, WindowMetrics, WindowMode,
        WinitInputAdapter,
    },
};
```

### Raw-слой ✅

#### `Scancode(pub u32)`

Физический код клавиши (USB HID usage), не символ раскладки.

```rust
let scancode = Scancode(0x04);
assert_eq!(scancode, Scancode::A);
```

#### Константы `Scancode::{W, A, S, D, SPACE, LEFT_CTRL, ESCAPE, F1}`

Физические позиции клавиш. На AZERTY `W` — это Z, но код остаётся тем же,
поэтому управление не зависит от раскладки.

```rust
assert_eq!(Scancode::W.0, 0x1A);
assert_eq!(Scancode::A.0, 0x04);
assert_eq!(Scancode::SPACE.0, 0x2C);
assert_eq!(Scancode::ESCAPE.0, 0x29);
```

#### `MouseButton`

Кнопка мыши: `Left`, `Right`, `Middle`, `Other(u16)`.

```rust
assert_ne!(MouseButton::Left, MouseButton::Right);
```

#### `RawEventKind`

Тип сырого события: `Key { scancode, pressed, repeat }`,
`MouseButton { button, pressed }`, `MouseMotion { dx, dy }`,
`Wheel { dx, dy }`, `Focus { focused }`, `Text(String)`.

```rust
let kind = RawEventKind::Focus { focused: true };
assert!(matches!(kind, RawEventKind::Focus { focused: true }));
```

#### `RawEvent::new(timestamp, kind) -> Self`

Создаёт событие с таймстемпом относительно начала кадра/сессии.

```rust
let event = RawEvent::new(
    Duration::from_millis(1),
    RawEventKind::Key { scancode: Scancode::W, pressed: true, repeat: false },
);
assert_eq!(event.timestamp, Duration::from_millis(1));
```

#### `RawEvent::is_key_press(&self) -> bool`

`true` для нового нажатия клавиши; автоповтор (`repeat = true`) не считается.

```rust
let press = RawEvent::new(Duration::ZERO, RawEventKind::Key {
    scancode: Scancode::W, pressed: true, repeat: false,
});
let repeat = RawEvent::new(Duration::ZERO, RawEventKind::Key {
    scancode: Scancode::W, pressed: true, repeat: true,
});
assert!(press.is_key_press());
assert!(!repeat.is_key_press());
```

#### `RawQueue::new() -> Self`

Пустая очередь событий кадра.

```rust
let queue = RawQueue::new();
assert!(queue.is_empty());
```

#### `RawQueue::push(&mut self, event)`

Добавляет событие.

```rust
let mut queue = RawQueue::new();
queue.push(RawEvent::new(Duration::ZERO, RawEventKind::Focus { focused: true }));
assert_eq!(queue.len(), 1);
```

#### `RawQueue::len(&self) -> usize`

Число событий.

```rust
let mut queue = RawQueue::new();
queue.push(RawEvent::new(Duration::ZERO, RawEventKind::Focus { focused: true }));
assert_eq!(queue.len(), 1);
```

#### `RawQueue::is_empty(&self) -> bool`

Пуста ли очередь.

```rust
assert!(RawQueue::new().is_empty());
```

#### `RawQueue::events(&self) -> &[RawEvent]`

События без очистки.

```rust
let mut queue = RawQueue::new();
queue.push(RawEvent::new(Duration::ZERO, RawEventKind::Focus { focused: true }));
assert_eq!(queue.events().len(), 1);
```

#### `RawQueue::take(&mut self) -> Vec<RawEvent>`

Забирает события; очередь очищается.

```rust
let mut queue = RawQueue::new();
queue.push(RawEvent::new(Duration::ZERO, RawEventKind::Focus { focused: true }));
let events = queue.take();
assert_eq!(events.len(), 1);
assert!(queue.is_empty());
```

#### `RawQueue::clear(&mut self)`

Очищает очередь.

```rust
let mut queue = RawQueue::new();
queue.push(RawEvent::new(Duration::ZERO, RawEventKind::Focus { focused: true }));
queue.clear();
assert!(queue.is_empty());
```

#### `RawQueue::last_timestamp(&self) -> Option<Duration>`

Таймстемп последнего события.

```rust
let mut queue = RawQueue::new();
queue.push(RawEvent::new(Duration::from_millis(5), RawEventKind::Focus { focused: true }));
assert_eq!(queue.last_timestamp(), Some(Duration::from_millis(5)));
```

#### `RawQueue::focus_lost(&self) -> bool`

Была ли потеря фокуса в кадре (все клавиши считаются отпущенными).

```rust
let mut queue = RawQueue::new();
queue.push(RawEvent::new(Duration::ZERO, RawEventKind::Focus { focused: false }));
assert!(queue.focus_lost());
```

### Mapping ✅

Примеры используют действие движения:

```rust
use uuid::Uuid;
use flux_platform::input::mapping::{ActionKind, InputAction};

const MOVE: InputAction = InputAction::new(
    "move",
    Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_0300),
    ActionKind::Axis2,
);
```

#### `ActionValue::is_active(self) -> bool`

Активно ли значение: `Bool(true)` или любая ось с модулем больше `1e-6`.

```rust
assert!(ActionValue::Bool(true).is_active());
assert!(!ActionValue::Axis2([0.0, 0.0]).is_active());
```

#### `ActionValue::as_bool(self) -> bool`

Приводит значение к `bool`; для осей — через `is_active`.

```rust
assert!(ActionValue::Axis1(0.5).as_bool());
assert!(!ActionValue::Axis1(0.0).as_bool());
```

#### `ActionValue::as_axis1(self) -> f64`

Первая ось; `Bool` даёт `0.0`/`1.0`.

```rust
assert_eq!(ActionValue::Bool(true).as_axis1(), 1.0);
assert_eq!(ActionValue::Axis2([0.25, 0.5]).as_axis1(), 0.25);
```

#### `ActionValue::as_axis2(self) -> [f64; 2]`

Первые две оси; `Axis1` дополняется нулём.

```rust
assert_eq!(ActionValue::Axis1(0.5).as_axis2(), [0.5, 0.0]);
assert_eq!(ActionValue::Axis3([1.0, 2.0, 3.0]).as_axis2(), [1.0, 2.0]);
```

#### `ActionKind`

Тип действия: `Button`, `Axis1`, `Axis2`, `Axis3`.

```rust
assert_eq!(MOVE.kind, ActionKind::Axis2);
```

#### `InputAction::new(name, id, kind) -> Self`

Действие: имя для отладки, стабильный UUID для данных, тип значения.

```rust
const JUMP: InputAction = InputAction::new("jump", Uuid::from_u128(2), ActionKind::Button);
assert_eq!(JUMP.name, "jump");
```

#### `PlayerId(pub u8)`

Игрок для local multiplayer; `Default` — `PlayerId(0)`.

```rust
assert_eq!(PlayerId::default(), PlayerId(0));
```

#### `BindingSource`

Источник биндинга: `Key(Scancode)`, `Mouse(MouseButton)`,
`GamepadButton(u16)`, `GamepadAxis { axis, sign }`.

```rust
let source = BindingSource::Key(Scancode::W);
assert_eq!(source, BindingSource::Key(Scancode::W));
```

#### `BindingSource::key(scancode) -> Self`

Удобный конструктор клавиши.

```rust
assert_eq!(BindingSource::key(Scancode::A), BindingSource::Key(Scancode::A));
```

#### `AxisComponent::button() -> Self`

Кнопочный вклад: `axis = 0`, `sign = 1.0`.

```rust
let component = AxisComponent::button();
assert_eq!(component.axis, 0);
assert_eq!(component.sign, 1.0);
```

#### `AxisComponent::axis(axis, sign) -> Self`

Вклад в конкретную ось с направлением.

```rust
let component = AxisComponent::axis(1, -1.0);
assert_eq!(component.axis, 1);
assert_eq!(component.sign, -1.0);
```

#### `Binding::new(action, source, player) -> Self`

Глобальный биндинг кнопки.

```rust
let binding = Binding::new(MOVE, BindingSource::key(Scancode::W), PlayerId(0));
assert_eq!(binding.context, "");
```

#### `Binding::in_context(self, context) -> Self`

Биндинг действует только в контексте.

```rust
let binding = Binding::new(MOVE, BindingSource::key(Scancode::W), PlayerId(0))
    .in_context(InputContext::GAMEPLAY.name);
assert_eq!(binding.context, "gameplay");
```

#### `Binding::with_component(self, component) -> Self`

Задаёт вклад в ось.

```rust
let binding = Binding::new(MOVE, BindingSource::key(Scancode::W), PlayerId(0))
    .with_component(AxisComponent::axis(1, 1.0));
assert_eq!(binding.component.axis, 1);
```

#### `InputContext::new(name, priority) -> Self`

Контекст ввода с приоритетом поглощения.

```rust
let context = InputContext::new("custom", 10);
assert_eq!(context.priority, 10);
```

#### `InputContext::GAMEPLAY`

Геймплейный контекст (`"gameplay"`, приоритет 0).

```rust
assert_eq!(InputContext::GAMEPLAY.name, "gameplay");
```

#### `InputContext::UI`

UI-контекст (`"ui"`, приоритет 100) — поглощает действие и источники.

```rust
assert!(InputContext::UI.priority > InputContext::GAMEPLAY.priority);
```

#### `InputContext::DIALOGUE`

Контекст диалога (`"dialogue"`, приоритет 50).

```rust
assert_eq!(InputContext::DIALOGUE.name, "dialogue");
```

#### `ContextStack::new() -> Self`

Пустой стек контекстов.

```rust
let stack = ContextStack::new();
assert!(stack.is_empty());
```

#### `ContextStack::push(&mut self, context)`

Кладёт контекст наверх.

```rust
let mut stack = ContextStack::new();
stack.push(InputContext::UI);
assert_eq!(stack.top(), Some(InputContext::UI));
```

#### `ContextStack::pop(&mut self) -> Option<InputContext>`

Снимает верхний контекст.

```rust
let mut stack = ContextStack::new();
stack.push(InputContext::UI);
assert_eq!(stack.pop(), Some(InputContext::UI));
```

#### `ContextStack::top(&self) -> Option<InputContext>`

Верхний контекст.

```rust
let mut stack = ContextStack::new();
stack.push(InputContext::DIALOGUE);
assert_eq!(stack.top(), Some(InputContext::DIALOGUE));
```

#### `ContextStack::active(&self) -> impl Iterator<Item = &InputContext>`

Активные контексты от верхнего к нижнему.

```rust
let mut stack = ContextStack::new();
stack.push(InputContext::GAMEPLAY);
stack.push(InputContext::UI);
let names: Vec<&str> = stack.active().map(|context| context.name).collect();
assert_eq!(names, vec!["ui", "gameplay"]);
```

#### `ContextStack::contains(&self, name) -> bool`

Есть ли контекст в стеке.

```rust
let mut stack = ContextStack::new();
stack.push(InputContext::UI);
assert!(stack.contains("ui"));
```

#### `ContextStack::len(&self) -> usize`

Число контекстов.

```rust
let mut stack = ContextStack::new();
stack.push(InputContext::UI);
assert_eq!(stack.len(), 1);
```

#### `ContextStack::is_empty(&self) -> bool`

Пуст ли стек.

```rust
assert!(ContextStack::new().is_empty());
```

#### `InputState::new() -> Self`

Начальное состояние устройств: фокус есть.

```rust
let state = InputState::new();
assert!(state.focused());
```

#### `InputState::apply(&mut self, event)`

Применяет raw-событие: клавиши, кнопки, дельты, колесо, фокус, текст.

```rust
let mut state = InputState::new();
state.apply(&RawEvent::new(Duration::ZERO, RawEventKind::Key {
    scancode: Scancode::W, pressed: true, repeat: false,
}));
assert!(state.key_pressed(Scancode::W));
```

#### `InputState::key_pressed(&self, scancode) -> bool`

Нажата ли физическая клавиша.

```rust
let mut state = InputState::new();
state.apply(&RawEvent::new(Duration::ZERO, RawEventKind::Key {
    scancode: Scancode::SPACE, pressed: true, repeat: false,
}));
assert!(state.key_pressed(Scancode::SPACE));
```

#### `InputState::source_active(&self, source) -> bool`

Активен ли источник биндинга (клавиша/мышь/кнопка/ось геймпада).

```rust
let mut state = InputState::new();
state.set_gamepad_button(3, true);
assert!(state.source_active(BindingSource::GamepadButton(3)));
```

#### `InputState::mouse_delta(&self) -> [f64; 2]`

Сырая дельта мыши за кадр.

```rust
let mut state = InputState::new();
state.apply(&RawEvent::new(Duration::ZERO, RawEventKind::MouseMotion { dx: 2.0, dy: -1.0 }));
assert_eq!(state.mouse_delta(), [2.0, -1.0]);
```

#### `InputState::wheel(&self) -> [f64; 2]`

Колесо за кадр.

```rust
let mut state = InputState::new();
state.apply(&RawEvent::new(Duration::ZERO, RawEventKind::Wheel { dx: 0.0, dy: 1.0 }));
assert_eq!(state.wheel(), [0.0, 1.0]);
```

#### `InputState::focused(&self) -> bool`

Есть ли фокус окна.

```rust
assert!(InputState::new().focused());
```

#### `InputState::text(&self) -> &str`

Текстовый поток кадра.

```rust
let mut state = InputState::new();
state.apply(&RawEvent::new(Duration::ZERO, RawEventKind::Text("a".into())));
assert_eq!(state.text(), "a");
```

#### `InputState::end_frame(&mut self)`

Сбрасывает per-frame величины: дельту мыши, колесо, текст.

```rust
let mut state = InputState::new();
state.apply(&RawEvent::new(Duration::ZERO, RawEventKind::MouseMotion { dx: 1.0, dy: 1.0 }));
state.end_frame();
assert_eq!(state.mouse_delta(), [0.0, 0.0]);
```

#### `InputState::set_gamepad_axis(&mut self, axis, value)`

Устанавливает ось геймпада с клампом `-1..1`.

```rust
let mut state = InputState::new();
state.set_gamepad_axis(0, 2.0);
assert!(state.source_active(BindingSource::GamepadAxis { axis: 0, sign: 1 }));
```

#### `InputState::set_gamepad_button(&mut self, button, pressed)`

Устанавливает кнопку геймпада.

```rust
let mut state = InputState::new();
state.set_gamepad_button(0, true);
assert!(state.source_active(BindingSource::GamepadButton(0)));
```

#### `Chord::new(a, b) -> Self`

Чорда из двух клавиш; порядок нормализуется.

```rust
let chord = Chord::new(Scancode::LEFT_CTRL, Scancode::S);
assert_eq!(chord.keys()[0], Scancode::S);
```

#### `Chord::keys(self) -> [Scancode; 2]`

Клавиши в каноническом порядке.

```rust
let chord = Chord::new(Scancode::LEFT_CTRL, Scancode::S);
assert_eq!(chord.keys().len(), 2);
```

#### `Chord::is_active(self, state) -> bool`

Активна ли чорда (нажаты обе клавиши).

```rust
let mut state = InputState::new();
state.apply(&RawEvent::new(Duration::ZERO, RawEventKind::Key {
    scancode: Scancode::LEFT_CTRL, pressed: true, repeat: false,
}));
state.apply(&RawEvent::new(Duration::ZERO, RawEventKind::Key {
    scancode: Scancode::S, pressed: true, repeat: false,
}));
assert!(Chord::new(Scancode::LEFT_CTRL, Scancode::S).is_active(&state));
```

#### `ChordBinding::new(action, chord, player) -> Self`

Глобальная чорда.

```rust
let binding = ChordBinding::new(MOVE, Chord::new(Scancode::LEFT_CTRL, Scancode::S), PlayerId(0));
assert_eq!(binding.context, "");
```

#### `ChordBinding::in_context(self, context) -> Self`

Чорда в контексте.

```rust
let binding = ChordBinding::new(MOVE, Chord::new(Scancode::LEFT_CTRL, Scancode::S), PlayerId(0))
    .in_context(InputContext::UI.name);
assert_eq!(binding.context, "ui");
```

#### `ChordBinding::with_component(self, component) -> Self`

Вклад чорды в ось.

```rust
let binding = ChordBinding::new(MOVE, Chord::new(Scancode::LEFT_CTRL, Scancode::S), PlayerId(0))
    .with_component(AxisComponent::axis(0, 1.0));
assert_eq!(binding.component.axis, 0);
```

#### `InputMap::new() -> Self`

Пустая карта действий.

```rust
let map = InputMap::new();
assert!(map.bindings().is_empty());
```

#### `InputMap::bind(&mut self, binding)`

Добавляет биндинг.

```rust
let mut map = InputMap::new();
map.bind(Binding::new(MOVE, BindingSource::key(Scancode::W), PlayerId(0)));
assert_eq!(map.bindings().len(), 1);
```

#### `InputMap::bind_chord(&mut self, binding)`

Добавляет биндинг чорды.

```rust
let mut map = InputMap::new();
map.bind_chord(ChordBinding::new(MOVE, Chord::new(Scancode::LEFT_CTRL, Scancode::S), PlayerId(0)));
assert_eq!(map.chords().len(), 1);
```

#### `InputMap::bindings(&self) -> &[Binding]`

Биндинги только для чтения.

```rust
let map = InputMap::new();
assert!(map.bindings().is_empty());
```

#### `InputMap::chords(&self) -> &[ChordBinding]`

Биндинги чорд только для чтения.

```rust
let map = InputMap::new();
assert!(map.chords().is_empty());
```

#### `InputMap::contexts(&self) -> &ContextStack`

Стек контекстов.

```rust
let map = InputMap::new();
assert!(map.contexts().is_empty());
```

#### `InputMap::contexts_mut(&mut self) -> &mut ContextStack`

Мутабельный стек контекстов.

```rust
let mut map = InputMap::new();
map.contexts_mut().push(InputContext::UI);
assert!(map.contexts().contains("ui"));
```

#### `InputMap::resolve(&self, state, player) -> BTreeMap<Uuid, ActionValue>`

Разрешает действия игрока. Порядок обхода: контексты от верхнего к нижнему,
затем глобальные биндинги. Внутри контекста биндинги одного действия
накапливаются; модальный контекст (`priority > gameplay`) поглощает действие
и источники. Результат отсортирован по UUID.

```rust
let mut map = InputMap::new();
map.bind(Binding::new(MOVE, BindingSource::key(Scancode::W), PlayerId(0))
    .with_component(AxisComponent::axis(1, 1.0)));
let mut state = InputState::new();
state.apply(&RawEvent::new(Duration::ZERO, RawEventKind::Key {
    scancode: Scancode::W, pressed: true, repeat: false,
}));
let values = map.resolve(&state, PlayerId(0));
assert_eq!(values[&MOVE.id].as_axis2(), [0.0, 1.0]);
```

### Curves ✅

#### `Deadzone::new(inner, outer) -> Self`

Радиальная мёртвая зона с проверкой порядка: `inner` клампится в `0..1`,
`outer` — не меньше `inner`.

```rust
let deadzone = Deadzone::new(0.1, 1.0);
assert_eq!(deadzone.inner, 0.1);
```

#### `Deadzone::apply(&self, value) -> [f64; 2]`

Применяет deadzone с rescale: вход за внутренним радиусом перенормируется в
`0..1`, а не обрезается.

```rust
let deadzone = Deadzone::new(0.1, 1.0);
assert_eq!(deadzone.apply([0.05, 0.0]), [0.0, 0.0]);
let output = deadzone.apply([1.0, 0.0]);
assert!((output[0] - 1.0).abs() < 1e-9);
```

#### `ResponseCurve::apply(self, value) -> f64`

Кривая отклика: `Linear` или `Exponential(exponent)` с сохранением знака.

```rust
assert_eq!(ResponseCurve::Linear.apply(0.5), 0.5);
assert!(ResponseCurve::Exponential(2.0).apply(0.5) < 0.5);
```

#### `AxisSettings::apply(&self, value) -> [f64; 2]`

Полный конвейер: deadzone → response → sensitivity → invert; результат
клампится в `-1..1`.

```rust
let settings = AxisSettings {
    deadzone: Deadzone::new(0.0, 1.0),
    sensitivity: [2.0, 1.0],
    ..AxisSettings::default()
};
assert_eq!(settings.apply([0.25, 0.0]), [0.5, 0.0]);
```

### Rebinding ✅

#### `ProfileBinding::new(action, action_name, kind, source, player) -> Self`

Запись профиля: UUID действия, читаемое имя, тип, источник, игрок.

```rust
let entry = ProfileBinding::new(MOVE.id, MOVE.name, MOVE.kind, BindingSource::key(Scancode::W), PlayerId(0));
assert_eq!(entry.action, MOVE.id);
```

#### `ProfileBinding::in_context(self, context) -> Self`

Биндинг в контексте (строка, `""` — глобальный).

```rust
let entry = ProfileBinding::new(MOVE.id, MOVE.name, MOVE.kind, BindingSource::key(Scancode::W), PlayerId(0))
    .in_context("gameplay");
assert_eq!(entry.context, "gameplay");
```

#### `ProfileBinding::with_component(self, component) -> Self`

Вклад биндинга в ось.

```rust
let entry = ProfileBinding::new(MOVE.id, MOVE.name, MOVE.kind, BindingSource::key(Scancode::W), PlayerId(0))
    .with_component(AxisComponent::axis(1, 1.0));
assert_eq!(entry.component.axis, 1);
```

#### `Conflict`

Конфликт биндингов: `{ player, context, source, actions }`; `actions`
отсортированы по UUID.

```rust
let conflict = Conflict {
    player: PlayerId(0),
    context: String::new(),
    source: BindingSource::key(Scancode::SPACE),
    actions: vec![MOVE.id],
};
assert_eq!(conflict.actions.len(), 1);
```

#### `BindingProfile::new() -> Self`

Пустой профиль биндингов.

```rust
let profile = BindingProfile::new();
assert!(profile.entries().is_empty());
```

#### `BindingProfile::bind(&mut self, entry)`

Добавляет запись в профиль.

```rust
let mut profile = BindingProfile::new();
profile.bind(ProfileBinding::new(MOVE.id, MOVE.name, MOVE.kind, BindingSource::key(Scancode::W), PlayerId(0)));
assert_eq!(profile.entries().len(), 1);
```

#### `BindingProfile::entries(&self) -> &[ProfileBinding]`

Записи профиля только для чтения.

```rust
let profile = BindingProfile::new();
assert!(profile.entries().is_empty());
```

#### `BindingProfile::capture(event) -> Option<BindingSource>`

Захват источника из следующего raw-события. Автоповтор и отпускание
игнорируются.

```rust
let event = RawEvent::new(Duration::ZERO, RawEventKind::Key {
    scancode: Scancode::W, pressed: true, repeat: false,
});
assert_eq!(BindingProfile::capture(&event), Some(BindingSource::key(Scancode::W)));
```

#### `BindingProfile::rebind(&mut self, action, player, context, source) -> bool`

Переназначает источник существующего действия. `false` — запись не найдена.

```rust
let mut profile = BindingProfile::new();
profile.bind(ProfileBinding::new(MOVE.id, MOVE.name, MOVE.kind, BindingSource::key(Scancode::W), PlayerId(0)));
assert!(profile.rebind(MOVE.id, PlayerId(0), "", BindingSource::key(Scancode::S)));
```

#### `BindingProfile::detect_conflicts(&self) -> Vec<Conflict>`

Конфликты: один источник назначен нескольким действиям одного игрока в
одном контексте. Результат отсортирован по игроку, контексту и источнику.

```rust
let mut profile = BindingProfile::new();
profile.bind(ProfileBinding::new(MOVE.id, MOVE.name, MOVE.kind, BindingSource::key(Scancode::SPACE), PlayerId(0)));
profile.bind(ProfileBinding::new(Uuid::from_u128(9), "jump", ActionKind::Button, BindingSource::key(Scancode::SPACE), PlayerId(0)));
assert_eq!(profile.detect_conflicts().len(), 1);
```

#### `BindingProfile::to_ron(&self) -> Result<String, ron::Error>`

Сериализует профиль в RON.

```rust
let profile = BindingProfile::new();
let text = profile.to_ron().unwrap();
assert!(text.contains("entries"));
```

#### `BindingProfile::from_ron(text) -> Result<Self, ron::de::SpannedError>`

Читает профиль из RON.

```rust
let profile = BindingProfile::new();
let text = profile.to_ron().unwrap();
let restored = BindingProfile::from_ron(&text).unwrap();
assert_eq!(profile, restored);
```

#### `detect_conflicts(profile) -> Vec<Conflict>`

Свободная функция-обёртка над `BindingProfile::detect_conflicts`.

```rust
let profile = BindingProfile::new();
assert!(detect_conflicts(&profile).is_empty());
```

### Intention-снапшот ✅

#### `IntentionActions`

UUID действий, из которых собирается `Intention`: `movement`, `look`,
`jump`, `crouch` (опциональные).

```rust
let actions = IntentionActions {
    movement: MOVE.id,
    look: None,
    jump: None,
    crouch: None,
};
assert_eq!(actions.movement, MOVE.id);
```

#### `IntentionActions::movement_only(movement) -> Self`

Минимальный набор: только движение.

```rust
let actions = IntentionActions::movement_only(MOVE.id);
assert!(actions.jump.is_none());
```

#### `IntentionConfig`

Настройки конвертера: `movement: AxisSettings`, `look: AxisSettings`,
`mouse_sensitivity: [f64; 2]`.

```rust
let config = IntentionConfig::default();
assert_eq!(config.mouse_sensitivity, [0.1, 0.1]);
```

#### `intention_from_values(values, actions, config) -> Intention`

Собирает `Intention` из уже разрешённых действий: применяет кривые движения
и обзора, выставляет `jump`/`crouch`, клампит оси.

```rust
let mut values = std::collections::BTreeMap::new();
values.insert(MOVE.id, ActionValue::Axis2([0.0, 1.0]));
let intention = intention_from_values(&values, IntentionActions::movement_only(MOVE.id), IntentionConfig::default());
assert_eq!(intention.move_axis, [0.0, 1.0]);
```

#### `intention_from_input(map, state, player, actions, config) -> Intention`

Разрешает действия игрока и собирает `Intention`. Если действие обзора не
объявлено, `look` берётся из дельты мыши, умноженной на
`mouse_sensitivity`.

```rust
let mut map = InputMap::new();
map.bind(Binding::new(MOVE, BindingSource::key(Scancode::W), PlayerId(0))
    .with_component(AxisComponent::axis(1, 1.0)));
let mut state = InputState::new();
state.apply(&RawEvent::new(Duration::ZERO, RawEventKind::Key {
    scancode: Scancode::W, pressed: true, repeat: false,
}));
let intention = intention_from_input(
    &map,
    &state,
    PlayerId(0),
    IntentionActions::movement_only(MOVE.id),
    IntentionConfig::default(),
);
assert_eq!(intention.move_axis, [0.0, 1.0]);
```

### Окно и display ✅

#### `WindowId(pub u64)`

Идентификатор окна.

```rust
let id = WindowId(7);
assert_eq!(id.0, 7);
```

#### `WindowMode`

Режим окна: `Windowed` (по умолчанию), `Borderless`, `Exclusive`.

```rust
assert_eq!(WindowMode::default(), WindowMode::Windowed);
```

#### `CloseDecision`

Решение по close-request: `Pending`, `Accept`, `Deny`.

```rust
assert_eq!(CloseDecision::default(), CloseDecision::Pending);
```

#### `CloseRequest::new() -> Self`

Новый неразрешённый запрос на закрытие.

```rust
let request = CloseRequest::new();
assert_eq!(request.decision(), CloseDecision::Pending);
```

#### `CloseRequest::accept(&mut self)`

Принять закрытие.

```rust
let mut request = CloseRequest::new();
request.accept();
assert!(request.is_accepted());
```

#### `CloseRequest::deny(&mut self)`

Отклонить закрытие (например, есть несохранённые данные).

```rust
let mut request = CloseRequest::new();
request.deny();
assert!(request.is_denied());
```

#### `CloseRequest::decision(self) -> CloseDecision`

Текущее решение.

```rust
assert_eq!(CloseRequest::new().decision(), CloseDecision::Pending);
```

#### `CloseRequest::is_accepted(self) -> bool`

Закрытие принято.

```rust
let mut request = CloseRequest::new();
request.accept();
assert!(request.is_accepted());
```

#### `CloseRequest::is_denied(self) -> bool`

Закрытие отклонено.

```rust
let mut request = CloseRequest::new();
request.deny();
assert!(request.is_denied());
```

#### `WindowEvent`

Событие окна: `Resize(PhysicalSize)`, `Focus(bool)`, `DpiChanged(DpiInfo)`,
`CloseRequest(CloseRequest)`, `Moved { x, y }`, `ScaleFactorChanged(f64)`.

```rust
let event = WindowEvent::Focus(true);
assert!(matches!(event, WindowEvent::Focus(true)));
```

#### `WindowEventQueue::new() -> Self`

Пустая очередь событий окна.

```rust
let queue = WindowEventQueue::new();
assert!(queue.is_empty());
```

#### `WindowEventQueue::push(&mut self, event)`

Добавляет событие, коалесцируя resize и DPI: в очереди остаётся только
последнее событие каждого коалесцируемого типа.

```rust
let mut queue = WindowEventQueue::new();
queue.push(WindowEvent::Resize(PhysicalSize::new(800, 600)));
queue.push(WindowEvent::Resize(PhysicalSize::new(1280, 720)));
assert_eq!(queue.len(), 1);
```

#### `WindowEventQueue::events(&self) -> &[WindowEvent]`

События только для чтения.

```rust
let mut queue = WindowEventQueue::new();
queue.push(WindowEvent::Focus(true));
assert_eq!(queue.events().len(), 1);
```

#### `WindowEventQueue::take(&mut self) -> Vec<WindowEvent>`

Забирает события, очищая очередь.

```rust
let mut queue = WindowEventQueue::new();
queue.push(WindowEvent::Focus(true));
assert_eq!(queue.take().len(), 1);
assert!(queue.is_empty());
```

#### `WindowEventQueue::len(&self) -> usize`

Число событий.

```rust
assert_eq!(WindowEventQueue::new().len(), 0);
```

#### `WindowEventQueue::is_empty(&self) -> bool`

Пуста ли очередь.

```rust
assert!(WindowEventQueue::new().is_empty());
```

#### `WindowEventQueue::is_close_requested(&self) -> bool`

Есть ли неразрешённый close-request.

```rust
let mut queue = WindowEventQueue::new();
queue.push(WindowEvent::CloseRequest(CloseRequest::new()));
assert!(queue.is_close_requested());
```

#### `WindowEventQueue::resolve_close(&mut self, decision) -> bool`

Разрешает первый неразрешённый close-request. `false` — запроса нет.

```rust
let mut queue = WindowEventQueue::new();
queue.push(WindowEvent::CloseRequest(CloseRequest::new()));
assert!(queue.resolve_close(CloseDecision::Deny));
assert!(!queue.is_close_requested());
```

#### `WindowMetrics::from_physical(physical, dpi, mode) -> Self`

Метрики окна: физический и логический размер, DPI, режим.

```rust
let metrics = WindowMetrics::from_physical(
    PhysicalSize::new(1280, 720),
    DpiInfo::new(2.0),
    WindowMode::Windowed,
);
assert_eq!(metrics.logical, LogicalSize::new(640.0, 360.0));
```

#### `PhysicalSize::new(width, height) -> Self`

Физический размер в пикселях.

```rust
assert_eq!(PhysicalSize::new(1920, 1080).width, 1920);
```

#### `PhysicalSize::area(self) -> u64`

Площадь в пикселях.

```rust
assert_eq!(PhysicalSize::new(4, 5).area(), 20);
```

#### `PhysicalSize::to_logical(self, dpi) -> LogicalSize`

Перевод в логические единицы с учётом DPI.

```rust
let logical = PhysicalSize::new(1280, 720).to_logical(DpiInfo::new(2.0));
assert_eq!(logical, LogicalSize::new(640.0, 360.0));
```

#### `LogicalSize::new(width, height) -> Self`

Логический (DPI-независимый) размер.

```rust
assert_eq!(LogicalSize::new(640.0, 360.0).height, 360.0);
```

#### `LogicalSize::to_physical(self, dpi) -> PhysicalSize`

Перевод в физические пиксели с округлением к ближайшему.

```rust
let physical = LogicalSize::new(640.0, 360.0).to_physical(DpiInfo::new(2.0));
assert_eq!(physical, PhysicalSize::new(1280, 720));
```

#### `DpiInfo::new(scale_factor) -> Self`

DPI-масштаб с защитой от нуля/NaN (некорректное значение → `1.0`).

```rust
assert_eq!(DpiInfo::new(0.0), DpiInfo::default());
assert_eq!(DpiInfo::new(2.0).scale_factor, 2.0);
```

#### `DpiInfo::physical_to_logical(self, value) -> f64`

Физическое значение → логическое.

```rust
assert_eq!(DpiInfo::new(2.0).physical_to_logical(100.0), 50.0);
```

#### `DpiInfo::logical_to_physical(self, value) -> f64`

Логическое значение → физическое.

```rust
assert_eq!(DpiInfo::new(2.0).logical_to_physical(50.0), 100.0);
```

#### `VideoMode::new(physical, refresh_millihz) -> Self`

Видеорежим монитора; частота — в тысячных долях герца.

```rust
let mode = VideoMode::new(PhysicalSize::new(1920, 1080), 144_000);
assert_eq!(mode.refresh_millihz, 144_000);
```

#### `VideoMode::refresh_hz(self) -> f64`

Частота обновления в герцах.

```rust
let mode = VideoMode::new(PhysicalSize::new(1920, 1080), 144_000);
assert_eq!(mode.refresh_hz(), 144.0);
```

#### `MonitorInfo::new(id, name, physical, dpi, primary) -> Self`

Монитор с одним режимом по умолчанию (60 Гц).

```rust
let monitor = MonitorInfo::new(0, "main", PhysicalSize::new(1920, 1080), DpiInfo::new(1.0), true);
assert_eq!(monitor.modes.len(), 1);
```

#### `MonitorInfo::logical_size(&self) -> LogicalSize`

Логическое разрешение монитора.

```rust
let monitor = MonitorInfo::new(0, "main", PhysicalSize::new(3840, 2160), DpiInfo::new(2.0), true);
assert_eq!(monitor.logical_size(), LogicalSize::new(1920.0, 1080.0));
```

### winit-адаптер (feature `windowing`) ✅

#### `map_key_code(code) -> Option<Scancode>`

Физический код `winit` → `Scancode`. Неизвестные коды — `None`.

```rust
use winit::keyboard::KeyCode;
assert_eq!(map_key_code(KeyCode::KeyW), Some(Scancode::W));
assert_eq!(map_key_code(KeyCode::KeyZ), None);
```

#### `map_mouse_button(button) -> Option<MouseButton>`

Кнопка `winit` → `MouseButton`; `Back`/`Forward` → `Other(3)`/`Other(4)`.

```rust
use winit::event::MouseButton as WinitMouseButton;
assert_eq!(map_mouse_button(WinitMouseButton::Left), Some(MouseButton::Left));
assert_eq!(map_mouse_button(WinitMouseButton::Back), Some(MouseButton::Other(3)));
```

#### `cursor_grab_mode(mode) -> CursorGrabMode`

Режим курсора → `winit` grab mode: `Disabled` → `Locked`, `Confined` →
`Confined`, остальные → `None`.

```rust
use winit::window::CursorGrabMode;
assert_eq!(cursor_grab_mode(CursorMode::Disabled), CursorGrabMode::Locked);
```

#### `apply_cursor_mode(window, mode) -> Result<(), ExternalError>`

Применяет видимость и захват курсора к окну. Ошибка — если захват
недоступен.

```rust
fn grab(window: &winit::window::Window) -> Result<(), winit::error::ExternalError> {
    apply_cursor_mode(window, CursorMode::Disabled)
}
```

#### `WinitInputAdapter::new() -> Self`

Адаптер без сохранённой позиции курсора.

```rust
let adapter = WinitInputAdapter::new();
let _ = adapter;
```

#### `WinitInputAdapter::window_event(&mut self, event) -> Option<WindowEvent>`

Оконное событие `winit` → платформенное; неинтересные события — `None`.

```rust
let mut adapter = WinitInputAdapter::new();
let event = winit::event::WindowEvent::Focused(true);
assert_eq!(adapter.window_event(&event), Some(WindowEvent::Focus(true)));
```

#### `WinitInputAdapter::key_event(&mut self, event, timestamp) -> Option<RawEvent>`

Клавиша → `RawEventKind::Key`; работает по `PhysicalKey`, поэтому раскладка
не влияет.

```rust
fn translate(
    adapter: &mut WinitInputAdapter,
    event: &winit::event::KeyEvent,
) -> Option<RawEvent> {
    adapter.key_event(event, Duration::ZERO)
}
```

#### `WinitInputAdapter::cursor_moved(&mut self, position, timestamp) -> Option<RawEvent>`

Движение курсора → дельта `MouseMotion`. Первое событие даёт нулевую дельту
и `None`.

```rust
let mut adapter = WinitInputAdapter::new();
let position = winit::dpi::PhysicalPosition::new(10.0, 10.0);
assert!(adapter.cursor_moved(position, Duration::ZERO).is_none());
let next = winit::dpi::PhysicalPosition::new(15.0, 8.0);
assert!(adapter.cursor_moved(next, Duration::ZERO).is_some());
```

#### `WinitInputAdapter::mouse_button(&mut self, state, button, timestamp) -> Option<RawEvent>`

Кнопка мыши → `RawEventKind::MouseButton`.

```rust
let mut adapter = WinitInputAdapter::new();
let raw = adapter.mouse_button(
    winit::event::ElementState::Pressed,
    winit::event::MouseButton::Left,
    Duration::ZERO,
).unwrap();
assert!(matches!(raw.kind, RawEventKind::MouseButton { pressed: true, .. }));
```

#### `WinitInputAdapter::mouse_wheel(&mut self, delta, timestamp) -> RawEvent`

Колесо → `RawEventKind::Wheel`; line/pixel delta приводятся к `f64`.

```rust
let mut adapter = WinitInputAdapter::new();
let raw = adapter.mouse_wheel(winit::event::MouseScrollDelta::LineDelta(0.0, 1.0), Duration::ZERO);
assert!(matches!(raw.kind, RawEventKind::Wheel { dy: 1.0, .. }));
```

#### `WinitInputAdapter::ime(&mut self, event, timestamp) -> Option<RawEvent>`

IME → `RawEventKind::Text` только на `Ime::Commit`; `Preedit` отбрасывается.

```rust
let mut adapter = WinitInputAdapter::new();
let raw = adapter.ime(&winit::event::Ime::Commit("a".to_owned()), Duration::ZERO).unwrap();
assert_eq!(raw.kind, RawEventKind::Text("a".to_owned()));
```

### Системные сервисы ✅/🟡

#### `CursorMode::ALL`

Все четыре режима в каноническом порядке: `Normal`, `Hidden`, `Disabled`,
`Confined`.

```rust
assert_eq!(CursorMode::ALL.len(), 4);
assert_eq!(CursorMode::ALL[0], CursorMode::Normal);
```

#### `CursorMode::grabs(self) -> bool`

Захватывает ли режим курсор (`Disabled`/`Confined`).

```rust
assert!(CursorMode::Disabled.grabs());
assert!(!CursorMode::Normal.grabs());
```

#### `CursorMode::is_visible(self) -> bool`

Видим ли курсор (`Normal`).

```rust
assert!(CursorMode::Normal.is_visible());
assert!(!CursorMode::Hidden.is_visible());
```

#### `CursorPosition::new(x, y) -> Self`

Позиция курсора в логических координатах окна.

```rust
let position = CursorPosition::new(10.0, 20.0);
assert_eq!(position.x, 10.0);
```

#### `CursorPosition::clamped(self, logical) -> Self`

Ограничивает позицию логическим размером окна.

```rust
let position = CursorPosition::new(-5.0, 200.0)
    .clamped(LogicalSize::new(100.0, 50.0));
assert_eq!(position, CursorPosition::new(0.0, 50.0));
```

#### `Clipboard::get_text(&self) -> Option<String>`

Текст из буфера обмена.

```rust
let mut clipboard = MemoryClipboard::new();
clipboard.set_text("flux");
assert_eq!(clipboard.get_text().as_deref(), Some("flux"));
```

#### `Clipboard::set_text(&mut self, text)`

Записывает текст в буфер.

```rust
let mut clipboard = MemoryClipboard::new();
clipboard.set_text("flux");
assert_eq!(clipboard.as_str(), Some("flux"));
```

#### `Clipboard::clear(&mut self)`

Очищает буфер.

```rust
let mut clipboard = MemoryClipboard::new();
clipboard.set_text("flux");
clipboard.clear();
assert_eq!(clipboard.get_text(), None);
```

#### `MemoryClipboard::new() -> Self`

Буфер в памяти для headless/тестов.

```rust
let clipboard = MemoryClipboard::new();
assert!(clipboard.as_str().is_none());
```

#### `MemoryClipboard::as_str(&self) -> Option<&str>`

Текст без копирования.

```rust
let mut clipboard = MemoryClipboard::new();
clipboard.set_text("abc");
assert_eq!(clipboard.as_str(), Some("abc"));
```

#### `ImeEvent`

Событие IME-композиции: `Start`, `Update { text, cursor }`, `End`.

```rust
let event = ImeEvent::Update { text: "に".to_owned(), cursor: None };
assert!(matches!(event, ImeEvent::Update { .. }));
```

#### `TextEvent`

Текстовое событие: `Ime(ImeEvent)` или `Commit(String)`.

```rust
let event = TextEvent::Commit("abc".to_owned());
assert_eq!(event, TextEvent::Commit("abc".to_owned()));
```

#### `TextInput::new() -> Self`

Пустой аккумулятор текста.

```rust
let input = TextInput::new();
assert_eq!(input.committed(), "");
```

#### `TextInput::apply(&mut self, event)`

Применяет событие: композиция не попадает в committed до `End`; `Commit`
добавляет текст сразу.

```rust
let mut input = TextInput::new();
input.apply(TextEvent::Commit("ab".to_owned()));
assert_eq!(input.committed(), "ab");
```

#### `TextInput::composition(&self) -> Option<&str>`

Текущая композиция.

```rust
let mut input = TextInput::new();
input.apply(TextEvent::Ime(ImeEvent::Start));
input.apply(TextEvent::Ime(ImeEvent::Update { text: "a".to_owned(), cursor: None }));
assert_eq!(input.composition(), Some("a"));
```

#### `TextInput::is_composing(&self) -> bool`

Идёт ли композиция.

```rust
let mut input = TextInput::new();
input.apply(TextEvent::Ime(ImeEvent::Start));
assert!(input.is_composing());
```

#### `TextInput::committed(&self) -> &str`

Подтверждённый текст.

```rust
let mut input = TextInput::new();
input.apply(TextEvent::Commit("abc".to_owned()));
assert_eq!(input.committed(), "abc");
```

#### `TextInput::take_committed(&mut self) -> String`

Забирает подтверждённый текст, очищая аккумулятор.

```rust
let mut input = TextInput::new();
input.apply(TextEvent::Commit("abc".to_owned()));
assert_eq!(input.take_committed(), "abc");
assert_eq!(input.committed(), "");
```

#### `TextInput::cancel_composition(&mut self)`

Отменяет композицию без фиксации.

```rust
let mut input = TextInput::new();
input.apply(TextEvent::Ime(ImeEvent::Start));
input.cancel_composition();
assert!(!input.is_composing());
```

#### `ProjectPaths::from_root(root) -> Self`

Каталоги проекта относительно корня: `assets`, `saves`, `logs`,
`crash-dumps`.

```rust
let paths = ProjectPaths::from_root("game");
assert_eq!(paths.saves, std::path::PathBuf::from("game/saves"));
```

#### `ProjectPaths::directories(&self) -> [&Path; 5]`

Все каталоги в каноническом порядке.

```rust
let paths = ProjectPaths::from_root("game");
assert_eq!(paths.directories().len(), 5);
```

#### `ProjectPaths::ensure(&self) -> io::Result<()>`

Создаёт каталоги идемпотентно.

```rust
let paths = ProjectPaths::from_root("target/test-paths");
paths.ensure().unwrap();
assert!(paths.assets.exists());
```

#### `ProjectPaths::save_file(&self, name) -> PathBuf`

Путь к файлу сохранения.

```rust
let paths = ProjectPaths::from_root("game");
assert_eq!(paths.save_file("slot1.ron"), std::path::PathBuf::from("game/saves/slot1.ron"));
```

#### `ProjectPaths::log_file(&self, name) -> PathBuf`

Путь к файлу лога.

```rust
let paths = ProjectPaths::from_root("game");
assert_eq!(paths.log_file("run.log"), std::path::PathBuf::from("game/logs/run.log"));
```

#### `ThermalPressure`

Тепловое давление ОС: `Nominal`, `Fair`, `Serious`, `Critical`.

```rust
assert_eq!(ThermalPressure::default(), ThermalPressure::Nominal);
```

#### `PowerInfo::should_throttle(self) -> bool`

Нужно ли снижать нагрузку: батарея ≤ 20% или давление `Serious`/`Critical`.

```rust
let low = PowerInfo { on_battery: true, battery_percent: Some(10), ..PowerInfo::default() };
assert!(low.should_throttle());
assert!(!PowerInfo::default().should_throttle());
```

#### `PowerEvent`

Событие питания: `Suspending`, `Resumed`, `BatteryLow`,
`ThermalChanged(ThermalPressure)`.

```rust
let event = PowerEvent::ThermalChanged(ThermalPressure::Serious);
assert!(matches!(event, PowerEvent::ThermalChanged(_)));
```

#### `GamepadInfo`

Подключённый геймпад: `{ id, name, vendor, product }`.

```rust
let info = GamepadInfo { id: 0, name: "pad".to_owned(), vendor: 0, product: 0 };
assert_eq!(info.id, 0);
```

#### `DeviceEvent`

Событие hot-plug: `Connected(GamepadInfo)` или `Disconnected(u16)`.

```rust
let event = DeviceEvent::Disconnected(0);
assert_eq!(event, DeviceEvent::Disconnected(0));
```

#### `DeviceRegistry::new() -> Self`

Пустой реестр геймпадов.

```rust
let registry = DeviceRegistry::new();
assert!(registry.is_empty());
```

#### `DeviceRegistry::apply(&mut self, event)`

Применяет событие hot-plug.

```rust
let mut registry = DeviceRegistry::new();
registry.apply(&DeviceEvent::Connected(GamepadInfo {
    id: 1, name: "pad".to_owned(), vendor: 0, product: 0,
}));
assert!(registry.is_connected(1));
```

#### `DeviceRegistry::connected(&self) -> impl Iterator<Item = &GamepadInfo>`

Подключённые устройства в детерминированном порядке `id`.

```rust
let mut registry = DeviceRegistry::new();
registry.apply(&DeviceEvent::Connected(GamepadInfo { id: 2, name: "b".to_owned(), vendor: 0, product: 0 }));
registry.apply(&DeviceEvent::Connected(GamepadInfo { id: 1, name: "a".to_owned(), vendor: 0, product: 0 }));
let ids: Vec<u16> = registry.connected().map(|info| info.id).collect();
assert_eq!(ids, vec![1, 2]);
```

#### `DeviceRegistry::is_connected(&self, id) -> bool`

Подключён ли геймпад.

```rust
let mut registry = DeviceRegistry::new();
registry.apply(&DeviceEvent::Connected(GamepadInfo { id: 1, name: "pad".to_owned(), vendor: 0, product: 0 }));
assert!(registry.is_connected(1));
```

#### `DeviceRegistry::len(&self) -> usize`

Число подключённых устройств.

```rust
let registry = DeviceRegistry::new();
assert_eq!(registry.len(), 0);
```

#### `DeviceRegistry::is_empty(&self) -> bool`

Нет ли устройств.

```rust
assert!(DeviceRegistry::new().is_empty());
```

#### `gilrs_backend::poll(gilrs) -> Vec<DeviceEvent>`

Опрашивает `gilrs` и возвращает hot-plug события. Требует feature
`devices`.

```rust
let mut gilrs = gilrs::Gilrs::new().unwrap();
let events = gilrs_backend::poll(&mut gilrs);
let _ = events;
```

#### `gilrs_backend::poll_registry(gilrs, registry)`

Опрашивает `gilrs` и применяет hot-plug к реестру.

```rust
let mut gilrs = gilrs::Gilrs::new().unwrap();
let mut registry = DeviceRegistry::new();
gilrs_backend::poll_registry(&mut gilrs, &mut registry);
```

#### `CrashConfig::new(dump_dir) -> Self`

Конфигурация каталога краш-дампов.

```rust
let config = CrashConfig::new("logs/dumps");
assert_eq!(config.dump_dir, std::path::PathBuf::from("logs/dumps"));
```

#### `CrashConfig::default() -> Self`

Каталог по умолчанию — `crash-dumps`.

```rust
assert_eq!(CrashConfig::default().dump_dir, std::path::PathBuf::from("crash-dumps"));
```

#### `install(config)`

Ставит паник-хук с записью текстового дампа и breadcrumbs.

```rust
install(&CrashConfig::default());
```

#### `native` (feature `crash-dump`) 🟡

Модуль-заглушка для нативных per-OS дампов. Функций пока нет: `install`
пишет текстовый дамп через `flux-core`; нативные дампы требуют отдельного
ADR (`windows`-крейт и `unsafe`).

```rust
// Нативного API пока нет — модуль существует только как точка расширения.
```

## flux-physics

Collision-минимум и KCC-минимум на `f64`. Крейт не знает про ECS.

Импорт:

```rust
use flux_core::input::Intention;
use flux_physics::{
    move_character, Capsule, CharacterConfig, CharacterState, Contact, DVec3, MoveResult, Ray,
    Tile, TileGrid,
};
```

### Collision ✅

#### `Capsule::new(radius, half_height) -> Self`

Вертикальная капсула: радиус и половина высоты цилиндрической части.

```rust
let capsule = Capsule::new(0.3, 0.9);
assert_eq!(capsule.radius, 0.3);
```

#### `Capsule::aabb(self, position) -> (DVec3, DVec3)`

Мировой AABB капсулы в позиции: `(min, max)`.

```rust
let capsule = Capsule::new(0.5, 1.0);
let (min, max) = capsule.aabb(DVec3::ZERO);
assert_eq!(max.y, 1.5);
assert_eq!(min.x, -0.5);
```

#### `Ray::new(origin, direction, max_distance) -> Self`

Луч; направление нормализуется при запросе.

```rust
let ray = Ray::new(DVec3::ZERO, DVec3::X, 10.0);
assert_eq!(ray.max_distance, 10.0);
```

#### `Contact`

Контакт свипа/луча: `{ time, normal, point }`. Для `sweep_capsule`
`time` — доля `0..1`, для `raycast` — дистанция.

```rust
let contact = Contact { time: 0.5, normal: DVec3::Y, point: DVec3::ZERO };
assert_eq!(contact.time, 0.5);
```

#### `Tile`

Ячейка тайлового grid: `Empty` или `Solid`.

```rust
assert_ne!(Tile::Empty, Tile::Solid);
```

#### `TileGrid::from_rows(rows) -> Self`

Строит grid из ASCII-строк: `#` — твёрдая ячейка, остальное — пусто. Первая
строка — верхний ряд, `y = 0` — нижний.

```rust
let grid = TileGrid::from_rows(&["..", "##"]);
assert!(grid.is_solid(0, 0));
assert!(!grid.is_solid(0, 1));
```

#### `TileGrid::from_rows_with_tile_size(rows, tile_size) -> Self`

То же, но с размером ячейки в метрах. Некорректный размер → `1.0`.

```rust
let grid = TileGrid::from_rows_with_tile_size(&["#"], 2.0);
assert_eq!(grid.tile_size(), 2.0);
```

#### `TileGrid::width(&self) -> usize`

Ширина в ячейках.

```rust
assert_eq!(TileGrid::from_rows(&["###"]).width(), 3);
```

#### `TileGrid::height(&self) -> usize`

Высота в ячейках.

```rust
assert_eq!(TileGrid::from_rows(&["#", "#"]).height(), 2);
```

#### `TileGrid::tile_size(&self) -> f64`

Размер ячейки.

```rust
assert_eq!(TileGrid::from_rows(&["#"]).tile_size(), 1.0);
```

#### `TileGrid::is_empty(&self) -> bool`

Пуст ли grid.

```rust
assert!(TileGrid::from_rows(&[]).is_empty());
```

#### `TileGrid::tile(&self, x, y) -> Option<Tile>`

Ячейка по индексам; вне границ — `None`.

```rust
let grid = TileGrid::from_rows(&["#"]);
assert_eq!(grid.tile(0, 0), Some(Tile::Solid));
assert_eq!(grid.tile(1, 0), None);
```

#### `TileGrid::is_solid(&self, x, y) -> bool`

Твёрдая ли ячейка.

```rust
let grid = TileGrid::from_rows(&["#"]);
assert!(grid.is_solid(0, 0));
```

#### `TileGrid::cell_of(&self, point) -> Option<(usize, usize)>`

Ячейка, содержащая точку мира; вне grid — `None`.

```rust
let grid = TileGrid::from_rows(&["..", ".."]);
assert_eq!(grid.cell_of(DVec3::new(1.5, 0.5, 0.0)), Some((1, 0)));
```

#### `TileGrid::is_solid_at(&self, point) -> bool`

Твёрдая ли точка мира.

```rust
let grid = TileGrid::from_rows(&["#", "."]);
assert!(!grid.is_solid_at(DVec3::new(0.5, 0.5, 0.0)));
assert!(grid.is_solid_at(DVec3::new(0.5, 1.5, 0.0)));
```

#### `TileGrid::cell_bounds(&self, x, y) -> Option<(DVec3, DVec3)>`

Границы ячейки в мире: `(min, max)`.

```rust
let grid = TileGrid::from_rows_with_tile_size(&["#"], 2.0);
let (min, max) = grid.cell_bounds(0, 0).unwrap();
assert_eq!(min, DVec3::ZERO);
assert_eq!(max, DVec3::new(2.0, 2.0, 0.0));
```

#### `TileGrid::sweep_capsule(&self, capsule, from, to) -> Option<Contact>`

Свип вертикальной капсулы; первый контакт, при равном времени — меньший
индекс `(y, x)`. Использует зазор `SKIN = 1e-4`.

```rust
let grid = TileGrid::from_rows(&["..#", "..#"]);
let capsule = Capsule::new(0.4, 0.5);
let contact = grid.sweep_capsule(&capsule, DVec3::new(1.0, 1.0, 0.0), DVec3::new(3.0, 1.0, 0.0));
assert!(contact.is_some());
```

#### `TileGrid::raycast(&self, ray) -> Option<Contact>`

Луч по тайлам; `Contact::time` — дистанция вдоль нормализованного луча.

```rust
let grid = TileGrid::from_rows(&["#", "."]);
let ray = Ray::new(DVec3::new(0.5, 3.0, 0.0), DVec3::new(0.0, -1.0, 0.0), 10.0);
let contact = grid.raycast(&ray).unwrap();
assert!((contact.time - 1.0).abs() < 1e-9);
```

### KCC ✅

#### `CharacterState`

Состояние персонажа: `{ position, velocity, grounded, ground_normal,
ground_hysteresis }`. `Default` — в начале координат, не на земле, нормаль
`Y`.

```rust
let state = CharacterState::default();
assert!(!state.grounded);
assert_eq!(state.ground_normal, DVec3::Y);
```

#### `CharacterConfig`

Параметры персонажа: радиус/полувысота капсулы, скорости, ускорения,
гравитация, прыжок, предельная скорость падения, `step_height`,
`slope_limit_cos`, гистерезис, глубина пробы пола, множитель приседа.

```rust
let config = CharacterConfig::default();
assert_eq!(config.radius, 0.3);
assert_eq!(config.step_height, 0.3);
```

#### `MoveResult`

Результат шага KCC: `{ state: CharacterState, hit: bool }`.

```rust
let result = MoveResult { state: CharacterState::default(), hit: false };
assert!(!result.hit);
```

#### `move_character(state, config, intention, dt, grid) -> MoveResult`

Чистая функция шага KCC: горизонтальное ускорение, гравитация, прыжок,
скольжение, step-up по `step_height`, ограничение склона по
`slope_limit_cos`, ground-гистерезис, присед. Не аллоцирует и не знает про
ECS.

```rust
let grid = TileGrid::from_rows(&["....", "####"]);
let config = CharacterConfig::default();
let state = CharacterState {
    position: DVec3::new(1.0, 1.0 + config.half_height + config.radius, 0.0),
    ..CharacterState::default()
};
let result = move_character(&state, &config, &Intention::IDLE, 1.0 / 60.0, &grid);
assert!(result.state.position.y <= state.position.y);
```

## flux-graphics

RHI-контракты, headless-null backend (I0), Vulkan (I1), Transfer/ресурсы/бюджет
(I2), Render Graph (I3), Camera/RenderScene (I4), PBR/G-buffer/tonemap (I5),
шейдерные пермутации (I6), GraphicsSettings v1 (I7), golden-image-харнесс
(I8) и DXC-путь/query pools (I6b). Vulkan/DX12 — за feature-гейтами
`vulkan`/`dx12`; по умолчанию выключены, `Null` доступен всегда.

Импорт:

```rust
use flux_core::memory::MemoryTag;
use flux_graphics::{
    create_device, AliasSlot, BackendKind, BarrierBatch, BindingDecl, BudgetSnapshot, BufferId,
    BufferUsage, CacheStats, Camera, Capabilities, CommandBuffer, CompiledPass,
    DeterministicCompiler, DeviceSignature, FeatureKind, GbufferLayout, GoldenBaseline, GoldenError,
    GoldenSettings, GoldenStore, GoldenVerdict, GpuBufferDesc, GpuImageDesc, GpuProfiler, GpuZone,
    GraphError, GraphImage, GraphImageKey, GraphPass, GraphicsFeature, GraphicsFeatureRegistry,
    GraphicsSettings, HardwareTier, ImageBarrier, ImageId, ImageState, InFlightBatch, KeywordDecl,
    KeywordMask, KeywordSet, Material, NullCommandBuffer, NullDevice, NullVramAllocator, PassKind,
    PermutationCache, PermutationRequest, PipelineCacheFile, PipelineCacheLoad, PipelineDesc,
    PipelineId, PipelineKind, PipelineVariant, PresentMode, Pressure, PrimitiveTopology,
    QualityPreset, QueryPoolDesc, QueryPoolId, RenderGraph, RenderGraphPlan, RenderPassDesc,
    RenderScene, ResolvedShader, ResourceEntry, ResourceKey, ResourceRegistry, ResourceState,
    Rgba8Image, RhiDevice, RhiError, SettingsChange, ShaderCompiler, ShaderDeclaration, ShaderError,
    ShaderLibrary, ShaderStage, SurfaceId, SurfaceKind, SurfaceTarget, Swapchain, SwapchainDesc,
    SwapchainId, TextureFormat, TextureUsage, Tonemap, TransferSystem, UploadJob, VertexInput,
    VramAllocator, VramBudget, VramLease, WindowSurface, MAX_KEYWORDS, MAX_MATERIAL_KEYWORDS,
    MAX_RENDER_SCALE, MAX_SHADOW_RESOLUTION, MIN_RENDER_SCALE, MIN_SHADOW_RESOLUTION,
    VERTEX_FORMAT_V1,
};
```

### Бэкенд и форматы ✅

#### `BackendKind`

Бэкенд: `Null` (по умолчанию), `Vulkan`, `Dx12`.

```rust
assert_eq!(BackendKind::default(), BackendKind::Null);
assert_eq!(BackendKind::Vulkan.name(), "vulkan");
```

#### `BackendKind::name(self) -> &'static str`

Имя бэкенда для логов.

```rust
assert_eq!(BackendKind::Null.name(), "null");
assert_eq!(BackendKind::Dx12.name(), "dx12");
```

#### `PresentMode`

Режим представления: `Fifo` (по умолчанию), `Mailbox`, `Immediate`.

```rust
assert_eq!(PresentMode::default(), PresentMode::Fifo);
```

#### `TextureFormat`

Формат: `Rgba8Srgb`, `Rgba8Unorm`, `Bgra8Srgb`, `Rgba16Float`, `R8Unorm`,
`D32Float`.

```rust
assert_ne!(TextureFormat::Rgba8Srgb, TextureFormat::D32Float);
```

#### `TextureFormat::bytes_per_pixel(self) -> u32`

Байт на пиксель без сжатия.

```rust
assert_eq!(TextureFormat::Rgba8Srgb.bytes_per_pixel(), 4);
assert_eq!(TextureFormat::Rgba16Float.bytes_per_pixel(), 8);
assert_eq!(TextureFormat::R8Unorm.bytes_per_pixel(), 1);
```

#### `TextureFormat::is_depth(self) -> bool`

Глубинный ли формат.

```rust
assert!(TextureFormat::D32Float.is_depth());
assert!(!TextureFormat::Rgba8Srgb.is_depth());
```

#### `TextureFormat::is_srgb(self) -> bool`

sRGB ли формат.

```rust
assert!(TextureFormat::Rgba8Srgb.is_srgb());
assert!(!TextureFormat::Rgba8Unorm.is_srgb());
```

#### `BufferUsage::vertex() -> Self`

Вершинный буфер.

```rust
let usage = BufferUsage::vertex();
assert!(usage.vertex && usage.any());
```

#### `BufferUsage::index() -> Self`

Индексный буфер.

```rust
assert!(BufferUsage::index().index);
```

#### `BufferUsage::uniform() -> Self`

Uniform-буфер.

```rust
assert!(BufferUsage::uniform().uniform);
```

#### `BufferUsage::storage() -> Self`

Storage-буфер.

```rust
assert!(BufferUsage::storage().storage);
```

#### `BufferUsage::staging() -> Self`

Staging-буфер: источник и приёмник копирования.

```rust
let usage = BufferUsage::staging();
assert!(usage.transfer_src && usage.transfer_dst);
```

#### `BufferUsage::none() -> Self`

Ни одного флага.

```rust
assert!(!BufferUsage::none().any());
```

#### `BufferUsage::any(self) -> bool`

Есть ли хотя бы один флаг.

```rust
assert!(BufferUsage::vertex().any());
assert!(!BufferUsage::default().any());
```

#### `TextureUsage::sampled() -> Self`

Sampled-текстура.

```rust
assert!(TextureUsage::sampled().sampled);
```

#### `TextureUsage::render_target() -> Self`

Цветовой render target.

```rust
assert!(TextureUsage::render_target().render_target);
```

#### `TextureUsage::depth_stencil() -> Self`

Depth/stencil target.

```rust
assert!(TextureUsage::depth_stencil().depth_stencil);
```

#### `TextureUsage::storage() -> Self`

Storage-изображение.

```rust
assert!(TextureUsage::storage().storage);
```

#### `TextureUsage::staging() -> Self`

Staging-изображение.

```rust
assert!(TextureUsage::staging().transfer_dst);
```

#### `TextureUsage::none() -> Self`

Ни одного флага.

```rust
assert!(!TextureUsage::none().any());
```

#### `TextureUsage::any(self) -> bool`

Есть ли хотя бы один флаг.

```rust
assert!(TextureUsage::sampled().any());
```

### Хэндлы и дескрипторы ✅

#### `BufferId(pub u64)`

Хэндл GPU-буфера.

```rust
assert_eq!(BufferId(7).0, 7);
```

#### `ImageId(pub u64)`

Хэндл GPU-изображения.

```rust
assert_eq!(ImageId(3).0, 3);
```

#### `PipelineId(pub u64)`

Хэндл pipeline state object.

```rust
assert_eq!(PipelineId(1).0, 1);
```

#### `SwapchainId(pub u64)`

Хэндл swapchain.

```rust
assert_eq!(SwapchainId(0).0, 0);
```

#### `SurfaceId(pub u64)`

Хэндл поверхности представления (`VkSurfaceKHR`).

```rust
assert_eq!(SurfaceId(2).0, 2);
```

#### `GpuBufferDesc::new(label, size, usage) -> Self`

Дескриптор GPU-буфера.

```rust
let desc = GpuBufferDesc::new("vertex", 1024, BufferUsage::vertex());
assert_eq!(desc.size, 1024);
```

#### `GpuImageDesc::new(label, width, height, format) -> Self`

Дескриптор изображения: usage — `sampled()`, mips — 1.

```rust
let desc = GpuImageDesc::new("albedo", 1920, 1080, TextureFormat::Rgba8Srgb);
assert_eq!(desc.mip_levels, 1);
```

#### `GpuImageDesc::with_usage(self, usage) -> Self`

Задаёт использование изображения.

```rust
let desc = GpuImageDesc::new("rt", 1280, 720, TextureFormat::Rgba16Float)
    .with_usage(TextureUsage::render_target());
assert!(desc.usage.render_target);
```

#### `GpuImageDesc::with_mips(self, mip_levels) -> Self`

Задаёт число mip-уровней.

```rust
let desc = GpuImageDesc::new("tex", 256, 256, TextureFormat::Rgba8Srgb).with_mips(8);
assert_eq!(desc.mip_levels, 8);
```

#### `SwapchainDesc::new(label, width, height) -> Self`

Swapchain: `Bgra8Srgb`, `Fifo`, 2 кадра в полёте.

```rust
let desc = SwapchainDesc::new("main", 1280, 720);
assert_eq!(desc.frames_in_flight, 2);
```

#### `SwapchainDesc::with_format(self, format) -> Self`

Задаёт формат изображений.

```rust
let desc = SwapchainDesc::new("main", 1280, 720).with_format(TextureFormat::Rgba8Srgb);
assert_eq!(desc.format, TextureFormat::Rgba8Srgb);
```

#### `SwapchainDesc::with_present_mode(self, mode) -> Self`

Задаёт режим представления.

```rust
let desc = SwapchainDesc::new("main", 1280, 720).with_present_mode(PresentMode::Mailbox);
assert_eq!(desc.present_mode, PresentMode::Mailbox);
```

#### `SwapchainDesc::with_frames_in_flight(self, frames) -> Self`

Задаёт число кадров в полёте.

```rust
let desc = SwapchainDesc::new("main", 1280, 720).with_frames_in_flight(3);
assert_eq!(desc.frames_in_flight, 3);
```

#### `SwapchainDesc::with_surface(self, surface) -> Self`

Привязывает swapchain к поверхности представления: вместо offscreen-таргета
создаётся настоящий `VkSwapchainKHR` (present через `acquire`/`present`).
Без поверхности swapchain остаётся offscreen.

```rust
let desc = SwapchainDesc::new("main", 1280, 720).with_surface(SurfaceId(0));
assert_eq!(desc.surface, Some(SurfaceId(0)));
assert_eq!(SwapchainDesc::new("offscreen", 64, 64).surface, None);
```

#### `QueryPoolDesc::new(label, count) -> Self`

Пул GPU-запросов (таймстемпы); `count` — `1..=4096`. Хэндл — `QueryPoolId`.

```rust
let pool = QueryPoolDesc::new("zones", 16);
assert_eq!(pool.count, 16);
```

#### `PipelineKind`

Тип pipeline: `Graphics` или `Compute`.

```rust
assert_ne!(PipelineKind::Graphics, PipelineKind::Compute);
```

#### `PrimitiveTopology`

Топология примитивов: `TriangleList` (по умолчанию), `LineList`, `PointList`.

```rust
assert_eq!(PrimitiveTopology::default(), PrimitiveTopology::TriangleList);
```

#### `PipelineDesc::new(label, kind) -> Self`

Дескриптор pipeline без шейдеров — для null-бэкенда и контрактных тестов.
Значения по умолчанию: entry points `vs_main`/`fs_main`/`cs_main`, пустые
форматы, `vertex_stride = 12` (`f32x3`-позиция), `TriangleList`.

```rust
let desc = PipelineDesc::new("pbr", PipelineKind::Graphics);
assert_eq!(desc.kind, PipelineKind::Graphics);
assert_eq!(desc.vertex_entry, "vs_main");
assert_eq!(desc.vertex_stride, 12);
```

#### `PipelineDesc::graphics(label, vertex_shader, fragment_shader) -> Self`

Графический pipeline из SPIR-V вершинного и фрагментного шейдеров.

Закоммиченные артефакты лежат в `crates/flux-graphics/shaders/compiled/`:
`fullscreen.vert.spv` (полноэкранный треугольник по `SV_VertexID`) и
`solid.frag.spv` (константный цвет). Исходники — `shaders/*.hlsl`, метаданные
пермутаций — `shaders/solid.shader.json`.

#### `PipelineDesc::compute(label, compute_shader) -> Self`

Compute pipeline из SPIR-V.

#### Билдеры `PipelineDesc`

`with_color_formats(&'static [TextureFormat])`,
`with_depth_format(TextureFormat)`, `with_vertex_stride(u32)`,
`with_topology(PrimitiveTopology)`,
`with_bindings(&'static [DescriptorBinding])`,
`with_entries(vertex_entry, fragment_entry)`.

`with_bindings` описывает set 0 (ADR-0039): layout кэшируется по составу
биндингов, `bind_descriptor_set` требует совместимый pipeline.

`with_vertex_stride(0)` означает pipeline без vertex buffer: Vulkan не
объявляет vertex input, шейдер обязан строить вершины из `SV_VertexID`
(полноэкранный треугольник, ADR-0035).

```rust
let desc = PipelineDesc::graphics("gbuffer", &[], &[])
    .with_color_formats(&[TextureFormat::Rgba8Srgb, TextureFormat::Rgba16Float])
    .with_depth_format(TextureFormat::D32Float)
    .with_topology(PrimitiveTopology::TriangleList);
assert_eq!(desc.color_formats.len(), 2);
assert_eq!(desc.depth_format, Some(TextureFormat::D32Float));
```

Контракт SPIR-V ABI (entry points, vertex layout, форматы) зафиксирован
в ADR-0035. Интеграционный тест `tests/vulkan_pso.rs` компилирует PSO из
закоммиченных SPIR-V, рисует полноэкранный треугольник в offscreen-цель и
проверяет, что число validation-ошибок не изменилось; на машине без Vulkan
тест завершается успешно (headless-CI).

#### `RenderPassDesc::new(label) -> Self`

Пустой render pass: без аттачментов, clear color `[0,0,0,1]`.

```rust
let pass = RenderPassDesc::new("main");
assert!(!pass.has_attachments());
```

#### `RenderPassDesc::with_color(self, image) -> Self`

Добавляет цветовой аттачмент.

```rust
let pass = RenderPassDesc::new("main").with_color(ImageId(1));
assert_eq!(pass.color_attachments, vec![ImageId(1)]);
```

#### `RenderPassDesc::with_depth(self, image) -> Self`

Задаёт depth-аттачмент.

```rust
let pass = RenderPassDesc::new("main").with_depth(ImageId(2));
assert_eq!(pass.depth_attachment, Some(ImageId(2)));
```

#### `RenderPassDesc::with_clear(self, color) -> Self`

Задаёт цвет очистки.

```rust
let pass = RenderPassDesc::new("main").with_clear([0.1, 0.2, 0.3, 1.0]);
assert_eq!(pass.clear_color, [0.1, 0.2, 0.3, 1.0]);
```

#### `RenderPassDesc::has_attachments(&self) -> bool`

Есть ли хотя бы один аттачмент.

```rust
let pass = RenderPassDesc::new("main").with_depth(ImageId(1));
assert!(pass.has_attachments());
```

### Capabilities ✅

#### `Capabilities::headless() -> Self`

Консервативные возможности null-бэкенда: 16384 px, 2 кадра, 65536
bindless-слотов, `Fifo`, `Rgba8Srgb`/`Bgra8Srgb`, `D32Float`.

```rust
let caps = Capabilities::headless();
assert_eq!(caps.backend, BackendKind::Null);
assert_eq!(caps.max_texture_dimension_2d, 16_384);
```

#### `Capabilities::supports_present_mode(&self, mode) -> bool`

Поддерживается ли режим представления.

```rust
let caps = Capabilities::headless();
assert!(caps.supports_present_mode(PresentMode::Fifo));
assert!(!caps.supports_present_mode(PresentMode::Mailbox));
```

#### `Capabilities::supports_color_format(&self, format) -> bool`

Поддерживается ли цветовой формат.

```rust
assert!(Capabilities::headless().supports_color_format(TextureFormat::Rgba8Srgb));
```

#### `Capabilities::supports_depth_format(&self, format) -> bool`

Поддерживается ли глубинный формат.

```rust
assert!(Capabilities::headless().supports_depth_format(TextureFormat::D32Float));
```

### Поверхности представления ✅

RHI не зависит от оконной библиотеки: поверхность описывается стандартными
`raw-window-handle` 0.6 дескрипторами (`WindowSurface`) либо headless-таргетом
(`SurfaceTarget::Headless`, `VK_EXT_headless_surface`) для CI/lavapipe.
Поверхность создаётся на устройстве и передаётся в
`SwapchainDesc::with_surface`; без неё swapchain остаётся offscreen.
Владелец окна держит его живым, пока живёт поверхность.

#### `WindowSurface`

Сырые дескрипторы окна и дисплея (`RawWindowHandle`/`RawDisplayHandle`);
поля публичные — `window`, `display`.

#### `WindowSurface::new(window, display) -> Self`

Поверхность из готовых сырых дескрипторов.

#### `WindowSurface::from_provider(provider) -> Result<Self, RhiError>`

Берёт дескрипторы у провайдера `HasWindowHandle + HasDisplayHandle`
(например, `winit::Window`). `RhiError::Surface` — провайдер не отдал
дескриптор окна или дисплея.

#### `SurfaceTarget`

Источник поверхности: `Headless` (CI/тесты) или `Window(WindowSurface)`.

#### `SurfaceTarget::window(provider) -> Result<Self, RhiError>`

Оконный таргет из `raw-window-handle`-провайдера.

#### `SurfaceTarget::kind(&self) -> SurfaceKind`

Вид таргета без платформенных указателей.

#### `SurfaceKind`

`Headless` или `Window`; `Serialize`/`Deserialize` — для диагностики,
конфигов и CI-отчётов.

```rust
let mut device = NullDevice::new();
let surface = device.create_surface(&SurfaceTarget::Headless).unwrap();
assert_eq!(device.surface_kind(surface), Some(SurfaceKind::Headless));
assert_eq!(SurfaceTarget::Headless.kind(), SurfaceKind::Headless);
```

### RhiError ✅

#### `RhiError::Unsupported { feature }`

Фича/бэкенд ещё не реализованы.

```rust
let error = RhiError::Unsupported { feature: "vulkan backend (I1)" };
assert!(error.to_string().contains("unsupported"));
```

#### `RhiError::OutOfMemory { requested }`

Не хватает VRAM.

```rust
let error = RhiError::OutOfMemory { requested: 1024 };
assert!(error.to_string().contains("out of memory"));
```

#### `RhiError::DeviceLost`

Устройство потеряно.

```rust
assert_eq!(RhiError::DeviceLost.to_string(), "device lost");
```

#### `RhiError::InvalidHandle { kind, id }`

Хэндл не принадлежит устройству или уже уничтожен.

```rust
let error = RhiError::InvalidHandle { kind: "buffer", id: 3 };
assert!(error.to_string().contains("invalid handle"));
```

#### `RhiError::Validation(String)`

Нарушен контракт вызова или дескриптора.

```rust
let error = RhiError::validation("zero size");
assert!(matches!(error, RhiError::Validation(_)));
```

#### `RhiError::Surface(String)`

Ошибка поверхности/окна.

```rust
let error = RhiError::Surface("window closed".to_owned());
assert!(error.to_string().contains("surface"));
```

#### `RhiError::validation(message) -> Self`

Удобный конструктор `Validation`.

```rust
assert_eq!(RhiError::validation("x"), RhiError::Validation("x".to_owned()));
```

### RhiDevice ✅

#### `RhiDevice::backend(&self) -> BackendKind`

Бэкенд устройства.

```rust
let device = NullDevice::new();
assert_eq!(device.backend(), BackendKind::Null);
```

#### `RhiDevice::capabilities(&self) -> &Capabilities`

Возможности устройства.

```rust
let device = NullDevice::new();
assert_eq!(device.capabilities().backend, BackendKind::Null);
```

#### `RhiDevice::device_signature(&self) -> DeviceSignature`

Подпись устройства для инвалидации дисковых кэшей (PSO): бэкенд, имя GPU и
версия API. `Null` возвращает фиксированную подпись `flux-null`/0.

```rust
let device = NullDevice::new();
let signature = device.device_signature();
assert_eq!(signature.backend, BackendKind::Null);
assert_eq!(signature.device_name, "flux-null");
```

#### `RhiDevice::pipeline_cache(&self) -> Result<Vec<u8>, RhiError>`

Сериализует текущий кэш PSO. Пустой вектор — драйвер ещё не накопил данные.
`Null` возвращает последний загруженный блоб (эмуляция без компиляции).

```rust
let mut device = NullDevice::new();
assert!(device.pipeline_cache().unwrap().is_empty());
device.load_pipeline_cache(&[1, 2, 3]).unwrap();
assert_eq!(device.pipeline_cache().unwrap(), vec![1, 2, 3]);
```

#### `RhiDevice::load_pipeline_cache(&mut self, data) -> Result<(), RhiError>`

Заменяет кэш PSO блобом с диска; пустой срез — no-op. В Vulkan создаётся
новый `VkPipelineCache` с `initialData`, старый уничтожается. `Validation` —
драйвер отверг данные.

```rust
let mut device = NullDevice::new();
device.load_pipeline_cache(&[0xAA; 16]).unwrap();
assert_eq!(device.pipeline_cache().unwrap().len(), 16);
```

#### `RhiDevice::create_buffer(&mut self, desc) -> Result<BufferId, RhiError>`

Создаёт GPU-буфер. Пустой размер/usage — `Validation`, нехватка VRAM — `OutOfMemory`.

```rust
let mut device = NullDevice::with_budget(1024);
let id = device.create_buffer(&GpuBufferDesc::new("vb", 256, BufferUsage::vertex())).unwrap();
assert_eq!(device.allocated_bytes(), 256);
```

#### `RhiDevice::destroy_buffer(&mut self, id) -> bool`

Уничтожает буфер и освобождает лизинг. `false` — хэндл неизвестен.

```rust
let mut device = NullDevice::new();
let id = device.create_buffer(&GpuBufferDesc::new("vb", 16, BufferUsage::vertex())).unwrap();
assert!(device.destroy_buffer(id));
assert!(!device.destroy_buffer(id));
```

#### `RhiDevice::create_image(&mut self, desc) -> Result<ImageId, RhiError>`

Создаёт изображение. Проверяет размеры, mips и лимит
`max_texture_dimension_2d`.

```rust
let mut device = NullDevice::new();
let desc = GpuImageDesc::new("tex", 16, 16, TextureFormat::Rgba8Srgb);
let image = device.create_image(&desc).unwrap();
assert_eq!(device.allocated_bytes(), 16 * 16 * 4);
```

#### `RhiDevice::destroy_image(&mut self, id) -> bool`

Уничтожает изображение.

```rust
let mut device = NullDevice::new();
let desc = GpuImageDesc::new("tex", 16, 16, TextureFormat::Rgba8Srgb);
let image = device.create_image(&desc).unwrap();
assert!(device.destroy_image(image));
```

#### `RhiDevice::create_pipeline(&mut self, desc) -> Result<PipelineId, RhiError>`

Создаёт pipeline state object.

```rust
let mut device = NullDevice::new();
let pipeline = device.create_pipeline(&PipelineDesc::new("pbr", PipelineKind::Graphics)).unwrap();
assert_eq!(device.pipeline_count(), 1);
assert!(device.destroy_pipeline(pipeline));
```

#### `RhiDevice::destroy_pipeline(&mut self, id) -> bool`

Уничтожает pipeline.

```rust
let mut device = NullDevice::new();
let pipeline = device.create_pipeline(&PipelineDesc::new("pbr", PipelineKind::Graphics)).unwrap();
assert!(device.destroy_pipeline(pipeline));
```

#### `RhiDevice::create_descriptor_set(&mut self, desc) -> Result<DescriptorSetId, RhiError>`

Создаёт набор set 0 по layout-у `DescriptorSetDesc { label, bindings }`
(ADR-0039). `BindingKind` — `UniformBuffer`, `StorageBuffer`, `SampledImage`,
`StorageImage`, `Sampler` (ADR-0041); `DescriptorBinding::with_count(n)` задаёт
bindless-массив. Пустой layout, повтор биндинга или `count == 0` —
`Validation`.

#### `RhiDevice::update_descriptor_set(&mut self, id, writes) -> Result<(), RhiError>`

Записывает ресурсы: `DescriptorWrite::buffer(binding, array_index, buffer)`,
`DescriptorWrite::image(binding, array_index, image)` или
`DescriptorWrite::sampler(binding, array_index, sampler)`. Неизвестный
набор/биндинг, индекс вне `count`, несовпадение типа ресурса и неизвестный
ресурс — `Validation`.

Набор не должен быть в полёте: наборы и staging-буферы живут в кольце
`frames_in_flight`, обновление — на границе acquire (ADR-0039).

#### Сэмплеры и чтение изображений (ADR-0041, этап 4)

Реализовано в Null и Vulkan.

- `SamplerDesc { label, mag_filter, min_filter, mipmap_mode, address_u,
  address_v, address_w, max_anisotropy, lod_min, lod_max, compare }`,
  `SamplerId`; `RhiDevice::create_sampler` / `destroy_sampler`. Сэмплеры
  иммутабельны: смена фильтрации — это новый `SamplerId`.
  Перечисления: `FilterMode`, `MipmapMode`, `AddressMode`.
- `BindingKind::Sampler` (HLSL `sN`); `DescriptorResource::Sampler(SamplerId)`,
  `DescriptorWrite::sampler(...)`. `CombinedImageSampler` зарезервирован и в v1
  не реализуется.
- Валидация: `Sampler` ↔ `Sampler`; `SampledImage` ↔ `Image` с
  `TextureUsage::sampled`; `max_anisotropy > 1` требует
  `Capabilities::supports_sampler_anisotropy`; `lod_min > lod_max` или
  `max_anisotropy == 0` — `Validation`.
- `ImageState::ShaderRead` — единственное состояние чтения; барьер
  `→ ShaderRead` вставляет Render Graph перед read-проходом.

```rust
let mut device = NullDevice::new();
let storage = device
    .create_buffer(&GpuBufferDesc::new("storage", 64, BufferUsage::storage()))
    .unwrap();
let image = device
    .create_image(&GpuImageDesc::new("tex", 4, 4, TextureFormat::Rgba8Srgb))
    .unwrap();
let layout = [
    DescriptorBinding::new(0, BindingKind::StorageBuffer),
    DescriptorBinding::new(1, BindingKind::SampledImage).with_count(4),
];
let set = device
    .create_descriptor_set(&DescriptorSetDesc::new("set0").with_bindings(&layout))
    .unwrap();
device
    .update_descriptor_set(
        set,
        &[
            DescriptorWrite::buffer(0, 0, storage),
            DescriptorWrite::image(1, 3, image),
        ],
    )
    .unwrap();
assert!(device.destroy_descriptor_set(set));
```

#### `RhiDevice::destroy_descriptor_set(&mut self, id) -> bool`

Уничтожает набор; `false` — хэндл неизвестен.

#### `RhiDevice::write_buffer(&mut self, id, offset, data) -> Result<(), RhiError>`

Пишет данные в host-visible staging-буфер (`BufferUsage::staging()`).
Диапазон проверяется по размеру; не-host-visible буфер — `Validation`.

#### `RhiDevice::read_buffer(&mut self, id) -> Result<Vec<u8>, RhiError>`

Читает содержимое host-visible буфера (readback). Null-бэкенд хранит
записанные байты в памяти, Vulkan использует `map_memory` после `wait_idle`.

Staging определяется usage-флагами (`BufferUsage::is_staging()`:
`transfer_src && transfer_dst` без GPU-использования), а не свойствами
памяти. На software-бэкенде (lavapipe) вся память host-visible, поэтому
проверка по памяти пропускала бы storage-буферы; контракт должен совпадать
на всех устройствах.

```rust
let mut device = NullDevice::new();
let staging = device
    .create_buffer(&GpuBufferDesc::new("staging", 16, BufferUsage::staging()))
    .unwrap();
device.write_buffer(staging, 4, &[1, 2, 3, 4]).unwrap();
assert_eq!(&device.read_buffer(staging).unwrap()[4..8], &[1, 2, 3, 4]);
```

#### `RhiDevice::create_query_pool(&mut self, desc) -> Result<QueryPoolId, RhiError>`

Создаёт пул таймстемпов; `count` — `1..=4096`.

#### `RhiDevice::destroy_query_pool(&mut self, id) -> bool`

Уничтожает пул запросов.

#### `RhiDevice::read_query_pool(&mut self, id) -> Result<Vec<u64>, RhiError>`

Читает результаты после завершения GPU-работы. Null-бэкенд возвращает
детерминированные `index * 1000` нс; неизвестный пул — `Validation`.

```rust
let mut device = NullDevice::new();
let pool = device.create_query_pool(&QueryPoolDesc::new("zones", 2)).unwrap();
assert_eq!(device.read_query_pool(pool).unwrap(), vec![0, 1_000]);
assert!(device.destroy_query_pool(pool));
assert!(device.read_query_pool(pool).is_err());
```

#### `RhiDevice::create_command_buffer(&mut self) -> Result<Box<dyn CommandBuffer>, RhiError>`

Создаёт командный буфер.

```rust
let mut device = NullDevice::new();
let mut cmd = device.create_command_buffer().unwrap();
cmd.begin().unwrap();
cmd.end().unwrap();
```

#### `RhiDevice::create_surface(&mut self, target) -> Result<SurfaceId, RhiError>`

Создаёт поверхность представления из окна (`SurfaceTarget::Window`, через
`raw-window-handle` 0.6) или headless-таргета (`SurfaceTarget::Headless`).
Владелец окна держит его живым, пока живёт поверхность.

- `Unsupported` — платформа/расширение поверхности недоступны (например,
  нет `VK_EXT_headless_surface`);
- `Surface` — драйвер отверг дескрипторы окна/дисплея.

```rust
let mut device = NullDevice::new();
let surface = device.create_surface(&SurfaceTarget::Headless).unwrap();
assert_eq!(device.surface_count(), 1);
assert!(device.destroy_surface(surface));
assert_eq!(device.surface_count(), 0);
```

#### `RhiDevice::destroy_surface(&mut self, id) -> bool`

Уничтожает поверхность; `false` — хэндл неизвестен или уже уничтожен.

#### `RhiDevice::create_swapchain(&mut self, desc) -> Result<Box<dyn Swapchain>, RhiError>`

Создаёт swapchain. Проверяет размер, формат, present mode и
`frames_in_flight`. `desc.surface == None` — offscreen swapchain; с
`SwapchainDesc::with_surface` создаётся настоящий `VkSwapchainKHR` для
поверхности. Неизвестная поверхность — `InvalidHandle { kind: "surface" }`,
неподдерживаемая очередь/расширение — `Surface`.

```rust
let mut device = NullDevice::new();
let swapchain = device.create_swapchain(&SwapchainDesc::new("main", 1280, 720)).unwrap();
assert_eq!(swapchain.size(), (1280, 720));
```

```rust
let mut device = NullDevice::new();
let surface = device.create_surface(&SurfaceTarget::Headless).unwrap();
let desc = SwapchainDesc::new("present", 64, 64).with_surface(surface);
let mut swapchain = device.create_swapchain(&desc).unwrap();
let index = swapchain.acquire().unwrap();
swapchain.present(index).unwrap();
```

#### `RhiDevice::wait_idle(&mut self) -> Result<(), RhiError>`

Ждёт завершения всех GPU-работ.

```rust
let mut device = NullDevice::new();
device.wait_idle().unwrap();
```

### Swapchain ✅

#### `Swapchain::id(&self) -> SwapchainId`

Хэндл swapchain.

```rust
let mut device = NullDevice::new();
let swapchain = device.create_swapchain(&SwapchainDesc::new("main", 64, 64)).unwrap();
assert_eq!(swapchain.id(), SwapchainId(0));
```

#### `Swapchain::format(&self) -> TextureFormat`

Формат изображений.

```rust
let mut device = NullDevice::new();
let swapchain = device.create_swapchain(&SwapchainDesc::new("main", 64, 64)).unwrap();
assert_eq!(swapchain.format(), TextureFormat::Bgra8Srgb);
```

#### `Swapchain::present_mode(&self) -> PresentMode`

Режим представления.

```rust
let mut device = NullDevice::new();
let swapchain = device.create_swapchain(&SwapchainDesc::new("main", 64, 64)).unwrap();
assert_eq!(swapchain.present_mode(), PresentMode::Fifo);
```

#### `Swapchain::size(&self) -> (u32, u32)`

Текущий размер.

```rust
let mut device = NullDevice::new();
let swapchain = device.create_swapchain(&SwapchainDesc::new("main", 800, 600)).unwrap();
assert_eq!(swapchain.size(), (800, 600));
```

#### `Swapchain::image_count(&self) -> usize`

Число образов в цепочке: у Vulkan — реальные образы `VkSwapchainKHR`,
у Null — `frames_in_flight`. Может измениться после `resize`.

```rust
let mut device = NullDevice::new();
let mut swapchain = device.create_swapchain(&SwapchainDesc::new("main", 64, 64)).unwrap();
assert_eq!(swapchain.image_count(), 2);
swapchain.resize(1280, 720).unwrap();
assert_eq!(swapchain.image_count(), 2);
```

#### `Swapchain::resize(&mut self, width, height) -> Result<(), RhiError>`

Пересоздаёт изображения под новый размер; нулевой размер — `Validation`.

```rust
let mut device = NullDevice::new();
let mut swapchain = device.create_swapchain(&SwapchainDesc::new("main", 800, 600)).unwrap();
swapchain.resize(1280, 720).unwrap();
assert_eq!(swapchain.size(), (1280, 720));
assert!(swapchain.resize(0, 720).is_err());
```

#### `Swapchain::acquire(&mut self) -> Result<u32, RhiError>`

Забирает следующий образ. Повторный `acquire` без `present` — `Validation`.

```rust
let mut device = NullDevice::new();
let mut swapchain = device.create_swapchain(&SwapchainDesc::new("main", 64, 64)).unwrap();
let index = swapchain.acquire().unwrap();
assert!(swapchain.acquire().is_err());
swapchain.present(index).unwrap();
```

#### `Swapchain::present(&mut self, image_index) -> Result<(), RhiError>`

Представляет образ. Несовпадение индекса или `present` без `acquire` —
`Validation`.

```rust
let mut device = NullDevice::new();
let mut swapchain = device.create_swapchain(&SwapchainDesc::new("main", 64, 64)).unwrap();
let index = swapchain.acquire().unwrap();
swapchain.present(index).unwrap();
```

### Барьеры ✅

#### `ImageState`

Состояние изображения: `Undefined`, `ColorAttachment`, `DepthAttachment`,
`ShaderRead`, `Storage`, `TransferSrc`, `TransferDst`, `Present`.
`Storage` — запись из compute/graphics (layout `GENERAL`).

```rust
assert_ne!(ImageState::ColorAttachment, ImageState::Present);
```

#### `ImageBarrier`

Один барьер: `{ image, from, to }`.

```rust
let barrier = ImageBarrier {
    image: ImageId(1),
    from: ImageState::ColorAttachment,
    to: ImageState::Present,
};
assert_eq!(barrier.image, ImageId(1));
```

#### `BarrierBatch::new() -> Self`

Пустой набор барьеров.

```rust
let batch = BarrierBatch::new();
assert!(batch.is_empty());
```

#### `BarrierBatch::transition(&mut self, image, from, to)`

Добавляет переход изображения.

```rust
let mut batch = BarrierBatch::new();
batch.transition(ImageId(1), ImageState::ColorAttachment, ImageState::Present);
assert_eq!(batch.len(), 1);
```

#### `BarrierBatch::extend(&mut self, other)`

Добавляет барьеры другого набора.

```rust
let mut first = BarrierBatch::new();
first.transition(ImageId(1), ImageState::Undefined, ImageState::ShaderRead);
let mut second = BarrierBatch::new();
second.transition(ImageId(2), ImageState::Undefined, ImageState::ShaderRead);
first.extend(&second);
assert_eq!(first.len(), 2);
```

#### `BarrierBatch::len(&self) -> usize`

Число барьеров.

```rust
assert_eq!(BarrierBatch::new().len(), 0);
```

#### `BarrierBatch::is_empty(&self) -> bool`

Пуст ли набор.

```rust
assert!(BarrierBatch::new().is_empty());
```

#### `BarrierBatch::clear(&mut self)`

Очищает набор.

```rust
let mut batch = BarrierBatch::new();
batch.transition(ImageId(1), ImageState::Undefined, ImageState::Present);
batch.clear();
assert!(batch.is_empty());
```

#### `BarrierBatch::barriers(&self) -> &[ImageBarrier]`

Барьеры в порядке добавления.

```rust
let mut batch = BarrierBatch::new();
batch.transition(ImageId(1), ImageState::Undefined, ImageState::Present);
assert_eq!(batch.barriers()[0].image, ImageId(1));
```

### CommandBuffer ✅

#### `CommandBuffer::begin(&mut self) -> Result<(), RhiError>`

Начинает запись команд; повторный вызов — `Validation`.

```rust
let mut cmd = NullCommandBuffer::new();
cmd.begin().unwrap();
assert!(cmd.begin().is_err());
```

#### `CommandBuffer::end(&mut self) -> Result<(), RhiError>`

Завершает запись; открытый render pass — `Validation`.

```rust
let mut cmd = NullCommandBuffer::new();
cmd.begin().unwrap();
cmd.end().unwrap();
```

#### `CommandBuffer::begin_render_pass(&mut self, desc) -> Result<(), RhiError>`

Начинает render pass. Без аттачментов — `Validation`.

```rust
let mut cmd = NullCommandBuffer::new();
cmd.begin().unwrap();
let pass = RenderPassDesc::new("main").with_color(ImageId(1));
cmd.begin_render_pass(&pass).unwrap();
```

#### `CommandBuffer::end_render_pass(&mut self) -> Result<(), RhiError>`

Завершает render pass; без открытого pass — `Validation`.

```rust
let mut cmd = NullCommandBuffer::new();
cmd.begin().unwrap();
let pass = RenderPassDesc::new("main").with_color(ImageId(1));
cmd.begin_render_pass(&pass).unwrap();
cmd.end_render_pass().unwrap();
```

#### `CommandBuffer::reset_query_pool(&mut self, pool, first, count) -> Result<(), RhiError>`

Сбрасывает диапазон запросов; требует `begin`, диапазон проверяется по
ёмкости пула.

#### `CommandBuffer::write_timestamp(&mut self, pool, index) -> Result<(), RhiError>`

Пишет таймстемп; требует `begin`, индекс проверяется по ёмкости пула.

```rust
let mut device = NullDevice::new();
let pool = device.create_query_pool(&QueryPoolDesc::new("zones", 2)).unwrap();
let mut cmd = device.create_command_buffer().unwrap();
cmd.begin().unwrap();
cmd.reset_query_pool(pool, 0, 2).unwrap();
cmd.write_timestamp(pool, 0).unwrap();
cmd.write_timestamp(pool, 1).unwrap();
cmd.end().unwrap();
```

#### `CommandBuffer::set_pipeline(&mut self, pipeline) -> Result<(), RhiError>`

Устанавливает pipeline; требует `begin`.

```rust
let mut cmd = NullCommandBuffer::new();
cmd.begin().unwrap();
cmd.set_pipeline(PipelineId(0)).unwrap();
assert_eq!(cmd.pipeline(), Some(PipelineId(0)));
```

#### `CommandBuffer::bind_descriptor_set(&mut self, set) -> Result<(), RhiError>`

Биндит набор set 0 к текущему pipeline; до `set_pipeline` — `Validation`.

#### `CommandBuffer::copy_buffer_to_image(&mut self, buffer, image) -> Result<(), RhiError>`

Копирует staging-буфер в изображение (mip 0, слой 0, весь размер). Источник
обязан быть staging-буфером; внутри render pass — `Validation`. Вызов
допустим только внутри copy-узла Render Graph (ADR-0039) — барьеры
(`TransferDst`) выставляет граф.

#### `CommandBuffer::copy_image_to_buffer(&mut self, image, buffer) -> Result<(), RhiError>`

Копирует изображение в staging-буфер для readback (mip 0, слой 0, весь размер).
Приёмник обязан быть staging-буфером; внутри render pass — `Validation`.
Вызов — только внутри copy-узла Render Graph (ADR-0039).

Null-бэкенд эмулирует содержимое: копии переносят байты между памятью буфера
и изображения (этап 4), поэтому срез «clear → copy → readback» проверяется в CI
без GPU. Vulkan выполняет реальные `vkCmdCopy*`; на обоих бэкендах результат
побайтово совпадает для форматов из `TextureFormat`.

```rust
let mut device = NullDevice::new();
let mut usage = TextureUsage::render_target();
usage.transfer_src = true;
let target = device
    .create_image(
        &GpuImageDesc::new("target", 2, 2, TextureFormat::Rgba8Unorm).with_usage(usage),
    )
    .unwrap();
let staging = device
    .create_buffer(&GpuBufferDesc::new("readback", 16, BufferUsage::staging()))
    .unwrap();
let mut cmd = device.create_command_buffer().unwrap();
cmd.begin().unwrap();
cmd.begin_render_pass(
    &RenderPassDesc::new("clear").with_color(target).with_clear([1.0, 0.0, 0.0, 1.0]),
)
.unwrap();
cmd.end_render_pass().unwrap();
cmd.copy_image_to_buffer(target, staging).unwrap();
cmd.end().unwrap();
cmd.finish().unwrap();
let pixels = device.read_buffer(staging).unwrap();
assert_eq!(&pixels[..4], &[255, 0, 0, 255]);
```

#### `CommandBuffer::draw(&mut self, vertices, instances) -> Result<(), RhiError>`

Рисует вершины; вне render pass — `Validation`.

```rust
let mut cmd = NullCommandBuffer::new();
cmd.begin().unwrap();
let pass = RenderPassDesc::new("main").with_color(ImageId(1));
cmd.begin_render_pass(&pass).unwrap();
cmd.draw(3, 1).unwrap();
assert_eq!(cmd.draws(), 1);
```

#### `CommandBuffer::dispatch(&mut self, groups) -> Result<(), RhiError>`

Запускает compute dispatch; внутри render pass — `Validation`.

```rust
let mut cmd = NullCommandBuffer::new();
cmd.begin().unwrap();
cmd.dispatch([8, 8, 1]).unwrap();
```

#### `CommandBuffer::barriers(&mut self, batch) -> Result<(), RhiError>`

Применяет барьеры; требует `begin`.

```rust
let mut cmd = NullCommandBuffer::new();
cmd.begin().unwrap();
let mut batch = BarrierBatch::new();
batch.transition(ImageId(1), ImageState::Undefined, ImageState::ShaderRead);
cmd.barriers(&batch).unwrap();
assert_eq!(cmd.barriers_applied(), 1);
```

#### `CommandBuffer::finish(self: Box<Self>) -> Result<(), RhiError>`

Завершает буфер и передаёт на исполнение; незакрытая запись — `Validation`.

```rust
let mut cmd = NullCommandBuffer::new();
cmd.begin().unwrap();
cmd.end().unwrap();
Box::new(cmd).finish().unwrap();
```

### VRAM ✅

#### `VramLease`

Лизинг памяти: `{ id, bytes, tag }`.

```rust
let mut allocator = NullVramAllocator::new(1024);
let lease = allocator.allocate(256, MemoryTag::Rendering).unwrap();
assert_eq!(lease.bytes, 256);
```

#### `Pressure::from_ratio(ratio) -> Self`

Уровень давления: `<0.70` Normal, `<0.85` Elevated, `<0.95` High, иначе Critical.

```rust
assert_eq!(Pressure::from_ratio(0.5), Pressure::Normal);
assert_eq!(Pressure::from_ratio(0.75), Pressure::Elevated);
assert_eq!(Pressure::from_ratio(0.9), Pressure::High);
assert_eq!(Pressure::from_ratio(0.99), Pressure::Critical);
```

#### `BudgetSnapshot::available(&self) -> u64`

Свободно байт с учётом `reserved` и `used`.

```rust
let snapshot = BudgetSnapshot { total: 1000, reserved: 100, used: 200, pressure: Pressure::Normal };
assert_eq!(snapshot.available(), 700);
```

#### `BudgetSnapshot::utilization(&self) -> f32`

Доля занятой памяти `0.0..=1.0`.

```rust
let snapshot = BudgetSnapshot { total: 1000, reserved: 100, used: 200, pressure: Pressure::Normal };
assert!((snapshot.utilization() - 0.3).abs() < 1e-6);
```

#### `VramAllocator::allocate(&mut self, bytes, tag) -> Result<VramLease, RhiError>`

Захватывает память; нулевой размер — `Validation`, превышение — `OutOfMemory`.

```rust
let mut allocator = NullVramAllocator::new(1024);
let lease = allocator.allocate(512, MemoryTag::Rendering).unwrap();
assert_eq!(allocator.used(), 512);
```

#### `VramAllocator::free(&mut self, lease) -> bool`

Освобождает лизинг.

```rust
let mut allocator = NullVramAllocator::new(1024);
let lease = allocator.allocate(512, MemoryTag::Rendering).unwrap();
assert!(allocator.free(lease));
assert_eq!(allocator.used(), 0);
```

#### `VramAllocator::budget(&self) -> BudgetSnapshot`

Текущий снимок бюджета.

```rust
let mut allocator = NullVramAllocator::new(100);
allocator.allocate(50, MemoryTag::Rendering).unwrap();
assert_eq!(allocator.budget().used, 50);
```

### Null backend ✅

#### `NullDevice::new() -> Self`

Null-устройство с бюджетом 256 МиБ и headless-capabilities.

```rust
let device = NullDevice::new();
assert_eq!(device.backend(), BackendKind::Null);
```

#### `NullDevice::with_budget(total) -> Self`

Null-устройство с заданным бюджетом VRAM.

```rust
let device = NullDevice::with_budget(1024);
assert_eq!(device.budget().total, 1024);
```

#### `NullDevice::with_capabilities(caps) -> Self`

Null-устройство с явными capabilities.

```rust
let device = NullDevice::with_capabilities(Capabilities::headless());
assert_eq!(device.capabilities().max_frames_in_flight, 2);
```

#### `NullDevice::buffer_count(&self) -> usize`

Число живых буферов.

```rust
let mut device = NullDevice::new();
device.create_buffer(&GpuBufferDesc::new("vb", 16, BufferUsage::vertex())).unwrap();
assert_eq!(device.buffer_count(), 1);
```

#### `NullDevice::image_count(&self) -> usize`

Число живых изображений.

```rust
let mut device = NullDevice::new();
device.create_image(&GpuImageDesc::new("tex", 8, 8, TextureFormat::Rgba8Srgb)).unwrap();
assert_eq!(device.image_count(), 1);
```

#### `NullDevice::surface_count(&self) -> usize`

Число живых поверхностей.

```rust
let mut device = NullDevice::new();
let surface = device.create_surface(&SurfaceTarget::Headless).unwrap();
assert_eq!(device.surface_count(), 1);
device.destroy_surface(surface);
assert_eq!(device.surface_count(), 0);
```

#### `NullDevice::surface_kind(&self, id) -> Option<SurfaceKind>`

Вид созданной поверхности; `None` — хэндл неизвестен. Null хранит только
вид: платформенные указатели не нужны и не пересекают потоки.

```rust
let mut device = NullDevice::new();
let surface = device.create_surface(&SurfaceTarget::Headless).unwrap();
assert_eq!(device.surface_kind(surface), Some(SurfaceKind::Headless));
```

#### `NullDevice::pipeline_count(&self) -> usize`

Число живых pipeline.

```rust
let mut device = NullDevice::new();
device.create_pipeline(&PipelineDesc::new("pbr", PipelineKind::Graphics)).unwrap();
assert_eq!(device.pipeline_count(), 1);
```

#### `NullDevice::sampler_count(&self) -> usize`

Число живых сэмплеров (ADR-0041).

```rust
let mut device = NullDevice::new();
device.create_sampler(&SamplerDesc::new("linear")).unwrap();
assert_eq!(device.sampler_count(), 1);
```

#### `NullDevice::sampler_desc(&self, id) -> Option<SamplerDesc>`

Дескриптор созданного сэмплера; `None` — хэндл неизвестен.

```rust
let mut device = NullDevice::new();
let desc = SamplerDesc::new("linear").with_address(AddressMode::ClampToEdge);
let sampler = device.create_sampler(&desc).unwrap();
assert_eq!(device.sampler_desc(sampler), Some(desc));
```

#### `NullDevice::descriptor_set_count(&self) -> usize`

Число живых дескрипторных наборов.

```rust
let mut device = NullDevice::new();
let layout = [DescriptorBinding::new(0, BindingKind::Sampler)];
let set = device
    .create_descriptor_set(&DescriptorSetDesc::new("set0").with_bindings(&layout))
    .unwrap();
assert_eq!(device.descriptor_set_count(), 1);
assert!(device.destroy_descriptor_set(set));
assert_eq!(device.descriptor_set_count(), 0);
```

#### `NullDevice::descriptor_write(&self, set, binding, array_index) -> Option<DescriptorResource>`

Записанный ресурс набора; `None` — запись не производилась.

```rust
let mut device = NullDevice::new();
let sampler = device.create_sampler(&SamplerDesc::new("s")).unwrap();
let layout = [DescriptorBinding::new(0, BindingKind::Sampler)];
let set = device
    .create_descriptor_set(&DescriptorSetDesc::new("set0").with_bindings(&layout))
    .unwrap();
device
    .update_descriptor_set(set, &[DescriptorWrite::sampler(0, 0, sampler)])
    .unwrap();
assert_eq!(
    device.descriptor_write(set, 0, 0),
    Some(DescriptorResource::Sampler(sampler)),
);
```

#### Эмуляция содержимого ресурсов (этап 4)

Null хранит байты буферов и изображений и воспроизводит семантику копий,
поэтому readback-тесты работают без GPU:

- `create_image` инициализирует изображение нулями; `create_buffer` — нет
  (буфер заполняется нулями при первом `write_buffer`/копировании).
- `begin_render_pass` записывает `clear_color` в color-аттачменты и обнуляет
  depth-аттачмент. Раскладка каналов соответствует `TextureFormat`:
  `Rgba8Srgb`/`Rgba8Unorm` — RGBA, `Bgra8Srgb` — BGRA, `R8Unorm` — R,
  `Rgba16Float` — IEEE-754 binary16, `D32Float` — нули. sRGB-перенос не
  применяется: clear-значение пишется как есть (для `1.0`/`0.0` это
  эквивалентно).
- `copy_buffer_to_image` переносит байты staging-буфера в изображение,
  `copy_image_to_buffer` — наоборот; переносится `min(длина, размер)`.
- Draw/dispatch содержимое не меняют — Null не исполняет шейдеры. Поэтому
  GPU-эталон (PSNR) проверяется на Vulkan, а Null фиксирует контракт RHI.

#### `NullDevice::allocated_bytes(&self) -> u64`

Занято VRAM.

```rust
let mut device = NullDevice::new();
device.create_buffer(&GpuBufferDesc::new("vb", 64, BufferUsage::vertex())).unwrap();
assert_eq!(device.allocated_bytes(), 64);
```

#### `NullDevice::budget(&self) -> BudgetSnapshot`

Снимок бюджета.

```rust
let device = NullDevice::with_budget(1024);
assert_eq!(device.budget().total, 1024);
```

#### `NullVramAllocator::new(total) -> Self`

Null-аллокатор с бюджетом.

```rust
let allocator = NullVramAllocator::new(1024);
assert_eq!(allocator.total(), 1024);
```

#### `NullVramAllocator::total(&self) -> u64`

Всего байт.

```rust
assert_eq!(NullVramAllocator::new(512).total(), 512);
```

#### `NullVramAllocator::used(&self) -> u64`

Занято байт.

```rust
let mut allocator = NullVramAllocator::new(512);
allocator.allocate(64, MemoryTag::Rendering).unwrap();
assert_eq!(allocator.used(), 64);
```

#### `NullVramAllocator::lease_count(&self) -> usize`

Число активных лизингов.

```rust
let mut allocator = NullVramAllocator::new(512);
allocator.allocate(64, MemoryTag::Rendering).unwrap();
assert_eq!(allocator.lease_count(), 1);
```

#### `NullCommandBuffer::new() -> Self`

Пустой null-командный буфер.

```rust
let cmd = NullCommandBuffer::new();
assert_eq!(cmd.draws(), 0);
```

#### `NullCommandBuffer::draws(&self) -> u32`

Число draw-вызовов.

```rust
let mut cmd = NullCommandBuffer::new();
cmd.begin().unwrap();
let pass = RenderPassDesc::new("main").with_color(ImageId(1));
cmd.begin_render_pass(&pass).unwrap();
cmd.draw(3, 1).unwrap();
assert_eq!(cmd.draws(), 1);
```

#### `NullCommandBuffer::passes(&self) -> u32`

Число render pass.

```rust
let mut cmd = NullCommandBuffer::new();
cmd.begin().unwrap();
let pass = RenderPassDesc::new("main").with_color(ImageId(1));
cmd.begin_render_pass(&pass).unwrap();
cmd.end_render_pass().unwrap();
assert_eq!(cmd.passes(), 1);
```

#### `NullCommandBuffer::barriers_applied(&self) -> usize`

Число применённых барьеров.

```rust
let mut cmd = NullCommandBuffer::new();
cmd.begin().unwrap();
cmd.barriers(&BarrierBatch::new()).unwrap();
assert_eq!(cmd.barriers_applied(), 0);
```

#### `NullCommandBuffer::pipeline(&self) -> Option<PipelineId>`

Установленный pipeline.

```rust
let mut cmd = NullCommandBuffer::new();
cmd.begin().unwrap();
cmd.set_pipeline(PipelineId(0)).unwrap();
assert_eq!(cmd.pipeline(), Some(PipelineId(0)));
```

#### `create_device(backend) -> Result<Box<dyn RhiDevice>, RhiError>`

Создаёт устройство выбранного бэкенда. `Null` доступен всегда. `Vulkan`
работает при включённой feature `vulkan` и наличии loader-а/GPU; без feature,
без loader-а или без graphics-очереди возвращается `Unsupported`. `Dx12` —
`Unsupported` до I2.

```rust
let device = create_device(BackendKind::Null).unwrap();
assert_eq!(device.backend(), BackendKind::Null);
assert!(matches!(
    create_device(BackendKind::Dx12),
    Err(RhiError::Unsupported { .. })
));
```

### Кэш PSO на диске ✅

Кэш драйвера (`VkPipelineCache`) валиден только для создавшего его устройства,
поэтому файл привязан к [`DeviceSignature`]: `backend`, `device_name`,
`api_version`. Заголовок детерминирован (little-endian):
`FLUXPSO1 | version u32 | signature_hash u64 | payload_len u64 | payload`.
Запись атомарная (временный файл + переименование), чтение чужого/битого
файла возвращает `Invalid` — приложение пересобирает PSO и перезаписывает кэш.

#### `DeviceSignature`

Подпись устройства. `hash()` — стабильный FNV-1a по бэкенду, имени и версии API.

```rust
let a = DeviceSignature::new(BackendKind::Vulkan, "GPU A", 0x0040_0000);
let b = DeviceSignature::new(BackendKind::Vulkan, "GPU B", 0x0040_0000);
assert_eq!(a, a.clone());
assert_ne!(a.hash(), b.hash());
assert_ne!(a.hash(), 0);
```

#### `PipelineCacheFile::new(path, signature) -> Self`

Описатель файла кэша; диск не трогается до `load`/`save`.

```rust
let signature = DeviceSignature::new(BackendKind::Null, "flux-null", 0);
let file = PipelineCacheFile::new("target/pso.cache", signature);
assert_eq!(file.path().to_string_lossy(), "target/pso.cache");
assert_eq!(file.signature().device_name, "flux-null");
```

#### `PipelineCacheFile::load(&self) -> io::Result<PipelineCacheLoad>`

`Missing` — файла нет (первый запуск); `Loaded(blob)` — блоб принадлежит
устройству; `Invalid(reason)` — магия, версия формата, подпись устройства или
длина не совпали.

```rust
let signature = DeviceSignature::new(BackendKind::Null, "flux-null", 0);
let path = std::env::temp_dir().join("flux-api-pso-load.bin");
let file = PipelineCacheFile::new(&path, signature);
assert_eq!(file.load().unwrap(), PipelineCacheLoad::Missing);
file.save(&[1, 2, 3]).unwrap();
assert_eq!(file.load().unwrap(), PipelineCacheLoad::Loaded(vec![1, 2, 3]));
let _ = file.invalidate();
```

#### `PipelineCacheFile::save(&self, data) -> io::Result<()>`

Создаёт каталог, пишет заголовок и блоб во временный файл, затем переименовывает.

```rust
let path = std::env::temp_dir().join("flux-api-pso-save.bin");
let file = PipelineCacheFile::new(
    &path,
    DeviceSignature::new(BackendKind::Null, "flux-null", 0),
);
file.save(&[0xAB; 32]).unwrap();
assert!(matches!(
    file.load().unwrap(),
    PipelineCacheLoad::Loaded(ref data) if data.len() == 32
));
let _ = file.invalidate();
```

#### `PipelineCacheFile::invalidate(&self) -> io::Result<bool>`

Удаляет файл; `false` — файла не было.

```rust
let path = std::env::temp_dir().join("flux-api-pso-invalidate.bin");
let file = PipelineCacheFile::new(
    &path,
    DeviceSignature::new(BackendKind::Null, "flux-null", 0),
);
file.save(&[7; 8]).unwrap();
assert!(file.invalidate().unwrap());
assert!(!file.invalidate().unwrap());
```

#### `PIPELINE_CACHE_MAGIC`, `PIPELINE_CACHE_VERSION`

Магия `FLUXPSO1` и версия формата заголовка (`1`); меняются при изменении
раскладки файла, что автоматически инвалидирует старые кэши.

```rust
assert_eq!(&PIPELINE_CACHE_MAGIC, b"FLUXPSO1");
assert_eq!(PIPELINE_CACHE_VERSION, 1);
```

### Vulkan backend (feature `vulkan`) ✅

Бэкенд включается feature-гейтом и загружает Vulkan loader в рантайме
(`ash` с feature `loaded`, ADR-0034): сборка не требует Vulkan SDK, а
headless-null остаётся дефолтом.

```toml
flux-graphics = { workspace = true, features = ["vulkan"] }
```

Что реализовано в I1: instance (validation layers в debug-сборке), выбор
физического устройства и graphics-очереди, логическое устройство, пул
командных буферов, выделение памяти под буферы/изображения, кэш
render pass/framebuffer, offscreen swapchain, барьеры состояний и
leak-трекинг. В I5 добавлены реальные graphics/compute pipeline из SPIR-V:
shader modules, `PipelineLayout` без дескрипторов, vertex input (позиция
`R32G32B32_SFLOAT`, stride 12), dynamic viewport/scissor, blend disable,
render pass из форматов `PipelineDesc`. Кэш PSO создаётся вместе с
устройством: `pipeline_cache`/`load_pipeline_cache` отдают и принимают блоб
`VkPipelineCache`, дисковое хранение — `PipelineCacheFile` (раздел выше).

Поверхности и present: `create_surface` создаёт `VkSurfaceKHR` из
`SurfaceTarget` (win32/xlib/xcb/wayland или `VK_EXT_headless_surface`),
`create_swapchain` с `SwapchainDesc::surface` — настоящий `VkSwapchainKHR`
(`VulkanPresentSwapchain`): выбор формата/present mode/composite alpha,
acquire через fence, переход образа `UNDEFINED → PRESENT_SRC_KHR` и
`vkQueuePresentKHR`; `resize` пересоздаёт цепочку после `device_wait_idle`.
`VK_KHR_swapchain` включается на устройстве, если доступен.

#### `VulkanDevice::new() -> Result<Self, RhiError>`

Создаёт Vulkan-устройство.

- `Unsupported` — feature выключена, нет loader-а, нет GPU с graphics-очередью
  или нет подходящего color-формата.
- `Validation`/`OutOfMemory` — драйвер отверг instance/device.

```rust
match VulkanDevice::new() {
    Ok(device) => {
        assert_eq!(device.backend(), BackendKind::Vulkan);
        assert!(device.capabilities().max_texture_dimension_2d >= 1024);
    }
    Err(RhiError::Unsupported { .. }) => {
        // Нет Vulkan loader-а/GPU: тест или приложение остаются headless.
    }
    Err(other) => panic!("vulkan init failed: {other}"),
}
```

#### `VulkanDevice::device_name(&self) -> &str`

Имя выбранного физического устройства.

#### `VulkanDevice::api_version(&self) -> u32`

Версия Vulkan API, с которой создан instance (`vk::API_VERSION_1_0` или
`vk::API_VERSION_1_2`).

#### `VulkanDevice::validation_enabled(&self) -> bool`

Включены ли validation layers (в debug-сборке, если слой доступен).

#### `VulkanDevice::validation_errors(&self) -> u64`

Число validation-ошибок с момента старта процесса. Гейт I1: кадр clear-color
не добавляет ни одной.

#### `VulkanDevice::live_resources(&self) -> usize`

Число живых буферов и изображений (leak-трекинг «выделено N — освобождено N»).

#### `VulkanDevice::allocated_bytes(&self) -> u64`

Занято памяти ресурсами, байт.

#### `VulkanDevice::budget_snapshot(&self) -> BudgetSnapshot`

Снимок бюджета VRAM (device-local кучи; лизинги + ресурсы).

#### Поведение Vulkan-бэкенда

- Буферы и изображения создаются с реальной `VkDeviceMemory`; при ошибке
  выделения возвращается `OutOfMemory`, частично созданные объекты
  уничтожаются.
- `create_swapchain` без `desc.surface` создаёт offscreen-таргет; с
  поверхностью — `VkSwapchainKHR` и present через `vkQueuePresentKHR`
  (headless WSI на lavapipe; окно на реальном `winit` — после 3.1).
- Барьеры `ImageState` транслируются в stage/access/layout; depth clear —
  `0.0` для Reversed-Z (ADR-0008).
- `create_pipeline` собирает `VkPipeline` из SPIR-V; пустые или битые
  шейдеры — `Validation`, compute-шейдер для graphics-пайплайна — тоже
  `Validation`. `set_pipeline` биндит graphics/compute pipeline, `draw`
  требует graphics + активный render pass, `dispatch` — compute вне
  render pass.
- Реальные шейдеры и кадр с draw появятся в I6 (компилятор шейдеров);
  сейчас тесты покрывают типизированные ошибки и clear-color.
- `VulkanDevice` реализует `VramAllocator`: лизинги учитываются в общем
  бюджете вместе с реальными аллокациями.

```rust
let mut device = VulkanDevice::new().expect("vulkan device");
let buffer = device
    .create_buffer(&GpuBufferDesc::new("staging", 4096, BufferUsage::staging()))
    .unwrap();
assert_eq!(device.live_resources(), 1);
assert!(device.destroy_buffer(buffer));
assert_eq!(device.live_resources(), 0);
```


### Transfer, ResourceRegistry, VramBudget ✅

Слой `flux-graphics::render` зависит только от `rhi` и не знает конкретного
API. I2 — планирование: загрузка получает VRAM-лизинг и место в staging-кольце,
батч фиксируется на кадре, ретирмент освобождает лизинги после fence-а.
Реальные copy-команды появятся, когда RHI получит `copy_buffer_to_image`
(I3/I5); контракт батча при этом не меняется.

#### `TransferSystem::new(staging_capacity) -> Self`

Планировщик загрузок с заданным размером staging-кольца.

```rust
let transfer = TransferSystem::new(4096);
assert_eq!(transfer.staging_capacity(), 4096);
```

#### `TransferSystem::with_default_capacity() -> Self`

Кольцо по умолчанию — 64 МиБ (`DEFAULT_STAGING_BYTES`).

```rust
let transfer = TransferSystem::with_default_capacity();
assert_eq!(transfer.staging_capacity(), 64 * 1024 * 1024);
```

#### `TransferSystem::upload(&mut self, allocator, image, bytes, tag) -> Result<u64, RhiError>`

Ставит загрузку в очередь, берёт VRAM-лизинг и возвращает детерминированный id.
`OutOfMemory` — staging-кольцо или VRAM исчерпаны; `Validation` — нулевой размер.

```rust
let mut allocator = NullVramAllocator::new(1 << 20);
let mut transfer = TransferSystem::new(4096);
let job = transfer
    .upload(&mut allocator, ImageId(1), 1024, MemoryTag::Rendering)
    .unwrap();
assert_eq!(job, 0);
assert_eq!(transfer.pending_count(), 1);
```

#### `TransferSystem::submit_batch(&mut self, frame) -> Result<u64, RhiError>`

Фиксирует ожидающие загрузки в батч на кадре `frame`. Пустой батч —
`Validation`.

```rust
let mut allocator = NullVramAllocator::new(1 << 20);
let mut transfer = TransferSystem::new(4096);
transfer
    .upload(&mut allocator, ImageId(1), 16, MemoryTag::Rendering)
    .unwrap();
let batch = transfer.submit_batch(7).unwrap();
assert_eq!(batch, 0);
assert_eq!(transfer.in_flight_batches(), 1);
```

#### `TransferSystem::retire(&mut self, allocator, completed_frame) -> usize`

Освобождает батчи с `frame <= completed_frame` (между кадрами, после fence).
Возвращает число завершённых батчей.

```rust
let mut allocator = NullVramAllocator::new(1 << 20);
let mut transfer = TransferSystem::new(4096);
transfer
    .upload(&mut allocator, ImageId(1), 16, MemoryTag::Rendering)
    .unwrap();
transfer.submit_batch(7).unwrap();
assert_eq!(transfer.retire(&mut allocator, 6), 0);
assert_eq!(transfer.retire(&mut allocator, 7), 1);
assert_eq!(allocator.budget().used, 0);
```

Геттеры: `staging_capacity`, `staging_used`, `pending_count`, `pending_bytes`,
`in_flight_batches`, `in_flight_bytes`, `is_idle`.

#### `ResourceKey(pub u64)`

Детерминированный ключ ресурса (ассет/UUID).

#### `ResourceState`

`Empty | Pending | Ready | Evicting` — состояние ресурса в реестре.

#### `ResourceEntry`

`state`, `bytes`, `slot`, `image`, `proxy` — запись реестра.

#### `ResourceRegistry::new(capacity) -> Self`

Реестр на `capacity` bindless-слотов.

```rust
let registry = ResourceRegistry::new(65_536);
assert_eq!(registry.capacity(), 65_536);
```

#### `ResourceRegistry::insert_pending(&mut self, key, bytes) -> Result<u32, RhiError>`

Регистрирует запрос загрузки и выдаёт минимальный свободный bindless-слот.
Дубликат ключа, нулевой размер и исчерпание слотов — `Validation`.

```rust
let mut registry = ResourceRegistry::new(2);
assert_eq!(registry.insert_pending(ResourceKey(1), 1024).unwrap(), 0);
assert_eq!(registry.state(ResourceKey(1)), Some(ResourceState::Pending));
```

#### `ResourceRegistry::set_proxy(&mut self, key, image) -> Result<(), RhiError>`

Задаёт прокси-изображение для `Pending`, чтобы рендер не ждал загрузку.

#### `ResourceRegistry::mark_ready(&mut self, key, image) -> Result<(), RhiError>`

`Pending → Ready`: ресурс готов, прокси снимается.

#### `ResourceRegistry::mark_failed(&mut self, key) -> Result<(), RhiError>`

`Pending → Empty`: неудачная загрузка снимается, слот возвращается.

#### `ResourceRegistry::begin_evict(&mut self, key) -> Result<(), RhiError>`

`Ready → Evicting`: ресурс больше нельзя использовать.

#### `ResourceRegistry::finish_evict(&mut self, key) -> Result<ImageId, RhiError>`

`Evicting → Empty`: возвращает изображение для уничтожения на GPU и
переиспользует слот.

```rust
let mut registry = ResourceRegistry::new(2);
registry.insert_pending(ResourceKey(1), 1024).unwrap();
registry.mark_ready(ResourceKey(1), ImageId(10)).unwrap();
registry.begin_evict(ResourceKey(1)).unwrap();
assert_eq!(registry.finish_evict(ResourceKey(1)).unwrap(), ImageId(10));
assert_eq!(registry.insert_pending(ResourceKey(2), 16).unwrap(), 0);
```

Геттеры: `state`, `entry`, `slot`, `image`, `proxy`, `count(state)`,
`active_bytes`, `eviction_candidates` (Ready, по возрастанию ключа),
`len`, `is_empty`.

#### `VramBudget::new(total) -> Self`

Бюджет VRAM с гистерезисом 5 п.п. (`DEFAULT_HYSTERESIS`).

```rust
let budget = VramBudget::new(1024);
assert_eq!(budget.pressure(), Pressure::Normal);
```

#### `VramBudget::with_hysteresis(total, hysteresis) -> Self`

Бюджет с явным гистерезисом (`0.0..1.0`; вне диапазона — значение по
умолчанию).

#### `VramBudget::reserve(&mut self, bytes) -> Result<(), RhiError>`

Резервирует память под transient-таргет; `OutOfMemory` — бюджет исчерпан.

#### `VramBudget::release_reserved(&mut self, bytes)`

Освобождает резерв.

#### `VramBudget::record_used(&mut self, used)`

Обновляет фактическую занятость (из `VramAllocator`) и пересчитывает давление.

#### `VramBudget::snapshot(&self) -> BudgetSnapshot`

Снимок `total/reserved/used/pressure`.

#### `VramBudget::can_allocate(&self, bytes) -> bool`

Хватает ли бюджета с учётом резерва.

#### `VramBudget::should_evict(&self) -> bool`

`true` при `High`/`Critical`.

Геттеры: `total`, `reserved`, `used`, `pressure`.

Давление повышается сразу, понижается только после гистерезиса: из
`Critical` — при доле `<= 0.90`, из `High` — `<= 0.80`, из `Elevated` —
`<= 0.65`.

```rust
let mut budget = VramBudget::new(1000);
budget.record_used(950);
assert_eq!(budget.pressure(), Pressure::Critical);
budget.record_used(910);
assert_eq!(budget.pressure(), Pressure::Critical);
budget.record_used(900);
assert_eq!(budget.pressure(), Pressure::High);
```

### Render Graph ✅

Граф не знает конкретного API: он планирует порядок пассов и переходы
`ImageState`, а RHI транслирует их в барьеры. Добавление пасса не требует
ручных барьеров. Модуль не импортирует `rhi::vulkan`.

#### `GraphImageKey(pub u64)`

Ключ изображения графа; совпадает с id ресурса в `ResourceRegistry`.

#### `GraphImage::new(key, bytes) -> Self`

Transient-изображение; участвует в aliasing.

#### `GraphImage::external(key, bytes) -> Self`

Внешний ресурс (swapchain image, импорт); в aliasing не участвует.

Поля: `key`, `bytes`, `external`.

#### `PassKind`

`Graphics | Compute | Transfer | Present` — определяет целевое состояние записей.

#### `GraphPass::new(name, kind) -> Self`

Пасс с именем и типом. Билдеры: `read(key)`, `write(key)`, `color(key)`,
`depth(key)`.

```rust
let pass = GraphPass::new("gbuffer", PassKind::Graphics)
    .color(GraphImageKey(1))
    .depth(GraphImageKey(2));
```

#### `GraphError`

`DuplicateImage`, `DuplicatePass`, `UnknownImage { pass, image }`, `Cycle`.

#### `RenderGraph::new() -> Self`

Пустой граф.

#### `RenderGraph::add_image(&mut self, image) -> Result<(), GraphError>`

Регистрирует изображение; дубликат ключа — `DuplicateImage`.

#### `RenderGraph::add_pass(&mut self, pass) -> Result<(), GraphError>`

Добавляет пасс; повтор имени — `DuplicatePass`. Ссылки на изображения
проверяются при компиляции.

#### `RenderGraph::compile(&self) -> Result<RenderGraphPlan, GraphError>`

Строит детерминированный топологический порядок (по зависимостям
read/write, tie-break — порядок добавления), автоматические барьеры и план
aliasing. `UnknownImage` — пасс ссылается на неизвестный ресурс; `Cycle` —
циклическая зависимость.

```rust
let mut graph = RenderGraph::new();
graph.add_image(GraphImage::new(GraphImageKey(1), 1024)).unwrap();
graph.add_image(GraphImage::new(GraphImageKey(2), 1024)).unwrap();
graph
    .add_pass(GraphPass::new("gbuffer", PassKind::Graphics).color(GraphImageKey(1)))
    .unwrap();
graph
    .add_pass(
        GraphPass::new("lighting", PassKind::Graphics)
            .read(GraphImageKey(1))
            .color(GraphImageKey(2)),
    )
    .unwrap();
let plan = graph.compile().unwrap();
assert_eq!(plan.order, vec!["gbuffer", "lighting"]);
```

#### `RenderGraph::execute(&self, plan, commands) -> Result<(), RhiError>`

Применяет барьеры плана к командному буферу. Тела пассов (draw/dispatch и
привязка ресурсов) появятся в I4/I5 — контракт синхронизации уже зафиксирован.

```rust
let mut commands = NullCommandBuffer::new();
commands.begin().unwrap();
graph.execute(&plan, &mut commands).unwrap();
commands.end().unwrap();
assert_eq!(commands.barriers_applied(), plan.barrier_count());
```

#### `RenderGraph::set_output_size(&mut self, width, height) -> bool`

Меняет выходной размер; `true` — размер изменился. Нулевые размеры
игнорируются. Несколько resize до rebuild коалесцируются в один.

#### `RenderGraph::needs_rebuild(&self) -> bool`

Нужен ли rebuild (resize/preset).

#### `RenderGraph::rebuild_if_needed(&mut self) -> bool`

Сбрасывает флаг и возвращает, был ли он установлен.

Геттеры: `images`, `passes`, `output_size`.

#### `CompiledPass`

`name`, `kind`, `barriers` (`BarrierBatch`), `reads`, `writes`.

#### `AliasSlot`

`bytes` (ёмкость слота) и `members` (ключи с непересекающимся временем
жизни, по возрастанию).

#### `RenderGraphPlan`

`passes`, `alias_slots`, `order`; `barrier_count()` — суммарное число
барьеров.

```rust
let mut graph = RenderGraph::new();
graph.add_image(GraphImage::new(GraphImageKey(1), 1024)).unwrap();
graph.add_image(GraphImage::new(GraphImageKey(2), 1024)).unwrap();
graph
    .add_pass(GraphPass::new("a", PassKind::Graphics).color(GraphImageKey(1)))
    .unwrap();
graph
    .add_pass(
        GraphPass::new("b", PassKind::Graphics)
            .read(GraphImageKey(1))
            .color(GraphImageKey(2)),
    )
    .unwrap();
let plan = graph.compile().unwrap();
assert_eq!(plan.alias_slots[0].members, vec![GraphImageKey(1)]);
```

Планирование на async compute и профиль `Quality::Editor` — заготовки
следующих итераций; `RenderGraph` уже не зависит от API.

### Камера и RenderScene ✅

Камера задаётся в sim-пространстве, но рендер работает camera-relative:
`RenderScene` вычитает позицию камеры из трансформов, а view-матрица —
чистый поворот (камера в начале координат). Projection — Reversed-Z:
near → `1.0`, far → `0.0` (ADR-0008).

#### `Camera`

Поля: `position`, `forward`, `up`, `fov_y` (радианы), `aspect`, `near`, `far`.
По умолчанию: позиция `ZERO`, взгляд `-Z`, верх `+Y`, FOV 60°, 16:9,
near `0.1`, far `1000`.

#### `Camera::new(position, forward, up) -> Self`

Камера с ориентацией; остальные параметры — по умолчанию.

#### `Camera::camera_relative(&self, world) -> Vec3`

`world - position`.

#### `Camera::view_matrix(&self) -> Mat4`

View-матрица без трансляции: `look_at_rh(0, forward, up)`. Нулевой
`forward`/`up` заменяется детерминированным значением.

#### `Camera::projection_reversed_z(&self) -> Mat4`

Reversed-Z projection для диапазона `0..1`. Некорректные `near/far/aspect/fov`
санитизируются, чтобы не получить NaN.

#### `Camera::view_projection(&self) -> Mat4`

`projection_reversed_z() * view_matrix()`.

```rust
let camera = Camera::new(Vec3::new(10.0, 0.0, 0.0), -Vec3::Z, Vec3::Y);
assert_eq!(
    camera.camera_relative(Vec3::new(13.0, 2.0, -1.0)),
    Vec3::new(3.0, 2.0, -1.0)
);
let clip = camera.projection_reversed_z() * Vec4::new(0.0, 0.0, -camera.near, 1.0);
assert!((clip.z / clip.w - 1.0).abs() < 1e-4);
```

#### `RenderScene::new() -> Self`

Пустая сцена; буферы переиспользуются между кадрами.

#### `RenderScene::push(&mut self, current, previous, mesh, material)`

Добавляет инстанс: позиции текущего и предыдущего тика, mesh/material id.

#### `RenderScene::interpolate(&mut self, alpha)`

Считает camera-relative позиции: `previous.lerp(current, alpha) - camera_position`.
`alpha` ограничивается `0..=1`.

```rust
let mut scene = RenderScene::new();
scene.set_camera_position(Vec3::new(5.0, 0.0, 0.0));
scene.push(Vec3::new(10.0, 0.0, 0.0), Vec3::new(8.0, 0.0, 0.0), 1, 2);
scene.interpolate(0.5);
assert_eq!(scene.render_positions(), &[Vec3::new(4.0, 0.0, 0.0)]);
```

Геттеры: `len`, `is_empty`, `capacity`, `camera_position`, `render_positions`,
`current_positions`, `previous_positions`, `mesh_ids`, `material_ids`;
`clear()` сохраняет ёмкость.

### PBR, G-buffer, тон-маппинг и пост-обработка ✅

CPU-эталоны I5: те же формулы используются GPU-шейдерами после I6 и
golden-image-проверками. Модули: `render::pbr`, `render::gbuffer`,
`render::tonemap`, `render::post`.

#### `Material`

Поля `base_color` (linear), `metallic` `0..=1`, `roughness` `0.04..=1`.
Конструкторы `new` (санитизация), `dielectric(base_color, roughness)`,
`metal(base_color, roughness)`; `f0()` — `0.04` для диэлектрика, `base_color`
для металла (lerp по metallic).

```rust
let metal = Material::metal(Vec3::new(1.0, 0.2, 0.1), 0.3);
assert_eq!(metal.f0(), Vec3::new(1.0, 0.2, 0.1));
let dielectric = Material::dielectric(Vec3::ONE, 0.5);
assert_eq!(dielectric.f0(), Vec3::splat(0.04));
```

#### Функции PBR

- `distribution_ggx(n_dot_h, roughness)` — Trowbridge-Reitz.
- `visibility_smith_height_correlated(n_dot_v, n_dot_l, roughness)` —
  `G / (4 n·l n·v)`.
- `fresnel_schlick(cos_theta, f0)`.
- `multiscatter_energy(f0, roughness, n_dot_v)` — компенсация Karis.
- `evaluate_direct(material, normal, view, light, radiance)` — сумма
  Lambert-диффуза и GGX-спекуляра, умноженная на `n·l` и radiance.

```rust
let material = Material::new(Vec3::splat(0.5), 0.5, 0.5);
let color = evaluate_direct(material, Vec3::Z, Vec3::Z, Vec3::Z, Vec3::splat(1.0));
assert!(color.is_finite() && color.x >= 0.0);
```

#### G-buffer ABI (ADR-0017)

`ALBEDO_FORMAT = Rgba8Srgb`, `NORMAL_FORMAT = Rgba16Float`,
`ORM_FORMAT = Rgba8Unorm`, `DEPTH_FORMAT = D32Float`; `COLOR_ATTACHMENTS = 3`,
слоты `ALBEDO_SLOT`/`NORMAL_SLOT`/`ORM_SLOT` = `0`/`1`/`2`. `GbufferLayout`
хранит форматы и отдаёт `color_formats() -> [TextureFormat; 3]`. Расширения
(clearcoat) живут в bindless-индексе RT2 и не меняют layout.

```rust
let layout = GbufferLayout::default();
assert_eq!(
    layout.color_formats(),
    [
        TextureFormat::Rgba8Srgb,
        TextureFormat::Rgba16Float,
        TextureFormat::Rgba8Unorm,
    ]
);
```

#### `Tonemap`

`Aces` (Narkowicz, по умолчанию) и `Agx` (приближение: log2-нормализация +
smoothstep). `apply(color, exposure)` умножает на `2^exposure` и возвращает
`0..=1`. Отдельные функции: `aces(color)`, `agx(color)`.

```rust
let mapped = Tonemap::Aces.apply(Vec3::splat(4.0), 0.0);
assert!((0.0..=1.0).contains(&mapped.x));
assert!(Tonemap::Agx.apply(Vec3::splat(0.1), 1.0).x > 0.0);
```

#### Пост-обработка

- `bloom_threshold(color, threshold)` — `max(color - threshold, 0)`.
- `bloom_combine(base, bloom, intensity)` — `base + bloom * intensity`.
- `taa_blend(history, current, alpha)` — lerp с clamp `0..=1`.
- `ssao_factor(center_depth, sample_depths, radius)` — доля перекрытых
  сэмплов; `1.0` — нет окклюзии, `0.0` — полная (Reversed-Z: ближе = больше).

```rust
assert_eq!(bloom_threshold(Vec3::splat(1.5), 1.0), Vec3::splat(0.5));
assert_eq!(taa_blend(Vec3::ZERO, Vec3::ONE, 0.5), Vec3::splat(0.5));
assert_eq!(ssao_factor(1.0, &[], 0.1), 1.0);
```

GPU-проходы (G-buffer → lighting → tonemap → bloom/TAA/SSAO) появятся
после DXC-пайплайна (I6b); контракты формул при этом не меняются.

### Шейдерные пермутации ✅

`shader.json` — декларация рядом с исходником (ADR-0014, ADR-0036):
stages, pipeline-варианты, три набора keywords (pipeline/mesh/material),
vertex inputs layout v1 и биндинги. Runtime-ядро не компилирует HLSL:
оно принимает `ShaderCompiler` и гарантирует fallback вместо крэша.
Порядок обхода — `BTreeMap`, поэтому ключи и статистика детерминированы.

#### `ShaderDeclaration`

Поля: `name`, `shader_uuid`, `stages`, `pipeline_variants`, `keywords`,
`vertex_inputs`, `bindings`. Методы: `from_json`, `to_json`, `validate`,
`keyword_index`, `keywords_of`, `material_keyword_count`.

Инварианты: UUID ≠ 0; есть vertex и fragment/compute; ≤ 64 keywords
(`MAX_KEYWORDS`); ≤ 6 material-keywords (`MAX_MATERIAL_KEYWORDS`);
уникальные имена keywords, вариантов, vertex location и `(group, binding)`;
вариант ссылается только на pipeline-keywords; формат вершины —
`R32G32B32_SFLOAT` (`VERTEX_FORMAT_V1`).

```json
{
  "name": "pbr",
  "shader_uuid": 42,
  "stages": { "vertex": "pbr.vert.hlsl", "fragment": "pbr.frag.hlsl" },
  "pipeline_variants": [
    { "name": "deferred_gbuffer", "keywords": ["CLUSTERED_LIGHTING"] }
  ],
  "keywords": [
    { "name": "CLUSTERED_LIGHTING", "set": "pipeline", "default": true },
    { "name": "SKINNING", "set": "mesh" },
    { "name": "NORMAL_MAP", "set": "material" }
  ],
  "vertex_inputs": [ { "location": 0, "format": "R32G32B32_SFLOAT" } ],
  "bindings": [ { "name": "albedo", "group": 0, "binding": 0 } ]
}
```

#### `KeywordMask`

Битовая маска: бит = индекс keyword в декларации. `empty`, `from_bits`,
`bits`, `len`, `contains`, `with`, `union`, `intersects`;
`from_names(declaration, set, names)` возвращает
`ShaderError::UnknownKeyword`, если имени нет или набор другой.

#### `PermutationRequest`

Полный ключ `pipeline × mesh × material`: `shader_uuid`,
`pipeline_variant`, `mesh`, `material`; билдеры `with_mesh`/`with_material`;
`hash()` — FNV-1a по UUID, варианту и маскам (стабилен между процессами).

#### `ShaderCompiler`

`compile(declaration, request) -> Result<Vec<u8>, ShaderError>`.
Реальный компилятор — `tools/flux-assetc shader` (DXC, ADR-0038).
`DeterministicCompiler` — тестовая заглушка: детерминированный байткод
для кэша и fallback, **не валидный SPIR-V**.

#### `ShaderLibrary`

`new(max_permutations)`, `register`, `precompile_core`, `resolve`, `stats`,
`cache`/`cache_mut`, `live_permutations`, `max_permutations`,
`declaration`, `core_len`, `len`/`is_empty`.

```rust
let json = r#"{
  "name": "pbr",
  "shader_uuid": 42,
  "stages": { "vertex": "pbr.vert.hlsl", "fragment": "pbr.frag.hlsl" },
  "pipeline_variants": [ { "name": "forward" } ],
  "keywords": [ { "name": "NORMAL_MAP", "set": "material" } ],
  "vertex_inputs": [ { "location": 0, "format": "R32G32B32_SFLOAT" } ],
  "bindings": []
}"#;
let declaration = ShaderDeclaration::from_json(json).unwrap();
let mut library = ShaderLibrary::new(64);
library.register(declaration).unwrap();
let compiler = DeterministicCompiler;
library.precompile_core(&compiler).unwrap();

let request = PermutationRequest::new(42, 0);
let resolved = library.resolve(&request, &compiler).unwrap();
assert!(!resolved.fallback);
assert_eq!(library.stats().hits, 1);
```

#### Fallback и кэш

`precompile_core` собирает ядро: каждый pipeline-вариант без mesh/material
keywords. `resolve` идёт по цепочке кэш → компиляция → fallback: неизвестный
вариант, маска чужого набора, ошибка компилятора и превышение бюджета отдают
fallback и увеличивают `CacheStats::fallbacks`/`budget_exceeded`;
незарегистрированный UUID — `ShaderError::UnknownShader`.

`PermutationCache` держит три уровня раздельно (ADR-0014): байткод
(`hash(исходник + keywords)`), PSO (`hash(байткод + state + форматы)`) и
материал (`(shader_uuid, material-маска)`).

DXC-компиляция HLSL → SPIR-V — `tools/flux-assetc shader` (поиск
`DXC_PATH` → рядом с бинарником → `tools/dxc/bin` → `PATH`; ADR-0038).
Runtime получает готовый SPIR-V; `DeterministicCompiler` используется только
в тестах.

Конвенция регистров (ADR-0039): `DescriptorBinding::binding(N)` ↔ `bN`
(uniform), `uN` (storage buffer), `tN` (sampled/storage image); всё в `set 0`.
Другая раскладка ломает соответствие layout-а и HLSL.

### GraphicsSettings v1 ✅

Пресеты — данные (ADR-0016): `HardwareTier` выводится из `Capabilities`,
`QualityPreset` задаёт гранулярность, `GraphicsSettings::diff` сообщает, что
перестроить. Device при смене настроек не пересоздаётся — такого флага в
`SettingsChange` нет намеренно.

#### `HardwareTier`

`Low`/`Medium`/`High`/`Ultra`; `from_capabilities(&Capabilities)` —
детерминированная эвристика (размер текстур, bindless, timeline semaphores);
`preset()` — пресет по умолчанию для яруса.

#### `QualityPreset`

`Low`/`Medium`/`High`/`Ultra` (по умолчанию `Medium`):
`shadow_resolution()` (512/1024/2048/4096), `render_scale()`
(0.75/1.0/1.0/1.25), `ssao()`, `bloom()`, `taa()`.

#### `GraphicsSettings`

Поля: `preset`, `shadow_resolution`, `render_scale`, `ssao`, `bloom`, `taa`,
`vsync`, `present_mode`. `from_preset`, `from_tier`, `sanitize`/`validated`
(clamp `0.25..=2.0` и `256..=8192`, vsync → `Fifo`), `diff`, `apply`.
Сериализуется (JSON/RON) — настройки персистентны.

```rust
let mut settings = GraphicsSettings::from_preset(QualityPreset::Medium);
let next = GraphicsSettings::from_preset(QualityPreset::High);
let change = settings.apply(next);
assert!(change.pipelines && change.graph && !change.swapchain);
assert_eq!(settings.shadow_resolution, 2048);
```

#### `SettingsChange`

`swapchain` (vsync/режим представления), `graph` (SSAO/bloom/TAA/тени),
`pipelines` (пресет → набор keywords). `none()`, `any()`.
Device не входит в структуру намеренно.

#### `GraphicsFeatureRegistry`

`new(&Capabilities)` — фиксированный порядок фич: `ssao`, `bloom`, `taa`,
`shadow_cascades`, `bindless`, `timeline_semaphores`, `mailbox`;
`entries`, `get`, `supported`. `GraphicsFeature { name, kind, levels,
supported }`, `FeatureKind::{Bool, Level}`.

```rust
let registry = GraphicsFeatureRegistry::new(&Capabilities::headless());
assert!(registry.get("taa").is_some_and(|feature| feature.supported));
assert!(!registry.get("mailbox").expect("mailbox").supported);
```

### Golden-image и GPU-зоны ✅

CPU-харнесс ADR-0021: сравнение кадров детерминировано, бейзлайны —
per-runner, обновление существующего бейзлайна требует `visual_change`.

Golden-политика (ADR-0039): Null-бэкенд сравнивается bit-exact; GPU-бэкенды —
на референс-устройстве с допуском (per-pixel diff и доля выбросов). Побитовое
равенство между драйверами/вендорами не требуется и не проверяется.

#### `Rgba8Image`

`new(width, height, pixels)` проверяет длину (`width * height * 4`),
`filled`, `pixel`, `len`. Ошибки — `GoldenError::{InvalidImage,
DimensionMismatch, MissingBaseline, VisualChangeRequired}`.

#### `psnr` / `ssim` / `diff_heatmap`

`psnr(reference, actual)` — по RGB-каналам; идентичные изображения дают
`f64::INFINITY`. `ssim(reference, actual)` — глобальный SSIM по luma BT.709
(идентичные → `1.0`). `diff_heatmap` — `|diff| × 8` по RGB, alpha `255`.

#### `GoldenSettings` / `GoldenStore`

`GoldenSettings { psnr: 60.0, ssim: 0.99 }` с `validated`.
`GoldenStore`: `update(runner, image, settings, visual_change)`,
`compare(runner, actual) -> GoldenVerdict`, `diff`, `baseline`, `len`;
сериализуется в JSON/RON. Неизвестный раннер — `MissingBaseline`.

```rust
let mut store = GoldenStore::new();
let settings = GoldenSettings::default();
let frame = Rgba8Image::filled(4, 4, [0, 0, 0, 255]);
store.update("windows-vulkan", frame.clone(), settings, false).unwrap();
let verdict = store.compare("windows-vulkan", &frame).unwrap();
assert!(verdict.passed);
assert_eq!(verdict.psnr, f64::INFINITY);
```

#### `GpuZone` / `GpuProfiler`

`GpuZone::new(name, start_ns, end_ns)` нормализует `end >= start`;
`duration_ns`. `GpuProfiler::new(capacity)`, `push` (вытесняет старейшую
зону), `push_from_timestamps(name, &[u64])` (пары start/end → зоны),
`zones`, `total_ns`, `zone_ns(name)`.

```rust
let mut profiler = GpuProfiler::new(8);
profiler.push(GpuZone::new("gbuffer", 0, 100));
profiler.push_from_timestamps("lighting", &[100, 250, 250, 400]);
assert_eq!(profiler.total_ns(), 400);
assert_eq!(profiler.zone_ns("lighting"), 300);
```

Таймстемпы приходят из query pool-ов RHI (ADR-0037):
`QueryPoolDesc`/`QueryPoolId`, `create_query_pool`/`read_query_pool`,
`reset_query_pool`/`write_timestamp`. Чтение использует `WAIT` и выполняется
между кадрами — это dev/CI-профиль, не горячий путь.

G4-гейт `tests/golden_gate.rs` сравнивает детерминированный кадр с
per-runner baseline и краснит сборку при расхождении без `visual_change`.

## flux-engine

Фасад: сборка, цикл кадра, плагины, ECS-интеграция KCC, extract для рендера,
детерминизм-харнесс.

Импорт:

```rust
use flux_core::contracts::{AudioDevice, NetBackend, Platform, Renderer, ScriptVM};
use flux_core::memory::FrameArena;
use flux_engine::{
    controller_post_system, controller_system, harness_scene, install_controller_schedule,
    CharacterBody, DeterminismHarness, Engine, EngineBuilder, EngineConfig, EngineContext,
    EngineError, ExtractStats, FrameStats, GfxBackend, GraphicsQuality, MeshRenderer, OverlayData,
    PhysicsStep, Plugin, RenderExtractor, RenderTransform, RunSummary, StateDiff, StateHash,
    TickResult, UserSettings, CONTROLLER_POST_SYSTEM, CONTROLLER_SYSTEM,
};
```

### Bootstrap ✅

#### `GfxBackend`

Графический бэкенд из конфига: `Null` (по умолчанию), `Vulkan`, `Dx12`.

```rust
assert_eq!(GfxBackend::default(), GfxBackend::Null);
```

#### `EngineConfig`

Конфигурация bootstrap: `fixed_step`, `max_steps_per_frame`,
`worker_threads`, `log_filter`, `logging`, `crash_handler`, `dump_dir`,
`backend`, `settings`, `throttle`.

```rust
let config = EngineConfig::default();
assert_eq!(config.max_steps_per_frame, 8);
assert!(config.logging);
```

#### `EngineConfig::default()`

Значения по умолчанию: шаг 60 Гц, 8 шагов, авто-воркеры, `info`, `Null`,
`throttle = true`.

```rust
let config = EngineConfig::default();
assert_eq!(config.worker_threads, 0);
assert_eq!(config.backend, GfxBackend::Null);
```

#### `EngineConfig::validate(&self) -> Result<(), String>`

Проверяет ненулевой шаг, ненулевой лимит шагов, разумное число воркеров и
валидность `UserSettings`.

```rust
let mut config = EngineConfig::default();
config.fixed_step = core::time::Duration::ZERO;
assert!(config.validate().is_err());
```

#### `EngineError`

Ошибка сборки: `InvalidConfig(String)` или `Logging(String)`.

```rust
let error = EngineError::InvalidConfig("bad".to_owned());
assert!(error.to_string().contains("invalid"));
```

#### `EngineBuilder::new() -> Self`

Сборщик с конфигурацией по умолчанию и без плагинов.

```rust
let builder = EngineBuilder::new();
let _ = builder;
```

#### `EngineBuilder::with_config(self, config) -> Self`

Заменяет конфигурацию целиком.

```rust
let config = EngineConfig::default();
let builder = EngineBuilder::new().with_config(config);
let _ = builder;
```

#### `EngineBuilder::with_plugin<P>(self, plugin) -> Self`

Регистрирует плагин; порядок регистрации — порядок вызовов шагов.

```rust
struct Noop;
impl Plugin for Noop {
    fn name(&self) -> &str { "noop" }
    fn build(&self, _ctx: &mut EngineContext) {}
}
let builder = EngineBuilder::new().with_plugin(Noop);
let _ = builder;
```

#### `EngineBuilder::with_worker_threads(self, workers) -> Self`

Число фоновых воркеров; `0` — авто (`available_parallelism - 1`).

```rust
let builder = EngineBuilder::new().with_worker_threads(2);
let _ = builder;
```

#### `EngineBuilder::with_backend(self, backend) -> Self`

Выбирает графический бэкенд.

```rust
let builder = EngineBuilder::new().with_backend(GfxBackend::Null);
let _ = builder;
```

#### `EngineBuilder::with_settings(self, settings) -> Self`

Пользовательские настройки.

```rust
let builder = EngineBuilder::new().with_settings(UserSettings::default());
let _ = builder;
```

#### `EngineBuilder::with_crash_handler(self, dump_dir) -> Self`

Включает паник-хук с breadcrumbs и текстовым дампом.

```rust
let builder = EngineBuilder::new().with_crash_handler("crash-dumps");
let _ = builder;
```

#### `EngineBuilder::build(self) -> Result<Engine, EngineError>`

Собирает движок: конфиг → логи → паник-хук → job system → плагины.

```rust
let engine = EngineBuilder::new().build().unwrap();
assert!(engine.is_running());
```

#### `EngineBuilder::run(self) -> !`

Bootstrap → цикл → shutdown. Окно и event loop не трогает.

```rust
// EngineBuilder::new().run();
```

#### `EngineBuilder::run_frames(self, frames) -> Result<RunSummary, EngineError>`

Собирает движок и прогоняет ровно `frames` кадров без throttle.

```rust
let summary = EngineBuilder::new().run_frames(2).unwrap();
assert_eq!(summary.frames, 2);
```

### Runtime ✅

#### `TickResult`

Результат шага симуляции: `{ steps, sim_tick, alpha, dropped_steps }`.

```rust
let result = TickResult::default();
assert_eq!(result.steps, 0);
```

#### `FrameStats`

Статистика кадра: `{ frame_index, frame_ms, fps, p50_ms, p99_ms, jobs,
panicked_jobs }`.

```rust
let stats = FrameStats::default();
assert_eq!(stats.frame_index, 0);
```

#### `RunSummary`

Итог прогона: `{ frames, ticks, panicked_jobs }`.

```rust
let summary = RunSummary::default();
assert_eq!(summary.frames, 0);
```

#### `Engine::tick(&mut self) -> TickResult`

Обновляет время и выполняет 0..N фиксированных шагов. Пауза — отсутствие
ненулевых шагов.

```rust
let mut engine = EngineBuilder::new().build().unwrap();
let result = engine.tick();
assert!(result.steps <= 8);
```

#### `Engine::frame(&mut self) -> FrameStats`

Кадр: сброс арены → публикация событий → переменные системы → fence job-ов
→ телеметрия.

```rust
let mut engine = EngineBuilder::new().build().unwrap();
let stats = engine.frame();
assert_eq!(stats.frame_index, 1);
```

#### `Engine::acquire(&mut self)`

Ожидание GPU. Отдельный шаг: редактор в edit-режиме может пропускать. Сейчас
стаб до RHI этапа 4.

```rust
let mut engine = EngineBuilder::new().build().unwrap();
engine.acquire();
```

#### `Engine::run_loop(mut self) -> !`

Прогоняет цикл до `request_shutdown`, затем штатно завершает процесс.

```rust
// engine.run_loop();
```

#### `Engine::run_frames(mut self, frames) -> RunSummary`

Прогоняет ровно `frames` кадров без throttle (headless, тесты, soak).

```rust
let summary = EngineBuilder::new().build().unwrap().run_frames(3);
assert_eq!(summary.frames, 3);
```

#### `Engine::request_shutdown(&mut self)`

Останавливает `run_loop` после текущего кадра.

```rust
let mut engine = EngineBuilder::new().build().unwrap();
engine.request_shutdown();
assert!(!engine.is_running());
```

#### `Engine::is_running(&self) -> bool`

Признак работы цикла.

```rust
let engine = EngineBuilder::new().build().unwrap();
assert!(engine.is_running());
```

#### `Engine::shutdown(&mut self)`

Штатная остановка: `Plugin::shutdown` и очистка очередей событий.

```rust
let mut engine = EngineBuilder::new().build().unwrap();
engine.shutdown();
assert!(!engine.is_running());
```

#### `Engine::config(&self) -> &EngineConfig`

Конфигурация движка.

```rust
let engine = EngineBuilder::new().build().unwrap();
assert_eq!(engine.config().backend, GfxBackend::Null);
```

#### `Engine::time(&self) -> &TimeContext`

Контекст времени (пауза, масштаб, тик).

```rust
let engine = EngineBuilder::new().build().unwrap();
assert_eq!(engine.time().sim_tick(), flux_core::time::SimTick::ZERO);
```

#### `Engine::time_mut(&mut self) -> &mut TimeContext`

Мутабельный контекст времени: пауза и time scale из UI/скриптов.

```rust
let mut engine = EngineBuilder::new().build().unwrap();
engine.time_mut().set_paused(true);
assert!(engine.time().is_paused());
```

#### `Engine::settings(&self) -> &UserSettings`

Пользовательские настройки.

```rust
let engine = EngineBuilder::new().build().unwrap();
assert!(engine.settings().vsync);
```

#### `Engine::settings_mut(&mut self) -> &mut UserSettings`

Мутабельные настройки: применяются к подсистемам на следующем кадре.

```rust
let mut engine = EngineBuilder::new().build().unwrap();
engine.settings_mut().vsync = false;
assert!(!engine.settings().vsync);
```

#### `Engine::jobs(&self) -> &JobSystem`

Job system движка.

```rust
let engine = EngineBuilder::new().build().unwrap();
assert!(engine.jobs().worker_count() >= 1);
```

#### `Engine::events(&self) -> &EventRegistry`

Реестр событий.

```rust
let engine = EngineBuilder::new().build().unwrap();
assert!(engine.events().is_empty::<u8>());
```

#### `Engine::frame_index(&self) -> u64`

Номер последнего кадра.

```rust
let mut engine = EngineBuilder::new().build().unwrap();
let _ = engine.frame();
assert_eq!(engine.frame_index(), 1);
```

#### `Engine::overlay(&self) -> OverlayData`

Read-only снимок dev-overlay: тик, последний `FrameStats`, число воркеров,
опционально число сущностей. Не меняет время и порядок систем.

```rust
let engine = EngineBuilder::new().build().unwrap();
let overlay = engine.overlay();
assert!(overlay.summary().contains("tick"));
```

#### `Engine::set_entity_count(&mut self, count)`

Сообщает движку число сущностей для overlay.

```rust
let mut engine = EngineBuilder::new().build().unwrap();
engine.set_entity_count(Some(7));
assert_eq!(engine.overlay().entities, Some(7));
```

### Настройки и overlay ✅

#### `GraphicsQuality`

Пресет качества: `Low`, `Medium` (по умолчанию), `High`, `Ultra`.

```rust
assert_eq!(GraphicsQuality::default(), GraphicsQuality::Medium);
```

#### `GraphicsQuality::shadow_resolution(self) -> u32`

Разрешение карты теней в текселях: 512/1024/2048/4096.

```rust
assert_eq!(GraphicsQuality::Low.shadow_resolution(), 512);
assert_eq!(GraphicsQuality::Ultra.shadow_resolution(), 4096);
```

#### `GraphicsQuality::render_scale(self) -> f32`

Множитель разрешения рендера: 0.75/1.0/1.0/1.25.

```rust
assert_eq!(GraphicsQuality::Low.render_scale(), 0.75);
```

#### `UserSettings`

Пользовательские настройки: `{ graphics, vsync, master_volume }`.

```rust
let settings = UserSettings::default();
assert!(settings.vsync);
```

#### `UserSettings::default()`

Значения по умолчанию: `Medium`, `vsync = true`, громкость `0.8`.

```rust
let settings = UserSettings::default();
assert_eq!(settings.master_volume, 0.8);
```

#### `UserSettings::validate(&self) -> Result<(), String>`

Проверяет, что `master_volume` в `0.0..=1.0`.

```rust
let mut settings = UserSettings::default();
settings.master_volume = 1.5;
assert!(settings.validate().is_err());
```

#### `OverlayData`

Снимок overlay: `{ sim_tick, frame, worker_threads, entities }`.

```rust
let overlay = OverlayData::default();
assert_eq!(overlay.worker_threads, 0);
```

#### `OverlayData::summary(&self) -> String`

Однострочная сводка для заголовка окна/лога: тик, кадр, FPS, frametime,
воркеры, сущности.

```rust
let overlay = OverlayData::default();
let text = overlay.summary();
assert!(text.contains("tick 0"));
assert!(text.contains("workers 0"));
```

### Plugin ✅

#### `Plugin::name(&self) -> &str`

Имя плагина для логов и диагностики.

```rust
struct Game;
impl Plugin for Game {
    fn name(&self) -> &str { "game" }
    fn build(&self, _ctx: &mut EngineContext) {}
}
assert_eq!(Game.name(), "game");
```

#### `Plugin::build(&self, ctx)`

Регистрирует ресурсы и подписки. Вызывается один раз до первого кадра.

```rust
struct Game;
impl Plugin for Game {
    fn name(&self) -> &str { "game" }
    fn build(&self, ctx: &mut EngineContext) {
        let _ = ctx.time().sim_tick();
    }
}
```

#### `Plugin::update_fixed(&mut self, ctx, step)`

Фиксированный шаг симуляции; может выполниться несколько раз за кадр.

```rust
struct Game;
impl Plugin for Game {
    fn name(&self) -> &str { "game" }
    fn build(&self, _ctx: &mut EngineContext) {}
    fn update_fixed(&mut self, ctx: &mut EngineContext, step: u32) {
        let _ = (ctx.time().sim_tick(), step);
    }
}
```

#### `Plugin::update_variable(&mut self, ctx)`

Переменный шаг кадра: UI, стриминг, рендер-подготовка.

```rust
struct Game;
impl Plugin for Game {
    fn name(&self) -> &str { "game" }
    fn build(&self, _ctx: &mut EngineContext) {}
    fn update_variable(&mut self, ctx: &mut EngineContext) {
        let _ = ctx.settings().vsync;
    }
}
```

#### `Plugin::shutdown(&mut self)`

Освобождает ресурсы перед остановкой движка.

```rust
struct Game;
impl Plugin for Game {
    fn name(&self) -> &str { "game" }
    fn build(&self, _ctx: &mut EngineContext) {}
    fn shutdown(&mut self) {}
}
```

### EngineContext ✅

#### `EngineContext::time(&self) -> &TimeContext`

Контекст времени: тик, alpha, пауза, масштаб.

```rust
fn read_time(ctx: &EngineContext) {
    let _ = ctx.time().sim_tick();
}
```

#### `EngineContext::events(&self) -> &EventRegistry`

Реестр типизированных событий.

```rust
fn read_events(ctx: &EngineContext) {
    let _ = ctx.events().is_empty::<u8>();
}
```

#### `EngineContext::jobs(&self) -> &JobSystem`

Job system.

```rust
fn read_jobs(ctx: &EngineContext) {
    let _ = ctx.jobs().worker_count();
}
```

#### `EngineContext::settings(&self) -> &UserSettings`

Пользовательские настройки.

```rust
fn read_settings(ctx: &EngineContext) {
    let _ = ctx.settings().master_volume;
}
```

#### `EngineContext::arena(&mut self) -> &mut FrameArena`

Frame-арена: сбрасывается в начале кадра.

```rust
fn use_arena(ctx: &mut EngineContext) {
    let _ = ctx.arena().alloc(16, 4);
}
```

#### `EngineContext::renderer(&self) -> Option<&dyn Renderer>`

Рендер-бэкенд как контракт (без деталей RHI).

```rust
fn read_renderer(ctx: &EngineContext) {
    let _ = ctx.renderer().map(|renderer| renderer.backend_name());
}
```

#### `EngineContext::platform(&self) -> Option<&dyn Platform>`

Платформенный слой.

```rust
fn read_platform(ctx: &EngineContext) {
    let _ = ctx.platform().map(|platform| platform.name());
}
```

#### `EngineContext::audio(&self) -> Option<&dyn AudioDevice>`

Аудио-устройство.

```rust
fn read_audio(ctx: &EngineContext) {
    let _ = ctx.audio().map(|audio| audio.sample_rate());
}
```

#### `EngineContext::net(&self) -> Option<&dyn NetBackend>`

Сетевой транспорт.

```rust
fn read_net(ctx: &EngineContext) {
    let _ = ctx.net().map(|net| net.name());
}
```

#### `EngineContext::script(&self) -> Option<&dyn ScriptVM>`

Скриптовая VM.

```rust
fn read_script(ctx: &EngineContext) {
    let _ = ctx.script().map(|script| script.name());
}
```

#### `EngineContext::spawn<F>(&mut self, job)`

Ставит задачу в job system и регистрирует её для fence кадра. Паника внутри
задачи не роняет воркер.

```rust
fn spawn_work(ctx: &mut EngineContext) {
    ctx.spawn(|| {
        let _ = 1 + 1;
    });
}
```

### Extract для рендера ✅

#### `RenderTransform`

Компонент: `{ current: Vec3, previous: Vec3 }` — позиции текущего и
предыдущего тика для alpha интерполяции.

#### `MeshRenderer`

Компонент: `{ mesh: u32, material: u32 }` — что рисовать.

#### `ExtractStats`

`{ entities: usize, elapsed: Duration }` — число инстансов и время
экстракции (гейт G4: ≤ 1 мс на эталонной сцене, замер — I8).

#### `RenderExtractor::new() -> Self`

Экстрактор с переиспользуемым буфером; после прогрева не аллоцирует.

#### `RenderExtractor::extract(&mut self, world, scene, camera, alpha) -> ExtractStats`

Заполняет `RenderScene`:

- итерирует `RenderTransform`, сортирует по `EntityUuid` (детерминизм);
- пропускает сущности без `MeshRenderer`;
- вычитает позицию камеры (camera-relative);
- интерполирует `previous → current` по `alpha`.

```rust
let mut world = World::new();
let entity = world.spawn_with_uuid(EntityUuid::new());
world
    .insert(
        entity,
        RenderTransform {
            current: Vec3::new(11.0, 0.0, 0.0),
            previous: Vec3::new(9.0, 0.0, 0.0),
        },
    )
    .unwrap();
world
    .insert(entity, MeshRenderer { mesh: 1, material: 2 })
    .unwrap();

let camera = Camera::new(Vec3::new(5.0, 0.0, 0.0), -Vec3::Z, Vec3::Y);
let mut scene = RenderScene::new();
let mut extractor = RenderExtractor::new();
let stats = extractor.extract(&mut world, &mut scene, &camera, 0.5);
assert_eq!(stats.entities, 1);
assert_eq!(scene.render_positions(), &[Vec3::new(5.0, 0.0, 0.0)]);
```

### ECS-интеграция KCC ✅

#### `PhysicsStep`

Ресурс фиксированного шага: `{ dt: f64 }`; по умолчанию `1.0 / 60.0`.

```rust
let step = PhysicsStep::default();
assert!((step.dt - 1.0 / 60.0).abs() < 1e-12);
```

#### `PhysicsStep::new(dt) -> Self`

Шаг симуляции в секундах.

```rust
assert_eq!(PhysicsStep::new(0.02).dt, 0.02);
```

#### `CharacterBody`

Компонент персонажа: `{ state, config, intention, hit }`.

```rust
let body = CharacterBody::default();
assert_eq!(body.intention, flux_core::input::Intention::IDLE);
```

#### `CONTROLLER_SYSTEM`

Имя системы Controller: `"controller"`.

```rust
assert_eq!(CONTROLLER_SYSTEM, "controller");
```

#### `CONTROLLER_POST_SYSTEM`

Имя пост-системы Controller: `"controller_post"`.

```rust
assert_eq!(CONTROLLER_POST_SYSTEM, "controller_post");
```

#### `controller_system(world, commands)`

Двигает персонажа по `Intention`: временно изымает `TileGrid` из ресурсов,
вызывает `move_character` для каждого `CharacterBody`, возвращает grid.

```rust
let mut world = flux_ecs::world::World::new();
let entity = world.spawn();
world.insert(entity, CharacterBody::default()).unwrap();
world.insert_resource(flux_physics::TileGrid::from_rows(&["####"]));
let mut commands = flux_ecs::commands::Commands::new();
controller_system(&mut world, &mut commands);
```

#### `controller_post_system(world, commands)`

Сбрасывает one-shot ввод после шага: `jump = false`, `hit = false`.

```rust
let mut world = flux_ecs::world::World::new();
let entity = world.spawn();
let mut body = CharacterBody::default();
body.intention.jump = true;
world.insert(entity, body).unwrap();
let mut commands = flux_ecs::commands::Commands::new();
controller_post_system(&mut world, &mut commands);
assert!(!world.get::<CharacterBody>(entity).unwrap().intention.jump);
```

#### `install_controller_schedule(schedule)`

Регистрирует Controller (`UPDATE`) и ControllerPost (`POST`) с доступом на
запись `CharacterBody`.

```rust
let mut schedule = flux_ecs::scheduler::Schedule::with_stages(&[
    flux_ecs::scheduler::Stage::UPDATE,
    flux_ecs::scheduler::Stage::POST,
]);
install_controller_schedule(&mut schedule);
assert_eq!(schedule.len(), 2);
```

### Детерминизм-харнесс ✅

#### `StateHash`

`{ hash: u64, tick: SimTick, journal: SaveJournal }` — хеш FNV-1a по
каноническому RON журнала.

```rust
let state = StateHash::default();
assert_eq!(state.hash, 0);
```

#### `StateHash::diff(&self, other) -> Option<StateDiff>`

Первое различие с другим состоянием; `None` — состояния идентичны.

```rust
let left = StateHash::default();
let right = StateHash::default();
assert!(left.diff(&right).is_none());
```

#### `StateDiff`

Первое различие: `{ tick, entity, schema, field, left, right }`.

```rust
let diff = StateDiff {
    tick: flux_core::time::SimTick::ZERO,
    entity: flux_ecs::entity::EntityUuid::new(),
    schema: flux_core::schema::SchemaRef::new(flux_core::schema::TypeId::new_v4(), 1),
    field: None,
    left: None,
    right: None,
};
assert!(diff.to_string().contains("tick"));
```

#### `DeterminismHarness::new(seed) -> Self`

Харнесс: `World` + `Schedule` + seeded-бот; сцена — пол и ступень.

```rust
let harness = DeterminismHarness::new(0x5EED);
assert_eq!(harness.tick(), flux_core::time::SimTick::ZERO);
```

#### `DeterminismHarness::step(&mut self)`

Один тик: бот → Controller → Physics → ControllerPost.

```rust
let mut harness = DeterminismHarness::new(1);
harness.step();
assert_eq!(harness.tick().get(), 1);
```

#### `DeterminismHarness::run(&mut self, ticks) -> StateHash`

Прогон `ticks` тиков и снятие хеша.

```rust
let mut harness = DeterminismHarness::new(1);
let hash = harness.run(10);
assert_eq!(hash.tick.get(), 10);
```

#### `DeterminismHarness::tick(&self) -> SimTick`

Текущий тик мира.

```rust
let harness = DeterminismHarness::new(1);
assert_eq!(harness.tick(), flux_core::time::SimTick::ZERO);
```

#### `DeterminismHarness::state_hash(&self) -> StateHash`

Канонический хеш текущего состояния: дельты по
`(entity_uuid, type_uuid, schema_rev)`, поля по `field_uuid`.

```rust
let harness = DeterminismHarness::new(1);
let hash = harness.state_hash();
assert_ne!(hash.hash, 0);
```

#### `harness_scene() -> TileGrid`

Сцена харнесса: пол и ступень.

```rust
let scene = harness_scene();
assert!(scene.width() > 0);
```

## Feature-гейты flux-platform

| Feature | Что включает | По умолчанию |
|---|---|---|
| `windowing` | `winit`-адаптер окна/event loop | выключен |
| `devices` | `gilrs`, hot-plug, хаптика | выключен |
| `crash-dump` | per-OS дампы (модуль-заглушка) | выключен |

Headless-сборка (без `windowing`) — обязательный CI-профиль (ADR-0033).
Clipboard и текст пока headless (`MemoryClipboard`, `TextInput`); системные
backend-ы появятся вместе с реальным окном этапа 4.

## Пример M3 ✅

`crates/flux-engine/examples/m3.rs` — `winit` 0.30 (`ApplicationHandler`) +
`softbuffer` 0.4. Каждый `RedrawRequested` двигает `DeterminismHarness` на
тик, рисует прямоугольник и обновляет заголовок `tick=… hash=…` со сводкой
`Engine::overlay`.

```rust
let mut app = App::new();
let event_loop = EventLoop::new().unwrap();
event_loop.set_control_flow(ControlFlow::Poll);
event_loop.run_app(&mut app).unwrap();
```

Запуск:

```text
cargo run -p flux-engine --example m3
```

## tools/flux-release

Релиз-инструмент (ADR-0040): Conventional Commits → semver bump → версия в
`Cargo.toml` → `CHANGELOG.md` → commit + annotated tag. Без Node: только
`git`, `cargo` и опциональные `gh`/`cargo-semver-checks`.

Импорт библиотеки:

```rust
use flux_release::{
    apply, parse_commit, preview, read_version, update_path_dependencies, write_version, Bump,
    Git, Locale, Options, ReleaseConfig, ReleaseError,
};
```

### CLI ✅

| Команда | Назначение |
|---|---|
| `release` | bump, changelog, commit, annotated tag; `--package`/`--publish`/`--push`/`--github-release` — opt-in |
| `version` | печатает следующую версию (read-only) |
| `changelog` | печатает запись changelog (read-only) |
| `check` | проверяет Conventional Commits; `--commits from..to`, `--strict` → exit 1 |
| `init` | создаёт `.releaserc.toml` и `CHANGELOG.md` (`--force`, `--locale en|ru`) |
| `doctor` | git/config/версия/наличие `cargo`, `gh`, `cargo-semver-checks` |

Общие флаги: `--root`, `--config`, `--json`, `--dry-run`.
Диапазон и версия: `--from`, `--to`, `--commits from..to`, `--tag-prefix`,
`--template <path>`, `--prerelease alpha|beta|rc`, `--first-release`.
Флаги `release`: `--no-verify`, `--no-git-checks`, `--package`, `--publish`,
`--push`, `--github-release`, `--semver-checks`, `--ci`.

`--github-release` идемпотентен: перед созданием проверяется существование
релиза (`gh release view` или `GET /repos/{slug}/releases/tags/{tag}`). Если
версия уже выпущена (тег есть), повторный прогон не меняет версию и changelog,
но создаёт отсутствующий GitHub Release — это восстанавливает релиз, прерванный
после пуша тега.

```bash
cargo run -p flux-release -- release --dry-run
cargo run -p flux-release -- release --prerelease beta
cargo run -p flux-release -- check --commits "origin/main..HEAD" --strict
cargo run -p flux-release -- version --json
```

Exit code: `0` — успех или штатный no-op; `1` — ошибка, а также нарушения
`check --strict`.

### `Options` ✅

Параметры запуска для библиотечного API: `root`, `config_path`, `dry_run`,
`json`, `from`, `to`, `commits`, `prerelease`, `tag_prefix`, `template`,
`no_verify`, `no_git_checks`, `package`, `publish`, `push`, `github_release`,
`semver_checks`, `first_release`, `ci`.
`Options::default()` — пустой корень и все переключатели выключены. `ci`
отключает git-хуки (как `--no-verify`); `push`/`package`/`publish`/
`github_release` остаются opt-in.

### `ReleaseConfig` ✅

```rust
pub struct ReleaseConfig {
    pub branches: Vec<String>,          // ["dev", "main", "master"]
    pub tag_format: String,             // "v{version}", обязан содержать {version}
    pub changelog: PathBuf,             // "CHANGELOG.md"
    pub initial_version: String,        // "0.1.0"
    pub mode: PackageMode,              // Fixed | Independent
    pub locale: Locale,                 // En | Ru
    pub template: Option<PathBuf>,      // внешний шаблон changelog
    pub packages: Vec<PackageConfig>,   // name + manifest для монорепо
    pub commit_types: BTreeMap<String, CommitTypeConfig>,
    pub skip_markers: Vec<String>,      // ["[skip release]"]
    pub bump_major_on_breaking_below_1_0: bool,
    pub semver_checks: bool,
}
```

Методы: `load(root, explicit)` (явный путь → `.releaserc.toml`/`release.toml`
→ умолчания; поверх — импорт subset `.releaserc.json`), `import_json`,
`validate`, `tag_for(version)`, `tag_prefix`, `initial_version`, `commit_type`,
`is_skipped`, `changelog_path`, `manifests`. Неизвестные поля в TOML —
ошибка (`deny_unknown_fields`).

```toml
branches = ["dev", "main", "master"]
tag_format = "v{version}"
changelog = "CHANGELOG.md"
initial_version = "0.1.0"
mode = "fixed"          # или "independent"
locale = "en"           # или "ru"
# bump_major_on_breaking_below_1_0 = true
# semver_checks = true

# [[packages]]
# name = "demo"
# manifest = "crates/demo/Cargo.toml"

# [commit_types]
# feat = { section = "Added", bump = "minor" }
```

Стандартная таблица `default_commit_types()`: `feat` → Added/minor,
`fix`/`revert` → Fixed/patch, `perf` → Performance/patch,
`refactor` → Changed/none, `docs` → Documentation/none,
`build`/`ci`/`test`/`chore`/`style` → Internal/none.

### `ConventionalCommit` ✅

```rust
pub struct ConventionalCommit {
    pub hash: String,
    pub short_hash: String,
    pub date: String,             // YYYY-MM-DD из git
    pub kind: String,             // feat, fix, ...
    pub scope: Option<String>,
    pub description: String,
    pub breaking: bool,           // "!" или BREAKING CHANGE
    pub breaking_notes: Vec<String>,
}
```

`parse_commit(&RawCommit) -> Option<ConventionalCommit>` возвращает `None` для
merge-коммитов и сообщений вне формата. `summary()` собирает
`type(scope): description`; `is_release_commit()` отсекает `chore(release)`;
`is_merge_commit(message)` — отдельная проверка.

### `Bump` и `BumpPlan` ✅

`Bump::{None, Patch, Minor, Major}` — `Ord`; итог берётся как максимум по
коммитам. `decide_bump(commits, config)` применяет таблицу типов и
`0.x`-политику; `next_version(current, bump, prerelease)` считает следующую
версию (канал `alpha`/`beta`/`rc`, инкремент внутри канала, смена канала — с
`.1`); `plan(commits, config, current, prerelease, first_release)` возвращает
`BumpPlan { current, next: Option<Version>, bump }` — `next: None` при
отсутствии значимых коммитов.

```rust
let plan = flux_release::bump::plan(&commits, &config, &current, Some("beta"), false)?;
assert_eq!(plan.next.map(|v| v.to_string()), Some("0.2.0-beta.1".to_string()));
```

### `Preview`, `PackagePreview`, `Outcome` ✅

`preview(&Options)` — read-only: `current`, `next`, `bump`, `tag`, `commits`,
`changelog`, `already_released`, `branch`, `packages`.
`apply(&Options)` — запись: `previous`, `next`, `bump`, `tag`, `commits`,
`changelog_written`, `committed`, `tagged`, `published`, `skipped`, `dry_run`,
`packages`. Обе структуры сериализуются в JSON (`--json`).

`PackagePreview` — `name`, `manifest`, `current`, `next`; заполняется только
при `mode = "independent"`.

### `check_commits` и `CheckReport` ✅

`check_commits(&Options)` читает диапазон: `commits` (`from..to`), иначе
`from` или последний тег → `to` или `HEAD`. Merge/skip/`chore(release)`
пропускаются. Возвращает
`CheckReport { total, conventional, invalid, config_ok }`, где `invalid` —
первые строки нарушителей. `--strict` превращает непустой `invalid` в ошибку
с exit code 1.

### `init` и `doctor` ✅

`init(root, force, locale)` создаёт `.releaserc.toml` и `CHANGELOG.md`
(существующий конфиг без `force` — ошибка).
`doctor(root, config_path) -> DoctorReport` возвращает `git_repo`, `clean`,
`branch`, `config_ok`, `version`, `tags`, `cargo`, `gh`, `semver_checks` —
без изменения файлов.

### `Git` ✅

Обёртка над CLI: `new`, `root`, `is_repo`, `is_clean`, `current_branch`,
`tag_exists`, `last_tag(prefix)` (`--sort=-v:refname`), `log(from, to)`
(формат `%H%x1f%h%x1f%cs%x1f%B%x1e`), `head_date`, `head_short`,
`remote_url`, `repo_slug` (`owner/repo`), `add_all`,
`commit(message, no_verify)`, `create_tag` (annotated), `push(branch, tag)`.
Свободные `commit_url(remote, hash)` и `repo_slug(remote)` поддерживают
https и ssh remote.

### `manifest` ✅

- `read_version(path)` — `workspace.package.version`, иначе
  `[package].version`.
- `read_package_name(path)` — имя пакета или `None`.
- `write_version(path, version)` — точечная правка через `toml_edit`.
- `update_path_dependencies(path, versions) -> Result<bool>` — обновляет
  `version` у path-зависимостей (dependencies/dev/build/workspace) только при
  совпадении имени; возвращает `true`, если что-то изменено.
- `publish_order(manifests) -> Result<Vec<PathBuf>>` — топологический порядок
  публикации: path-зависимости раньше зависимых пакетов; цикл — типизированная
  ошибка. `--publish` использует этот порядок.

### `changelog` ✅

`header(locale)`, `section_title(section, locale)`,
`render_sections(commits, config, repo_url)`,
`render_entry(version, date, commits, config, repo_url)`,
`contains_version(existing, version)`,
`insert_entry(existing, entry, version, locale) -> Option<String>`.
`render_sections` группирует по типу коммита (`Features`, `Fixes`,
`Performance`, `Refactoring`, `Reverts`, `Documentation`, `Internal`,
`Breaking`), внутри — по модулю (`scope`, без scope — `General`/`Общее`).
Старые имена секций (`Added`/`Fixed`/`Changed`) нормализуются.
Вставка идёт перед первой строкой `## `; при уже присутствующей версии
возвращается `None` — повторный релиз не дублирует запись.
Если в конфиге задан `template`, `release` подставляет в него `{version}`,
`{date}`, `{repo_url}` и `{sections}`; без `{sections}` секции дописываются
в конец.

### `ReleaseError` ✅

`Config(String)`, `Git(String)`, `Manifest { path, message }`,
`Changelog(String)`, `Version(String)`, `Command(String)`, `Io`, `Json`.
Публичный API не паникует на ожидаемых отказах.

### Инварианты ✅

- `--dry-run` не пишет ни файлов, ни коммитов/тегов.
- Повторный `release` при существующем теге — no-op (`skipped = true`).
- Порядок секций и типов — детерминированный (`BTreeMap`), дата — из git.
- `cargo publish`, `gh`, `cargo-semver-checks` вызываются только по явному
  флагу; их отсутствие не ломает обычный релиз.
- `Cargo.lock` обновляется `cargo update --workspace` после записи версий и
  входит в release-коммит.

## Вне реализованного API (⏳)

Ниже перечислены крейты и возможности, которых в коде ещё нет; их API в этом
документе не описан.

| Крейт | Что появится позже |
|---|---|
| `flux-graphics` | DX12 backend (позже), окно/`VkSurfaceKHR` и present-проход (следующая итерация), GPU-проходы G-buffer/lighting и SSAO-GPU, запись GPU-зон из проходов, async compute/`Quality::Editor` (I3+). RHI-контракты, headless-null (I0), Vulkan device/память/offscreen swapchain (I1), Transfer/ResourceRegistry/VramBudget (I2), Render Graph (I3), Camera/RenderScene и extract (I4), SPIR-V PSO и CPU-эталоны PBR/G-buffer/tonemap/post (I5), runtime-ядро шейдерных пермутаций (I6), GraphicsSettings v1 и FeatureRegistry (I7), golden-image-харнесс и GPU-зоны (I8), DXC-путь, реальные шейдеры/PSO-кадр, query pools и G4-гейт (I6b) уже реализованы |
| `flux-asset` | UUID-хэндлы, async-загрузка, `.fpf`, hot-reload |
| `flux-scene` | сцены, prefab-ы, world partition, save/load сцены |
| `flux-anim` | скелетная анимация, blend-деревья |
| `flux-ai` | navmesh, pathfinding, BT/HFSM, crowd |
| `flux-script` | Lua 5.4, песочница |
| `flux-net` | репликация, relevance, транспорт |
| `flux-audio` | сигнальный граф, микшер |
| `flux-ui` | in-game UI, текстовый движок |
| `flux-editor` | редактор сцены, undo, PIE |
| `flux-physics` | broadphase/BVH, solver, CCD, joints (этап 6) |
| `flux-platform` | нативные crash-дампы per-OS (отдельный ADR), кнопки/оси геймпада в `InputState` |

## Журнал API

- **I0 (этап 3)**: контракты `flux-platform` (input/window/display/cursor/
  clipboard/text/paths/power/devices/crash), collision/KCC в `flux-physics`.
- **I1**: чорды (`Chord`, `ChordBinding`), property-контракты ввода,
  `Intention`-снапшот, CI-инвариант «raw не в симуляции».
- **I2**: `flux-engine::simulation` (`CharacterBody`, `PhysicsStep`,
  `controller_system`, `controller_post_system`), step-up в KCC.
- **I3**: `flux-engine::determinism` (`DeterminismHarness`, `StateHash`,
  `StateDiff`), CI-шаг `determinism-smoke`, headless `Engine::tick`.
- **I4**: `window::winit_adapter` (`WinitInputAdapter`, конверсии), `devices::
  gilrs_backend` (hot-plug), `flux-engine::overlay` (`OverlayData`), M3-пример
  на `winit` + `softbuffer`. Clipboard/text — headless, нативный crash — отложен.
- **I0 (этап 4)**: `flux-graphics::rhi` — контракты `RhiDevice`, `Swapchain`,
  `CommandBuffer`, `VramAllocator`; `BackendKind`, `TextureFormat`,
  `BufferUsage`/`TextureUsage`, дескрипторы ресурсов, `PipelineDesc`,
  `RenderPassDesc`, `Capabilities`, `RhiError`, `BarrierBatch`, `Pressure`;
  headless-null backend (`NullDevice`, `NullSwapchain`, `NullCommandBuffer`,
  `NullVramAllocator`) и `create_device`. Features `vulkan`/`dx12` выключены,
  реальные бэкенды — I1+.
- **I1 (этап 4)**: `flux-graphics::rhi::vulkan` за feature `vulkan` —
  `VulkanDevice` (instance/device/queues, память, offscreen `VulkanSwapchain`,
  `VulkanCommandBuffer`, validation messenger, leak-трекинг), `ash` с
  runtime-загрузкой (ADR-0034). `create_pipeline`/`draw` ждут SPIR-V ABI (I5),
  окно/`VkSurfaceKHR` — I4.
- **I2 (этап 4)**: `flux-graphics::render` — `TransferSystem` (staging-кольцо
  64 МиБ, батчи, fence-ретирмент), `ResourceRegistry` (Empty/Pending/Ready/
  Evicting, прокси, переиспользование bindless-слотов), `VramBudget`
  (резерв, pressure-лестница с гистерезисом 5 п.п.). Copy-команды — после
  расширения RHI (I3/I5).
- **I3 (этап 4)**: `flux-graphics::render::graph` — `RenderGraph`,
  `GraphImage`/`GraphPass`, `GraphError`, `CompiledPass`, `AliasSlot`,
  `RenderGraphPlan`; детерминированный топологический порядок, автоматические
  барьеры из read/write, aliasing transient-ресурсов, коалесцированный resize.
  `ImageState::Storage` добавлен для compute/graphics-записей. Async compute и
  `Quality::Editor` — заготовки.
- **I4 (этап 4)**: `flux-graphics::render::scene` — `Camera` (camera-relative
  view, Reversed-Z projection) и `RenderScene` (SoA, alpha интерполяции,
  переиспользование буферов); `flux-engine::render_extract` —
  `RenderTransform`, `MeshRenderer`, `RenderExtractor`, `ExtractStats`
  (детерминированный порядок по UUID). Окно/`VkSurfaceKHR` и present —
  следующая итерация.
- **I5 (этап 4)**: `flux-graphics::rhi` — `PipelineDesc` с SPIR-V
  (`graphics`/`compute`, entry points, `color_formats`, `depth_format`,
  `vertex_stride`, `topology`), `PrimitiveTopology`; Vulkan собирает реальные
  graphics/compute PSO (shader modules, layout без дескрипторов, dynamic
  viewport/scissor), `set_pipeline`/`draw`/`dispatch` валидируют порядок;
  `vertex_stride == 0` — pipeline без vertex input (`SV_VertexID`).
  `flux-graphics::render` — `pbr` (`Material`, GGX/Smith/Fresnel/multiscatter,
  `evaluate_direct`), `gbuffer` (ABI ADR-0017, `GbufferLayout`), `tonemap`
  (`Tonemap::Aces/Agx`), `post` (`bloom_threshold`/`bloom_combine`/
  `taa_blend`/`ssao_factor`). Реальные HLSL-шейдеры и PSO-кадр с draw
  добавлены в I6b; GPU-проходы G-buffer/lighting — следующая итерация.
- **I6 (этап 4)**: `flux-graphics::shaders` — `ShaderDeclaration`
  (`shader.json`: stages, pipeline-варианты, три набора keywords, vertex
  inputs layout v1, биндинги; бюджет ≤ 6 material-keywords), `KeywordMask`,
  `PermutationRequest` (FNV-1a ключ), `ShaderCompiler`,
  `DeterministicCompiler` (тестовая заглушка; реальный путь — DXC),
  `PermutationCache`
  (байткод/PSO/материал) и `ShaderLibrary` (ядро fallback-ов, off-demand
  `resolve`, `CacheStats`).
- **I6b (этап 4)**: `tools/flux-assetc shader` — HLSL → SPIR-V через DXC
  (`DXC_PATH` → рядом с бинарником → `tools/dxc/bin` → `PATH`, ADR-0038);
  `shaders/fullscreen.vert.hlsl`, `shaders/solid.frag.hlsl` и
  `shaders/solid.shader.json`, скомпилированные `shaders/compiled/*.spv`;
  `tests/vulkan_pso.rs` — реальный graphics PSO, `draw(3, 1)` в
  `Rgba8Srgb`-цель без validation-ошибок (на GPU-машине); query pools RHI
  (ADR-0037), `GpuProfiler::push_from_timestamps`, G4-гейт
  `tests/golden_gate.rs`.
- **I7 (этап 4)**: `flux-graphics::settings` — `HardwareTier` (вывод из
  `Capabilities`), `QualityPreset` (тени/render scale/SSAO/bloom/TAA),
  `GraphicsSettings` (`from_preset`/`from_tier`, `sanitize`, `diff`, `apply`,
  сериализация), `SettingsChange` (swapchain/graph/pipelines, Device не
  пересоздаётся) и `GraphicsFeatureRegistry` (фиксированный порядок фич,
  учёт capabilities). Настройки персистентны (JSON/RON).
- **I8 (этап 4)**: `flux-graphics::golden` — `Rgba8Image`, `psnr`/`ssim`,
  `diff_heatmap`, `GoldenSettings`, `GoldenBaseline`, `GoldenStore`
  (per-runner бейзлайны, `visual_change`-гейт, сериализация),
  `GoldenVerdict`; `flux-graphics::profile` — `GpuZone`, `GpuProfiler`
  (детерминированные зоны, вытеснение старейших).
- **I6b (этап 4)**: `tools/flux-assetc shader` — офлайн HLSL → SPIR-V через
  DXC (`DXC_PATH`/`PATH`, типизированные ошибки, `--check`); RHI query
  pools — `QueryPoolDesc`/`QueryPoolId`, `create_query_pool`/
  `destroy_query_pool`/`read_query_pool`, `reset_query_pool`/
  `write_timestamp` (Vulkan timestamp queries, Null — детерминированные
  значения); `GpuProfiler::push_from_timestamps`; G4-гейт
  `crates/flux-graphics/tests/golden_gate.rs` с per-runner baseline и шагом CI.
- **I9 (этап 4, ADR-0039)**: RHI для GPU-проходов — `BindingKind`,
  `DescriptorBinding` (bindless `count`), `DescriptorSetDesc`,
  `DescriptorSetId`, `DescriptorWrite`/`DescriptorResource`;
  `RhiDevice::{create,update,destroy}_descriptor_set`, `write_buffer`,
  `read_buffer`; `PipelineDesc::with_bindings`; `CommandBuffer::
  bind_descriptor_set`, `copy_buffer_to_image`, `copy_image_to_buffer`.
  Vulkan: descriptor pool (`FREE_DESCRIPTOR_SET`), кэш layout-ов, копии и
  map-readback; Null — валидация layout-ов и детерминированный readback.
  `tests/vulkan_pso.rs`: roundtrip буфера, descriptor set и clear-кадра через
  staging-буфер.
- **Инструменты (ADR-0040)**: `tools/flux-release` — CLI `release`/`version`/
  `changelog`/`check`/`init`/`doctor`; `Options`, `ReleaseConfig`
  (fixed/independent, en/ru, `commit_types`, skip-маркеры, внешний шаблон),
  `ConventionalCommit`, `BumpPlan` (0.x-политика, pre-release каналы),
  `Preview`/`Outcome`, `CheckReport`, `DoctorReport`, `Git` (`repo_slug`),
  `manifest` (`publish_order`), `changelog` (`render_sections`); диапазон
  `--commits from..to`, идемпотентность, `--dry-run`/`--json`, opt-in
  `--package`/`--publish`/`--github-release` (gh или GitHub API)/`--semver-checks`;
  интеграционные тесты на временном git-репозитории.
