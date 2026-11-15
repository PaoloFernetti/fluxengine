# 0048. UI-stub и present в acquired-образ swapchain

- Статус: Accepted
- Дата: 2026-11-15
- Затрагивает: flux-graphics
- Supersedes: —

## Контекст

ADR-0045 открыл образы swapchain как `ImageId` (`acquire` → `ColorAttachment`,
`present` → `Present`), ADR-0047 дал реальную геометрию сцены. Последний шаг
пункта 4.5 — «… → UI-stub → present»: кадр должен выводиться в acquired-образ
цепочки вместе с оверлеем.

Внутренние изображения `GpuFrame` — `Rgba16Float` (HDR, TAA), а цепочка —
`Bgra8Srgb`/`Rgba8Srgb`. `PipelineDesc::color_formats` — статический срез,
поэтому один pipeline тонемапа на все форматы цепочки невозможен; при этом
внутренний выход нужен CPU-эталонам и readback-тестам.

## Варианты

1. **Тонемап сразу в образ цепочки.** Требует дублировать tonemap-pipeline под
   каждый формат, теряет внутренний HDR-выход для TAA/эталонов и смешивает
   вывод с пост-обработкой.
2. **Blit/копирование из внутреннего выхода в цепочку.** Не даёт нарисовать
   UI-оверлей; всё равно нужен отдельный проход, но без сэмплинга.
3. **Отдельный финальный проход `frame.ui`** (выбрано): читает tonemap/TAA-
   выход, рисует UI-stub и пишет в acquired-образ цепочки.

## Решение

- `GpuFrame::render_to(device, params, target, target_format)` добавляет в граф
  пасс `frame.ui` (`read` финального выхода, `color` цели) и пишет в `target`;
  состояние цели считается `ColorAttachment` (образ только что получен
  `acquire`, ADR-0045).
- `GpuFrame::present(device, params, swapchain)` = `acquire` →
  `render_to(acquired_image, swapchain.format())` → `present(index)`;
  возвращает индекс.
- Pipeline `frame-ui` (`fullscreen.vert` + `frame_ui.frag.hlsl`) создаётся
  лениво под формат цели и пересоздаётся при его смене; поддерживаются
  статические форматы `Bgra8Srgb`/`Rgba8Srgb`, иначе — `Unsupported`.
- Дескрипторный набор (изображение + sampler) перепривязывается каждый кадр к
  текущему финальному выходу: TAA ping-pong меняет изображение.
- UI-stub: затемнённая панель 30% × 25% в левом верхнем углу (blend 0.8) —
  заглушка до появления UI-системы; проверяет, что оверлей попадает в
  presented-кадр.
- `frame.ui` не входит в `FRAME_PASS_NAMES`/FrameStats: пул остаётся
  12 запросов, UI-проход не пишет таймстемпы.

## Следствия

- Пункт 4.5 закрыт по цепочке «геометрия → G-buffer → deferred lighting →
  UI-stub → present»; `render_to` же обслуживает будущий editor-RTT
  (`PresentTarget` из канона).
- Формат вывода ограничен sRGB 8-бит до появления runtime-форматов в
  `PipelineDesc` (отдельный ADR); HDR-цепочки — позже.
- Тесты: Null `null_frame_presents_ui_stub_to_swapchain` (acquire/render/present),
  Vulkan `ui_stub_is_presented_to_swapchain` — панель темнее эталонного кадра
  без UI, вне панели кадры совпадают, validation чистый, ресурсы освобождены.
