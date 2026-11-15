# 0049. Depth-only pipelines и состояние глубины

- Статус: Accepted
- Дата: 2026-11-15
- Затрагивает: flux-graphics
- Supersedes: —

## Контекст

Пункт 4.6 требует camera-relative теней, IBL и скайбокса. Для них нужны два
режима, которых не было в контракте PSO:

- **Тени** — проход без цветовых аттачментов: пишется только глубина из
  источника света (shadow map).
- **Скайбокс** — depth-test без записи (`Equal` против дальней плоскости,
  Reversed-Z = 0.0), чтобы небо не перекрывало геометрию и не меняло depth.

`PipelineDesc` требовал непустой `color_formats`, а `build_pipeline` жёстко
включал `depth_test_enable(true)`, `depth_write_enable(true)` и
`CompareOp::GREATER_OR_EQUAL`. Render pass depth-only уже поддерживался:
`RenderPassDesc::with_depth` без color-аттачментов проходит через
`has_attachments` и `create_render_pass`.

## Варианты

1. **Отдельный тип `DepthPipelineDesc`.** Дублирует layout, биндинги и
   push constants; два контракта вместо одного.
2. **Писать глубину в цветовой R8-таргет.** Теряется depth-тест и точность,
   появляется лишний формат и обход RHI.
3. **Расширить `PipelineDesc`** (выбрано): пустые `color_formats` допустимы
   при наличии `depth_format`; состояние глубины задаётся явно.

## Решение

- `DepthCompare` (`Never`, `Less`, `Equal`, `LessOrEqual`, `Greater`,
  `GreaterOrEqual`, `Always`), по умолчанию `GreaterOrEqual` — Reversed-Z
  (ADR-0008).
- `PipelineDesc` получает `depth_compare: DepthCompare` и `depth_write: bool`
  (`true` по умолчанию); builder — `with_depth_state(compare, write)`.
- Graphics pipeline валиден, если есть color-форматы **или** depth-формат;
  иначе — `RhiError::Validation`.
- Vulkan: `depth_compare` → `vk::CompareOp`, `depth_write` →
  `depthWriteEnable`; при отсутствии depth-аттачмента depth-stencil state
  остаётся выключенным.
- Null-бэкенд хранит `PipelineDesc` как раньше — новые поля доступны
  диагностике без отдельной ветки.
- `shadow.frag.hlsl` — пустой fragment-шейдер: цветовых выходов нет,
  глубина берётся из растеризации.

## Следствия

- Тени и скайбокс реализуются без обхода RHI: shadow pass — depth-only
  render pass, sky — `depth_write: false` с `DepthCompare::Equal`.
- Контракт остаётся статическим (`PipelineDesc: Copy`); динамические форматы
  по-прежнему вне области решения.
- Тест `tests/vulkan_pso.rs::depth_only_pipeline_renders_shadow_pass`:
  depth-only PSO и draw без validation-ошибок; PSO без color и без depth
  отвергается; ресурсы освобождены. Контрактный тест
  `rhi::types::depth_state_defaults_to_reversed_z` фиксирует значения по
  умолчанию.
