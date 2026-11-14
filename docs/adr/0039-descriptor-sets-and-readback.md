# 0039. Дескрипторные наборы, копирование и readback в RHI

- Статус: Accepted
- Дата: 2026-11-14
- Затрагивает: flux-graphics, flux-engine
- Supersedes: —

## Контекст

PSO из SPIR-V (ADR-0035) и query pools (ADR-0037) закрывают сборку pipeline
и профилирование, но GPU-проходы G-buffer → lighting → tonemap → post
упираются в две дыры RHI:

1. нет способа привязать к шейдеру uniform/storage-буферы и изображения —
   layout pipeline всегда пустой;
2. нет копирования buffer↔image и чтения результата на CPU, поэтому
   golden-кадры остаются синтетическими, а не реальными.

Нужен минимальный контракт, не привязывающий Render Graph к Vulkan/DX12 и
не требующий bindless-расширений на первом шаге, но допускающий их позже.

## Варианты

1. **Один descriptor set 0 с массивом биндингов** (выбрано). Просто, ложится
   на HLSL `register(tN/uN/bN)`, `count > 1` сразу даёт bindless-массивы.
2. **Набор на материал/проход.** Больше объектов и переключений; для первого
   GPU-прохода выгоды нет.
3. **Push constants вместо uniform.** Ограничены 128 байтами и не покрывают
   текстуры/storage.
4. **Полная root signature/DX12-модель.** Преждевременно: DX12-бэкенд ещё не
   реализован.

## Решение

- `BindingKind` — `UniformBuffer`, `StorageBuffer`, `SampledImage`,
  `StorageImage`; `DescriptorBinding { binding, kind, count }`, где `count` —
  размер массива (bindless), `1` по умолчанию.
- `DescriptorSetDesc { label, bindings }`; `DescriptorSetId` —
  детерминированный хэндл.
- `RhiDevice`: `create_descriptor_set`, `update_descriptor_set`,
  `destroy_descriptor_set`.
- `DescriptorWrite { binding, array_index, resource }`, где `resource` —
  `Buffer(BufferId)` или `Image(ImageId)`. Тип ресурса обязан совпадать с
  видом биндинга, индекс — попадать в `count`; неизвестный биндинг/ресурс —
  `RhiError::Validation`.
- `PipelineDesc::with_bindings(&'static [DescriptorBinding])` описывает set 0;
  `CommandBuffer::bind_descriptor_set` вызывается после `set_pipeline`.
- **Конвенция регистров HLSL:** `binding N` ↔ `bN` (uniform),
  `uN` (storage buffer), `tN` (sampled/storage image); всё в `set 0`.
  Таблица обязательна для всех шейдеров — иначе layout и HLSL расходятся.
- `CommandBuffer::copy_buffer_to_image` / `copy_image_to_buffer` копируют mip 0,
  слой 0, весь размер; копии запрещены внутри render pass (Vulkan-правило).
  Барьеры по-прежнему выставляет Render Graph.
- **Владение копиями:** копии выполняются только как узлы Render Graph
  (copy-узел). Прямые вызовы на `CommandBuffer` допустимы лишь внутри
  реализации такого узла — это единственное правило, которое одновременно
  закрывает upload, readback и барьеры.
- **Staging-контракт:** `write_buffer`/`read_buffer` работают только с
  host-visible staging-буферами (`transfer_src && transfer_dst` без
  GPU-использования); `copy_buffer_to_image` требует staging-источник,
  `copy_image_to_buffer` — staging-приёмник. Оба условия валидируются в
  Vulkan и Null; диапазон проверяется по размеру буфера.
- **Дисциплина кадров в полёте:** дескрипторные наборы и staging-буферы
  живут в кольце на `frames_in_flight`; набор/буфер не переиспользуется,
  пока кадр, ссылающийся на него, не завершён. `update_descriptor_set`
  разрешён только для набора вне полёта (граница acquire) — обновление
  сета, на который ссылается засабмиченный командный буфер, запрещено.
- Vulkan: descriptor pool с `FREE_DESCRIPTOR_SET` (256 наборов), кэш layout-ов
  по составу биндингов, `SAMPLED_IMAGE`/`STORAGE_IMAGE`, копии через
  `cmd_copy_*`, readback через `map_memory`.
- Null: layout и записи хранятся, копии считаются, содержимое staging-буфера
  живёт в памяти устройства; валидация идентична Vulkan.

### Последовательность зависимостей

`SAMPLED_IMAGE` без sampler-объекта в Vulkan не семплируется: биндинг
существует, но чтение текстуры невозможно. Поэтому до отдельного ADR по
сэмплерам реализуемы только проходы **без чтения текстур**:

1. **Сейчас:** G-buffer-запись (storage image / render target) → staging →
   readback; copy-узел графа.
2. **После sampler-ADR:** lighting, post (bloom/TAA/SSAO) и любые проходы,
   читающие изображения.

Это не «временное решение»: комбинированный sampler вручную, минуя контракт,
запрещён — он ломает ABI и тесты Null/Vulkan.

### Golden-политика

- Null-бэкенд — bit-exact (детерминированная CPU-эмуляция), допуск не нужен.
- GPU-бэкенды — сравнение на референс-устройстве с допуском: per-pixel diff
  и доля выбросов; bit-exact между драйверами/вендорами не гарантируется.
- G4 на GPU использует порог, а не побайтовое равенство; «разница в 2 LSB»
  не является автоматическим фейлом.

## Следствия

Плюсы:

- GPU-проходы получают единый контракт: буферы/изображения в set 0, без
  API-специфики в Render Graph.
- `count > 1` сразу поддерживает bindless-массивы — расширение без смены ABI.
- Реальный readback делает возможными golden-кадры из GPU вместо формул.
- Null-бэкенд сохраняет детерминизм и покрывает валидацию без GPU.

Минусы и риски:

- До sampler-ADR реализуемы только проходы без чтения текстур (см.
  «Последовательность зависимостей»); lighting/post ждут отдельного ADR.
- Нет push constants, indirect draw и storage-image barriers-хелперов.
- Один set 0 ограничивает частые переключения материалов; при необходимости
  добавляется set 1 новым ADR.
- Кольцо кадров в полёте требует явного владения наборами/staging-буферами:
  ошибка дисциплины даёт UB, поэтому валидация и тесты обязательны.

Заделы:

- Первый вертикальный срез: copy-узел + G-buffer-запись в storage image +
  readback через staging; Null-бэкенд — референс.
- Sampler/combined image, push constants, indirect — отдельные ADR.
- Golden-кадры G4 переводятся на реальный readback с GPU-допуском.
