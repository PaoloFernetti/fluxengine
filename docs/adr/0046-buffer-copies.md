# 0046. Копирование buffer→buffer: загрузка storage-буферов геометрии

- Статус: Accepted
- Дата: 2026-11-15
- Затрагивает: flux-graphics
- Supersedes: —

## Контекст

ADR-0039 закрепил staging-контракт: `write_buffer`/`read_buffer` работают
только с буферами `BufferUsage::is_staging()` (transfer-флаги без
GPU-использования), а копирование было только между буфером и изображением.

GPU-геометрия (пункт 4.5) требует device-local **storage**-буферов, которые
читает vertex pulling (`SV_VertexID`/`SV_InstanceID`): вершинные и
инстансные данные нельзя писать host-ом напрямую, а команды buffer→buffer в
RHI не было. Без неё загрузка геометрии либо ломает staging-контракт, либо
требует новой подсистемы загрузки ради одной команды.

## Варианты

1. **Разрешить host-visible storage** (`BufferUsage { storage, transfer_src,
   transfer_dst }` + `write_buffer`). Ломает правило «staging — единственный
   host-visible буфер» (ADR-0039), на дискретных GPU даёт медленный upload и
   скрывает синхронизацию.
2. **Отдельный `UploadBuffer`/transfer-API.** Новая подсистема ради одной
   команды; дублирует `CommandBuffer`.
3. **`CommandBuffer::copy_buffer_to_buffer`** (выбрано) — симметрично
   существующим `copy_buffer_to_image`/`copy_image_to_buffer`.

## Решение

`CommandBuffer` расширен:

```rust
fn copy_buffer_to_buffer(&mut self, src: BufferId, dst: BufferId, size: u64)
    -> Result<(), RhiError>;
```

- `src` обязан иметь `BufferUsage::transfer_src`, `dst` — `transfer_dst`;
  `size` не нулевой и не больше обоих буферов; вызов допустим вне render pass.
- Vulkan: `vkCmdCopyBuffer` + `MemoryBarrier` `TRANSFER_WRITE →
  SHADER_READ | VERTEX_ATTRIBUTE_READ` (`TRANSFER → VERTEX|FRAGMENT`), чтобы
  следующий submit видел данные в vertex pulling.
- Null: переносит байты в `NullMemory` (содержимое приёмника создаётся по
  известному размеру буфера), валидирует usage и границы.
- Барьеры изображений по-прежнему выставляет Render Graph (ADR-0039,
  ADR-0042); buffer-копия — самостоятельная команда upload-этапа.

## Следствия

- Путь загрузки: `create_buffer(staging)` → `write_buffer` →
  `copy_buffer_to_buffer` → `finish` (submit с fence-wait), затем кадр.
- Host-visible остаётся только у staging-буферов; storage-буферы
  device-local, как и требуют дискретные GPU.
- Покрытие: Null — `buffer_copy_uploads_storage_and_reads_back` (upload →
  обратная копия → readback), Vulkan — тот же срез в `tests/vulkan_pso.rs`
  с чистым validation-логом.
- `SceneGeometry` (4.5b) использует этот путь для вершин и инстансов;
  `BufferUsage` не меняется, ADR-0039 остаётся в силе.
