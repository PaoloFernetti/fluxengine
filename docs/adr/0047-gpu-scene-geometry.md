# 0047. GPU-геометрия сцены: vertex pulling из storage-буферов

- Статус: Accepted
- Дата: 2026-11-15
- Затрагивает: flux-graphics
- Supersedes: —

## Контекст

G-buffer-проход кадра (ADR-0042) рисует процедурную поверхность
полноэкранным треугольником: реальной геометрии сцены в GPU-пайплайне нет.
Пункт 4.5 требует «vertex pulling `SV_VertexID` → G-buffer → deferred
lighting → UI-stub → present».

`RenderScene` — SoA-снапшот (позиции, mesh/material id), а не vertex buffer.
Классический путь потребовал бы связать с каждым инстансом vertex/index
buffer и биндить их перед draw; для тысяч инстансов это лишние state-переходы
и копии. RHI уже умеет storage-буферы (ADR-0039) и их загрузку
`copy_buffer_to_buffer` (ADR-0046).

## Варианты

1. **Классические vertex/index buffers на инстанс.** Требует таблицы мешей,
   биндинга буферов и vertex layout v1 (ADR-0035); для одного unit-меша
   дублирует данные и усложняет batching.
2. **Bindless vertex fetch (descriptor index в push constants).** Гибко, но
   нужно несколько storage-дескрипторов и индекс на инстанс; избыточно для
   первого GPU-прохода.
3. **Vertex pulling: вершины и инстансы в storage-буферах, выборка по
   `SV_VertexID`/`SV_InstanceID`** (выбрано). Один unit-меш на все инстансы,
   per-instance данные — отдельный буфер.

## Решение

`render::geometry::SceneGeometry`:

- `vertices: BufferId` — единичный куб (36 вершин, 24 B: position + normal),
  `instances: BufferId` — `capacity × 48 B` (позиция/масштаб, albedo,
  roughness, metallic, padding); оба device-local
  (`storage | transfer_dst`), заполняются staging → `copy_buffer_to_buffer`
  (ADR-0046).
- Descriptor set 0: binding 0 — `StructuredBuffer<SceneVertex>`, binding 1 —
  `StructuredBuffer<SceneInstance>`.
- Pipeline `scene-gbuffer`: `scene.vert.hlsl` (`vs_main`) +
  `scene_gbuffer.frag.hlsl` (`fs_main`), `vertex_stride = 0`, форматы
  G-buffer (ADR-0017) и `D32Float`, push constants 64 B —
  view-projection (column-major).
- `SceneGeometry::upload(device, scene)` берёт `visible_indices()` после
  `extract` (или все инстансы, если culling не запускался); позиции —
  camera-relative, view-матрица — только поворот.
- `GpuFrame::render_scene(device, params, geometry, camera)` подменяет
  источник G-buffer-прохода: `frame.gbuffer` рисует инстансы
  (`draw(36, instance_count)`) вместо полноэкранного треугольника; остальные
  проходы (SSAO → lighting → bloom → tonemap → TAA) не меняются.
- Материалы — палитра-заглушка `MATERIAL_PALETTE` (`material_id % 4`) до
  появления ресурсов материалов; `mesh_id` зарезервирован под таблицу мешей.

## Следствия

- Один pipeline и один unit-меш обслуживают любое число инстансов; данные
  инстансов обновляются одним upload-ом, без пересоздания PSO.
- `GpuFrame::render` (процедурный кадр) остаётся для CPU-эталонов и
  PSNR-гейта; `render_scene` — путь реальной сцены.
- Vulkan-тест `scene_geometry_fills_gbuffer` проверяет центр куба: albedo,
  ORM (roughness 0.4, metallic 0), Reversed-Z глубину и чистый validation.
  Null шейдеры не исполняет, но проверяет upload/capacity/draw-команды.
- Попутно исправлен hang Vulkan: `read_query_pool` читает весь пул с `WAIT`,
  а отключённые проходы не писали таймстемпы. `GpuFrame` теперь закрывает
  неактивные слоты парой нулевых диапазонов — это же чинит
  `FrameFeatures::none()` на реальном GPU.
- Vertex layout v1 (ADR-0035) не задействован: сцена использует pulling;
  классические vertex buffers остаются для мешей с собственными атрибутами.
