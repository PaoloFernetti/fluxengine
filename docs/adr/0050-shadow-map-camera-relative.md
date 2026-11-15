# 0050. Shadow map: camera-relative light space

- Статус: Accepted
- Дата: 2026-11-15
- Затрагивает: flux-graphics
- Supersedes: —

## Контекст

Пункт 4.6 требует camera-relative теней. ADR-0049 дал depth-only PSO и
состояние глубины, но самого прохода не было: нужна карта теней, которая

- строится из одного направленного источника света;
- не теряет точность при уходе камеры от world origin (ADR-0008:
  floating origin → camera-relative рендер);
- читается обратно для гейта (readback глубины).

Инстансы сцены уже camera-relative: [`RenderScene::interpolate`] вычитает
позицию камеры, а view-матрица камеры — только поворот. Поэтому light
view-projection не должен зависеть от world origin.

## Варианты

1. **World-space light matrix** (свет смотрит в world origin). При смещении
   мира на 1e6 f32-точность глубины и X/Y карты теряется; тени «плывут».
2. **Каскадные shadow maps (CSM).** Правильное решение для больших сцен, но
   дороже: несколько карт, выбор каскада, стабилизация. Отложено.
3. **Одна camera-relative ортопроекция вокруг origin** (выбрано): инстансы
   уже относительно камеры, матрица света строится в том же пространстве и
   не зависит от мировых координат.

## Решение

- Константы `render::geometry`:
  - `SHADOW_MAP_SIZE = 512`;
  - `SHADOW_EXTENT = 12.0` — полуразмер ортопроекции вокруг камеры;
  - `SHADOW_LIGHT_DIR = Vec3::new(0.5, -1.0, 0.3)` — направление света по
    умолчанию.
- `shadow_view_projection(light_dir) -> Mat4` — `look_at_rh` из
  `-dir * SHADOW_EXTENT` в `Vec3::ZERO` плюс Reversed-Z ортопроекция
  (`x/y = 1/E`, `z_clip = z_view / (2E) + 1`, `w = 1`): near → 1, far → 0,
  как у камеры (ADR-0008). Вырожденный `light_dir` заменяется на `-Y`;
  при взгляде вдоль Y опорный up — `Z`.
- `SceneGeometry::shadow_pipeline` — depth-only PSO: `D32Float`,
  `DepthCompare::GreaterOrEqual`, `depth_write: true`, `vertex_stride = 0`,
  те же descriptor set 0 и push constants 64 B, что у G-buffer.
- `shaders/shadow_scene.vert.hlsl` — vertex pulling. Обязательны
  `[[vk::binding(0, 0)]]`/`[[vk::binding(1, 0)]]` для storage-буферов и
  `[[vk::push_constant]]` для light view-projection (как в
  `scene.vert.hlsl`). Без этих атрибутов DXC для SPIR-V генерирует
  UBO/дескрипторы, push constants до шейдера не доходят, матрица нулевая и
  все примитивы отсекаются (симптом — пустая карта при чистом validation).
- `frame.shadow` — первый пасс графа при наличии сцены:
  `RenderPassDesc::with_depth(shadow)`, clear depth `0.0`, store. Проход не
  входит в `FRAME_PASS_NAMES`/FrameStats: пул остаётся 12 запросов.
- `FrameStage::Shadow`; `readback` для этой стадии использует размер
  `SHADOW_MAP_SIZE × SHADOW_MAP_SIZE × 4`, а не размер кадра.
- Камера-relative контракт: `RenderScene::set_camera_position` задаёт точку
  вычитания; `GpuFrame::render_scene` строит `shadow_push` из
  `shadow_view_projection(SHADOW_LIGHT_DIR)` и не использует
  `camera.position`.

## Следствия

- Тени не зависят от мировых координат: сдвиг камеры и инстанса на `1e6`
  даёт побайтно ту же карту. Тест
  `tests/vulkan_frame.rs::shadow_map_records_camera_relative_depth`:
  карта не пуста, глубина куба в `(0, 1)`, угол — clear `0.0`, near/far
  карты совпадают, validation чистый, `live_resources() == 0`.
- Null-тест `null_frame_records_shadow_pass` фиксирует размер карты и
  отсутствие ошибок RHI (Null глубину не исполняет — нули).
- Ограничения и задел:
  - одна карта без каскадов, фиксированное направление света;
  - `GraphicsSettings::shadow_resolution` (512…4096) пока не связан с
    `SHADOW_MAP_SIZE` — константа 512 до появления CSM/настройки прохода;
  - сэмплирование карты (PCF) в `frame.lighting` и IBL/скайбокс — следующие
    шаги 4.6; их camera-relative направления используют тот же принцип
    (направления не зависят от origin).
- `shadow.frag.hlsl` остаётся пустым fragment-шейдером (ADR-0049): цветовых
  выходов нет.
