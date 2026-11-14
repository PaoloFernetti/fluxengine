# 0034. Vulkan-бэкенд на `ash`: runtime-загрузка, feature-гейт, отложенные surface и PSO

- Статус: Accepted
- Дата: 2026-11-13
- Затрагивает: `flux-graphics`, `flux-engine`, CI (headless-правило)
- Связано: ADR-0001 (свой RHI, не wgpu), ADR-0005 (один крейт + features),
  ADR-0008 (Reversed-Z), ADR-0017 (G-buffer/PBR), ADR-0033 (headless)

## Контекст

Итерация I0 закрыла RHI-контракты и headless-null backend, но реального GPU
ещё нет. I1 должен дать первый Vulkan-кадр (clear-color) и validation-гейт.
Требования, которые формируют решение:

- Сборка на CI-раннере **без Vulkan SDK** и без GPU обязана проходить:
  `Null` остаётся дефолтом, `vulkan` — feature-гейт.
- `ash` разрешён headless-правилом только в `flux-graphics`
  (`tools/ci/check-headless-deps.mjs`).
- RHI не должен протекать в Render Graph и наоборот (ADR-0001).
- В workspace включён `unsafe_code = "warn"`, а CI запускает clippy с
  `-D warnings`: `unsafe` нужно изолировать и явно разрешить в одном месте.
- `PipelineDesc` пока не содержит SPIR-V и layout-описаний: контракт I0
  стабилен, ломать его ради «настоящего PSO» в I1 нельзя.

## Варианты

1. **`wgpu`** — быстрый старт, но это второй RHI поверх нашего: нарушает
   ADR-0001 (Render Graph и барьеры должны быть своими), плюс скрывает
   Vulkan-специфику, нужную для bindless и timeline semaphores.
2. **`vulkano`** — безопасная объектная модель, но тяжёлый слой владения,
   навязывает свои аллокаторы и затрудняет точный leak-трекинг.
3. **`erupt`** — тонкие биндинги, но меньше документации/примеров и более
   агрессивная генерация; выше риск ошибок в `unsafe`.
4. **`ash` (linked)** — привычный API, но линкует `vulkan-1` на этапе
   сборки: CI без Vulkan SDK не соберётся.
5. **`ash` (loaded) + `debug`** — те же биндинги, Vulkan loader грузится в
   рантайме; сборка не требует SDK, отсутствие GPU обрабатывается
   типизированной ошибкой.

## Решение

Берём **`ash 0.38`** с `default-features = false`, features `loaded` и
`debug`; зависимость опциональна и включается feature `vulkan`
(`vulkan = ["dep:ash"]`).

- `rhi::vulkan` содержит `VulkanDevice`, `VulkanSwapchain`,
  `VulkanCommandBuffer`, `MemoryState` и validation messenger.
- `unsafe` изолирован: модули `rhi::vulkan::*` начинаются с
  `#![allow(unsafe_code)]`; публичный RHI-контракт остаётся безопасным.
- Instance создаётся с API 1.2, если loader её поддерживает, иначе 1.0.
  Validation layer `VK_LAYER_KHRONOS_validation` включается в debug-сборке;
  если слой/расширение недоступны, instance создаётся без них, а не падает.
- Память: одна `VkDeviceMemory` на ресурс (буфер/изображение), выбор типа
  по требованиям; бюджет VRAM считается по device-local кучам, лизинги
  `VramAllocator` учитываются вместе с реальными аллокациями.
- Swapchain в I1 — **offscreen** (`VulkanSwapchain`): изображения создаются
  устройством, `acquire`/`present` детерминированы, `resize` пересоздаёт
  изображения. `VkSwapchainKHR`/`VkSurfaceKHR` требуют оконного handle и
  появятся в I3 вместе с present-проходом Render Graph; тогда же
  рассматривается `raw-window-handle` (при необходимости — отдельный ADR).
- `create_pipeline` в I1 возвращает `RhiError::Unsupported`: настоящий
  `VkPipeline` требует SPIR-V и layout-описаний. `PipelineDesc` расширится
  в I5 (шейдерный ABI), контракт I0 при этом не ломается.
- DX12 остаётся за feature `dx12`; зависимость `windows` принимается
  отдельным решением, когда начнётся I2+ для DX12.

## Следствия

- **Плюсы**: CI без Vulkan SDK собирает workspace; на машине с GPU/lavapipe
  I1-тесты реально исполняются, а без loader-а — корректно пропускаются;
  `Null` остаётся дефолтом и не зависит от `ash`; leak-трекинг и validation
  проверяются на реальном драйвере.
- **Минусы/риски**: `unsafe`-кода становится много, он локализован, но
  требует дисциплины; offscreen swapchain не даёт окна до I3; PSO и draw
  недоступны до I5, поэтому I1-гейт — clear-color и барьеры, а не геометрия.
- **Что ломается**: ничего в публичном API I0; `create_device(Vulkan)` меняет
  поведение с «всегда Unsupported» на «устройство или типизированная ошибка».
- **Заделы**: timeline semaphores включаются при наличии Vulkan 1.2;
  descriptor indexing только запрашивается (bindless — I2); кэш
  render pass/framebuffer уже строится по форматам и размерам и переживёт
  переход к Render Graph.
