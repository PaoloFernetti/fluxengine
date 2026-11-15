# flux-graphics

RHI-контракты, Vulkan/DX12, Render Graph, PBR.

Канон: [docs/canon.md](../../docs/canon.md). План: [docs/architecture.md](../../docs/architecture.md).
Roadmap: [docs/Roadmap.md](../../docs/Roadmap.md). API: [docs/api.md](../../docs/api.md#flux-graphics).

## Структура

Один крейт с модулями (ADR-0005):

| Модуль | Назначение | Статус |
|---|---|---|
| `rhi` | контракты `RhiDevice`/`Swapchain`/`CommandBuffer`/`VramAllocator`, типы, capabilities, ошибки | I0 ✅ |
| `rhi::null` | headless-null backend для тестов и детерминизма | I0 ✅ |
| `rhi::vulkan` | Vulkan: instance/device/queues, память, offscreen swapchain, validation, SPIR-V PSO | I1/I5 ✅ |
| `rhi::dx12` | D3D12 backend на `windows` | I2+ ⏳ |
| `render` | Transfer, ResourceRegistry, VramBudget | I2 ✅ |
| `render::graph` | Render Graph, автоматические барьеры, aliasing, resize | I3 ✅ |
| `render::scene` | `Camera` (camera-relative, Reversed-Z), `RenderScene` (SoA, alpha) | I4 ✅ |
| `render::pbr` | `Material`, GGX/Smith/Fresnel/multiscatter, `evaluate_direct` | I5 ✅ (CPU-эталон) |
| `render::gbuffer` | ABI ADR-0017: форматы RT0–RT2, depth, `GbufferLayout` | I5 ✅ |
| `render::tonemap` | ACES/AgX (ADR-0018) | I5 ✅ (CPU-эталон) |
| `render::post` | bloom/TAA/SSAO — формулы | I5 ✅ (CPU-эталон) |
| `render::frame` | GPU-проходы кадра, CPU-эталоны и PSNR-гейт | I9c ✅ (ADR-0042) |
| `shaders` | `shader.json`, keyword-маски, fallback, кэш 3 уровней | I6 ✅ (DXC — I6b) |
| `settings` | `GraphicsSettings` v1, `HardwareTier`/`QualityPreset`, FeatureRegistry | I7 ✅ |
| `golden` | PSNR/SSIM, per-runner baselines, heatmap, `visual_change`-гейт | I8 ✅ |
| `profile` | `GpuZone`, `GpuProfiler` — контракт для `FrameStats`/trace | I8 ✅ |
| `pipeline_cache` | дисковый кэш PSO: заголовок с подписью устройства, инвалидация | I5 ✅ |

## Feature-гейты

По умолчанию не включён ни один нативный бэкенд: `Null` доступен всегда, что
позволяет собирать и тестировать крейт без GPU и без Vulkan SDK.

| Feature | Что включает |
|---|---|
| `vulkan` | Vulkan backend: `ash` 0.38 с runtime-загрузкой (`loaded` + `debug`) |
| `dx12` | D3D12 backend (`windows`) — I2+ |

```toml
flux-graphics = { workspace = true, features = ["vulkan"] }
```

## Контракт RHI (I0)

- RHI не знает про Render Graph; Render Graph не знает про конкретный API.
- Ошибки типизированы (`RhiError`), ожидаемые отказы не паникуют.
- `Capabilities` описывают форматы, present-режимы и лимиты, на которые
  опирается планировщик Render Graph.
- `VramAllocator` — единый владелец VRAM; давление (`Pressure`) с порогами
  0.70 / 0.85 / 0.95.

## Vulkan (I1/I5, ADR-0034, ADR-0035)

- `create_device(BackendKind::Vulkan)` создаёт `VulkanDevice`; без feature,
  без loader-а или без graphics-очереди возвращается `Unsupported`.
- Validation layers включаются в debug-сборке, если слой доступен;
  `VulkanDevice::validation_errors()` — счётчик для гейта «чистый лог».
- Память: `VkDeviceMemory` на ресурс, бюджет по device-local кучам,
  лизинги `VramAllocator` учитываются вместе с аллокациями.
- Поверхности: `SurfaceTarget::{Headless, Window}` (`raw-window-handle` 0.6,
  win32/xlib/xcb/wayland), `create_surface`/`destroy_surface`. Headless-WSI
  (`VK_EXT_headless_surface`) даёт реальный `VkSwapchainKHR` в CI на lavapipe.
- Swapchain: offscreen (`VulkanSwapchain`) и present
  (`VulkanPresentSwapchain` — acquire через fence, переход образа в
  `PRESENT_SRC_KHR`, `queue_present`, resize с ожиданием idle). `SwapchainDesc::with_surface`
  привязывает цепочку к поверхности.
- `create_pipeline` (I5) собирает реальные graphics/compute PSO из SPIR-V:
  shader modules, `PipelineLayout` без дескрипторов, vertex input
  `R32G32B32_SFLOAT` (stride 12), dynamic viewport/scissor, blend disable.
  `with_vertex_stride(0)` — pipeline без vertex buffer (`SV_VertexID`).
  Пустые/битые шейдеры и неверный `kind` — `Validation` (ADR-0035).
- `set_pipeline` биндит graphics/compute pipeline; `draw` требует graphics и
  активный render pass, `dispatch` — compute вне render pass.
- Кэш PSO (I5): `VkPipelineCache` создаётся вместе с устройством и
  используется всеми `create_pipeline`; `RhiDevice::pipeline_cache()` отдаёт
  блоб, `load_pipeline_cache()` заменяет кэш данными с диска.
  `PipelineCacheFile` хранит `FLUXPSO1`-заголовок с `DeviceSignature` и
  атомарно пишет файл; при смене GPU/API/формата кэш инвалидируется.
- Query pools (ADR-0037): timestamp-пулы, `reset_query_pool`/
  `write_timestamp`, `read_query_pool` с `TYPE_64 | WAIT`.
- Дескрипторы (ADR-0039): set 0 через `PipelineDesc::with_bindings`
  (`UniformBuffer`/`StorageBuffer`/`SampledImage`/`StorageImage`, bindless
  `count`); `create/update/destroy_descriptor_set`, `bind_descriptor_set`.
  Сэмплеры (ADR-0041): `SamplerDesc`/`SamplerId`, `BindingKind::Sampler`,
  `create_sampler`/`destroy_sampler`, `supports_sampler_anisotropy`.
- Push constants (ADR-0042): `PipelineDesc::with_push_constants(size)`,
  `CommandBuffer::push_constants(&[u8])`; Vulkan создаёт `VkPushConstantRange`
  (`VERTEX | FRAGMENT` / `COMPUTE`) и вызывает `cmd_push_constants`, Null
  валидирует размер/выравнивание и отдаёт `push_constants_bytes()`.
- Копирование и readback: `copy_buffer_to_image`/`copy_image_to_buffer`
  (вне render pass), `write_buffer`/`read_buffer` для host-visible staging.
  Null-бэкенд эмулирует содержимое (WP D4): clear color/depth в
  `begin_render_pass`, перенос байтов в копиях, поэтому срез
  «clear → copy → readback» проверяется без GPU.
- Реальные шейдеры: `shaders/fullscreen.vert.hlsl` (`SV_VertexID`) и
  `shaders/solid.frag.hlsl` → `shaders/compiled/*.spv` (DXC,
  `tools/flux-assetc shader`). `tests/vulkan_pso.rs` собирает PSO, рисует
  треугольник в `Rgba8Srgb`-цель, проверяет descriptor set и clear-кадр
  через staging-readback; лог validation остаётся чистым.
- GPU-проходы кадра (ADR-0042): `render::frame::GpuFrame` —
  `frame.gbuffer` (RT0/RT1/RT2 + depth) → `frame.ssao` → `frame.lighting`
  (GGX/Smith/Fresnel/multiscatter) → `frame.bloom` → `frame.tonemap` (ACES)
  → `frame.taa` (ping-pong истории). Шейдеры — `shaders/frame_*.frag.hlsl`
  → `shaders/compiled/frame_*.frag.spv`; параметры кадра идут push
  constants (64 B). Render Graph пишет тела пассов через `execute_with`,
  `oldLayout` истории берётся из `compile_with_initial`. `begin_render_pass`
  выставляет dynamic viewport/scissor по extent фреймбуфера.
- PSNR-гейт (ADR-0042): CPU-эталон каждой стадии получает readback
  предыдущего прохода; 8-битные цели сравниваются байтами, fp16 —
  декодированными значениями. `tests/vulkan_frame.rs`: все стадии ≥ 60 дБ,
  второй кадр TAA с историей, чистый validation, `live_resources() == 0`.
- FrameStats по пассам (ADR-0037 + ADR-0042): `GpuFrame` держит
  timestamp-пул (`FRAME_QUERY_COUNT = 12`), пишет пары вокруг каждого из
  шести проходов и отдаёт `zones()`, `pass_duration_ns()`,
  `frame_duration_ns()`; порядок имён — `FRAME_PASS_NAMES`. Null-тест
  проверяет состав и порядок зон без GPU.

## Проверки

```text
cargo test -p flux-graphics
cargo test -p flux-graphics --features vulkan
cargo test -p flux-graphics --features vulkan --test vulkan_pso
cargo test -p flux-graphics --features vulkan --test vulkan_surface
cargo test -p flux-graphics --features vulkan --test vulkan_frame
cargo test -p flux-graphics --test golden_gate
cargo clippy -p flux-graphics --features vulkan --all-targets -- -D warnings
```

`vulkan_pso`, `vulkan_surface` и `vulkan_frame` без доступного
Vulkan-устройства завершаются успешно (headless-CI), поэтому их можно
запускать на любом раннере; на lavapipe CI включает
`FLUX_REQUIRE_VULKAN=1` и падает вместо skip-а.

Пересборка SPIR-V шейдеров кадра:

```text
cargo build -p flux-assetc
target/debug/flux-assetc shader crates/flux-graphics/shaders/frame_gbuffer.frag.hlsl \
  --out crates/flux-graphics/shaders/compiled/frame_gbuffer.frag.spv \
  --entry fs_main --stage ps_6_0
```
