# 0051. Процедурное небо и аналитический IBL (camera-relative)

- Статус: Accepted
- Дата: 2026-11-15
- Затрагивает: flux-graphics
- Supersedes: —

## Контекст

Пункт 4.6 требует IBL и скайбокса с camera-relative направлениями. ADR-0049
дал depth-only PSO и режим `Equal` без записи для скайбокса, ADR-0050 —
camera-relative light space теней. Не хватает самой среды: неба, ambient и
источника направлений для IBL.

Ограничения текущего RHI и deferred-схемы:

- cubemap-проб (irradiance/prefiltered environment, BRDF LUT) и их запекания
  ещё нет;
- HDR-изображение пишется `frame.lighting` с `clear`, поэтому отдельный проход
  «небо поверх HDR» потребовал бы одновременно sampled и color attachment
  либо blend/load, которых в RHI пока нет;
- любой проход обязан быть camera-relative: позиции инстансов уже вычтены
  позицией камеры (ADR-0008).

## Варианты

1. **Полный IBL на cubemap-пробах.** Правильное решение, но нужен пайплайн
   запекания, cubemap-форматы, отдельные сэмплеры и BRDF LUT — объём вне шага.
2. **Отдельный проход `frame.sky`** (depth `Equal`, `depth_write = false`).
   Конфликтует с clear-only HDR: либо небо теряет уже посчитанный lighting,
   либо требуется blend/load и read-only depth-аттачмент.
3. **Аналитическая среда в `frame.lighting`** (выбрано): небо определяется по
   глубине, IBL — диффузный и зеркальный ambient от той же процедурной среды.

## Решение

- `FrameParams` получает `sky_zenith: [f32; 3]`, `sky_horizon: [f32; 3]`,
  `ibl_strength: f32`. Значения по умолчанию: зенит `[0.05, 0.10, 0.20]`,
  горизонт `[0.30, 0.35, 0.45]`, сила `1.0`.
- `FRAME_PUSH_CONSTANT_BYTES` расширяется с 64 до **96 B** (6 × float4):
  `params3 = (ssao_radius, ibl_strength, 0, 0)`, `params4 = sky_zenith.xyz`,
  `params5 = sky_horizon.xyz`. Минимум Vulkan для push constants — 128 B,
  запас сохраняется.
- `frame.lighting` читает глубину (binding 4, sampler — binding 5) и
  определяет небо по `depth <= 0` (Reversed-Z far, ADR-0008).
- Направление взгляда для неба — camera-relative и не зависит от origin:
  `ndc = (uv.x * 2 - 1, 1 - uv.y * 2)`, `aspect = width / height`,
  `dir = normalize(ndc.x * aspect, ndc.y, -1)` (FOV 90°, Y вверх).
- `sky_color(dir) = lerp(sky_horizon, sky_zenith, saturate(dir.y * 0.5 + 0.5))`.
- IBL (аналитическое приближение):
  - `ambient_diffuse = sky_color(n) * base_color * (1 - metallic) * ao * strength`;
  - `ambient_specular = sky_color(reflect(-v, n)) * f0 * ao * strength`;
  - прямое освещение — прежняя модель ADR-0017, но без early-return: при
    `n·l = 0` остаётся ambient.
- CPU-эталон `reference_lighting` получает `depth: &[f32]` и повторяет те же
  формулы; `sky_color`/`view_direction` — общие функции модуля.

## Следствия

- Скайбокс и IBL не зависят от world origin: используются camera-relative
  нормали, направления и глубина. Отдельный PSO/проход не нужен.
- PSNR-гейт `gpu_frame_passes_match_cpu_reference` обновлён: эталон lighting
  получает readback-глубину и включает IBL; все стадии по-прежнему ≥ 60 дБ.
- Новый Vulkan-тест `empty_scene_renders_procedural_sky`: пустая сцена
  (инстансов 0) оставляет глубину `0.0`, lighting отдаёт небо, PSNR ≥ 60 дБ,
  небо не чёрное, validation чистый, ресурсы освобождены.
- Null-тест `reference_sky_is_independent_of_surface_inputs` фиксирует, что
  цвет неба не зависит от G-buffer (albedo/normal/ORM).
- Ограничения и задел:
  - нет cubemap-проб, prefilter и BRDF LUT — спекуляр ambient не зависит от
    roughness;
  - FOV неба фиксирован 90°: камера в lighting не передаётся, а
    camera-relative view-матрица — только поворот;
  - `ibl_strength = 0` возвращает прежнюю модель прямого освещения;
  - CSM/тени в освещении и cubemap-пробы — следующие шаги.
