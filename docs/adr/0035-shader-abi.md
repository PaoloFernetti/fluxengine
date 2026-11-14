# 0035. Shader ABI: SPIR-V, entry points, vertex layout, pipeline descriptors

- Статус: Accepted
- Дата: 2026-11-13
- Затрагивает: flux-graphics, flux-asset (I6), flux-engine
- Supersedes: —

## Контекст

RHI I0 объявлял `PipelineDesc` без шейдеров, а `create_pipeline` был
заглушкой. I5 вводит реальные graphics/compute pipeline, и до I6 (компилятор
шейдеров, `shader.json`, пермутации) нужен стабильный ABI между контентом,
компилятором и бэкендами. Без него Vulkan и будущий DX12 начнут расходиться
в трактовке входов, форматов и vertex layout, а golden-image-тесты (I8)
потеряют точку опоры.

## Варианты

1. **SPIR-V как единственный вход RHI** (выбрано). Vulkan принимает его
   нативно; DX12 получает SPIR-V через кросс-компиляцию на этапе сборки
   шейдеров. Контракт минимален, байты детерминированы, хэш SPIR-V — ключ
   кэша PSO.
2. **DXIL + SPIR-V по бэкендам.** Двойной контракт, удвоенные тесты,
   рассинхронизация layout-ов и кэшей.
3. **Собственный шейдерный IR (wgpu-подобный).** Скрывает layout, но требует
   своего IR, трансляторов и отложенной валидации — цена больше выгоды на
   этапе 4.
4. **HLSL-исходники в рантайме.** Дорогая компиляция в кадре, недетерминизм,
   зависимость от DXC в рантайме.

## Решение

- Вход RHI — SPIR-V: `vertex_shader`, `fragment_shader`, `compute_shader`
  как `&'static [u8]`. Entry points по умолчанию `vs_main`/`fs_main`/`cs_main`,
  переопределяются `with_entries`.
- `PipelineDesc` фиксирует: `kind`, шейдеры, entry points, `color_formats`,
  `depth_format`, `vertex_stride`, `topology` (`TriangleList`/`LineList`/
  `PointList`).
- **Vertex layout v1**: одна позиция `R32G32B32_SFLOAT`, `vertex_stride = 12`.
  Нормали, UV, тангенты и instancing добавляются новым ADR с новым layout,
  а не расширением v1 «на месте».
- Render pass выводится из `color_formats` + `depth_format`; graphics PSO
  обязан совпадать с проходом, иначе — `RhiError::Validation`.
- `PipelineLayout` в I5 — без дескрипторов. Bindless-дескрипторы, uniform/
  storage-связки и push constants — отдельное решение (I6+).
- Пустые/битые SPIR-V, а также compute-шейдер в graphics-пайплайне —
  типизированная ошибка, не паника. Валидация не доверяет драйверу.
- Компиляция HLSL/DXC, `shader.json`, три набора keywords и кэш трёх
  уровней — I6. RHI получает уже скомпилированный SPIR-V и не знает
  об источниках.

## Следствия

Плюсы:

- Один ABI для Vulkan/DX12 и для golden-image-эталонов.
- Детерминированные байты: хэш SPIR-V — естественный ключ кэша PSO
  и пермутаций.
- Ошибки контракта типизированы и тестируемы без запуска драйвера.

Минусы и риски:

- DX12 требует кросс-компилятор SPIR-V → DXIL в пайплайне сборки.
- Layout v1 узкий: любой новый атрибут — новый ADR, а не тихое расширение.
- Невалидный SPIR-V может уронить драйвер: тесты I5 покрывают только
  типизированные отказы, реальные шейдеры приходят в I6 вместе с
  компилятором и валидатором.

Заделы:

- I6: `shader.json`, keywords, off-demand компиляция и fallback.
- I7: FeatureRegistry и пресеты порождают наборы пермутаций.
- I8: golden-image привязывается к хешу SPIR-V и версии компилятора.
