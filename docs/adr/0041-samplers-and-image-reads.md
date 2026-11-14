# 0041. Сэмплеры и чтение изображений в RHI

- Статус: Accepted
- Дата: 2026-11-14
- Затрагивает: flux-graphics, flux-engine, tools/flux-assetc
- Supersedes: —

## Контекст

ADR-0039 ввёл дескрипторные наборы, копирование buffer↔image и staging-readback,
но чтение текстур осталось невозможным: `SampledImage` привязывает только
`VkImageView`, а без sampler-объекта Vulkan не семплирует. DXC при компиляции
HLSL в SPIR-V (ADR-0035) объявляет `Texture2D tN` и `SamplerState sN` как
**отдельные** `UniformConstant`-переменные — «combined» в SPIR-V сам по себе не
появляется.

Поэтому GPU-проходы lighting, bloom, TAA, SSAO и любые чтения G-buffer
заблокированы (Roadmap 4.6, [`строка 604`](../Roadmap.md)). ADR-0039 прямо
фиксирует: до отдельного ADR по сэмплерам реализуемы только проходы без чтения
текстур.

## Варианты

1. **Только `COMBINED_IMAGE_SAMPLER`.** Один биндинг на текстуру и сэмплер.
   Компактно, но расходится с выходом DXC (нужны перепаковка layout-ов или
   рукописный SPIR-V) и с моделью DX12 (SRV + sampler heap).
2. **Раздельные `SampledImage` + `Sampler`** (выбрано). Совпадает с DXC-SPIR-V
   и с DX12, `count > 1` даёт массивы, bindless добавляется без смены контракта.
3. **Immutable/static samplers в pipeline layout.** Быстрее, но фиксирует
   фильтрацию в PSO и мешает рантайм-настройкам качества (ADR-0016).
4. **Bindless-only.** Преждевременно: требует descriptor indexing в DX12-модели
   и усложняет Null-валидацию.

## Решение

- `SamplerDesc { label, mag_filter, min_filter, mipmap_mode, address_u,
  address_v, address_w, max_anisotropy, lod_min, lod_max, compare, border }`.
  Перечисления: `FilterMode::{Nearest, Linear}`, `MipmapMode::{Nearest, Linear}`,
  `AddressMode::{ClampToEdge, Repeat, MirrorRepeat, ClampToBorder}`.
- `SamplerId` — детерминированный хэндл (как остальные RHI-хэндлы);
  `RhiDevice::create_sampler` / `destroy_sampler`.
- `BindingKind::Sampler` — VK `SAMPLER`, HLSL `sN`.
  `BindingKind::CombinedImageSampler` — **зарезервирован, в v1 не реализуется**:
  DXC выдаёт раздельные переменные, а combined требует пары image+sampler в
  одной записи, которой нет в `DescriptorWrite`; вводится при необходимости
  рукописного SPIR-V (см. «Уточнение при реализации»).
- `DescriptorResource::Sampler(SamplerId)` и
  `DescriptorWrite::sampler(binding, array_index, sampler)`.
- **Валидация (Null и Vulkan одинаково):** `Sampler` ↔ `Sampler`;
  `SampledImage` ↔ `Image` с `TextureUsage::sampled()`;
  `max_anisotropy > 1` требует `Capabilities::supports_sampler_anisotropy`;
  нарушение — `RhiError::Validation`.
- `ImageState::ShaderRead` — единственное состояние чтения. Render Graph обязан
  вставить барьер `→ ShaderRead` перед read-проходом. Vulkan: layout
  `SHADER_READ_ONLY_OPTIMAL`, stage `FRAGMENT_SHADER`, access `SHADER_READ`.
  Null: учёт состояния и ошибка при чтении в другом состоянии.
- **ABI регистров (расширение ADR-0035):** `bN` — uniform, `uN` — storage
  buffer, `tN` — sampled/combined image, `sN` — sampler; всё в `set 0`.
  assetc фиксирует флаги DXC (`-spirv`, `-fvk-use-dx-layout`) и не использует
  `-fvk-bind-sampler-heap`, иначе layout разъедется с контрактом.
- **Сэмплеры иммутабельны:** `update_sampler` не существует; смена фильтрации —
  это новый `SamplerId`. Это снимает для сэмплеров вопрос обновления набора
  в полёте (ADR-0039).
- Vulkan: `vkCreateSampler` на каждый `create_sampler`,
  `update_descriptor_set` пишет `vk::DescriptorImageInfo { sampler }` для
  `Sampler` и `{ image_view, image_layout: SHADER_READ_ONLY_OPTIMAL }` для
  `SampledImage`.
- Null: хранит `SamplerDesc`, валидирует биндинги и состояния; семплирование
  не эмулирует — read-проходы проверяются через storage-путь и копирование.
- DX12 (этап 6): SRV + sampler heap/static samplers; контракт не меняется,
  `CombinedImageSampler` маппится в SRV + static sampler.

### Последовательность

1. RHI-типы и Null (тесты без GPU).
2. Vulkan: кэш сэмплеров и `DescriptorImageInfo`.
3. assetc: правило регистров и проверка layout-ов шейдеров.
4. Render Graph: барьер `→ ShaderRead` и read-узлы (lighting, post).
5. Golden: GPU против CPU-эталона, PSNR ≥ 60 дБ (порог G4 в Roadmap).

## Последствия

- Разблокированы 4.6 (lighting/post) и каскадом 4.5/4.8/4.9 (FrameStats,
  рантайм-`SettingsChange`, запись зон).
- +1 вид биндинга, +1 хэндл, +1 кэш. Дублирующий `CombinedImageSampler` требует
  дисциплины: смешивать пути в одном наборе запрещено.
- Шейдеры обязаны следовать таблице регистров; нарушение ловится в assetc,
  а не «тихо неверным кадром».
- Анизотропия и `ClampToBorder` недоступны на слабых устройствах — это
  отражается в `Capabilities`, а не в коде прохода.

### Уточнение при реализации (D3)

- `CombinedImageSampler` оставлен зарезервированным: combined без пары
  image+sampler в `DescriptorWrite` означал бы скрытый дефолтный сэмплер, то
  есть неявное поведение вместо явного контракта.
- Кэш сэмплеров по `SamplerDesc` не вводится в v1: в дескрипторе есть `f32`
  LOD, поэтому он не годится в ключ `BTreeMap` без нормализации, а создание
  сэмплера — редкая операция. Кэш — оптимизация уровня D8.
- `Capabilities::supports_sampler_anisotropy` заполняется из
  `VkPhysicalDeviceFeatures::sampler_anisotropy`; Null сообщает `true`.

## Тесты

- `sampler_desc_validates_ranges` — границы полей `SamplerDesc` (Null).
- `descriptor_write_rejects_wrong_resource` — `Sampler` с `Image` и наоборот.
- `sampled_image_requires_sampled_usage` — изображение без `TextureUsage::sampled()`.
- Vulkan (feature `vulkan`, при наличии устройства): read-проход семплирует
  текстуру, validation layers чистые.
- `read_pass_after_copy_readback` — пиксель G-buffer, прочитанный обратно,
  совпадает на Null и Vulkan.

## Ссылки

- ADR-0016 (GraphicsSettings), ADR-0017 (G-buffer ABI), ADR-0018 (tonemap),
  ADR-0035 (shader ABI), ADR-0039 (дескрипторы и readback).
- Roadmap: 4.6 (PBR и G-buffer), 4.9 (golden и профилирование).
