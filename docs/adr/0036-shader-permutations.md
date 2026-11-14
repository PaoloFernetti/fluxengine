# 0036. Шейдерные пермутации: декларации, keyword-маски, fallback и кэш

- Статус: Accepted
- Дата: 2026-11-13
- Затрагивает: flux-graphics, flux-asset (I6b), flux-engine
- Supersedes: —

## Контекст

ADR-0014 зафиксировал модель пермутаций (функциональные варианты × keywords
материала, `shader.json`, off-demand компиляция, fallback, кэш трёх уровней),
но не описывал структуры данных и контракт runtime-ядра. I6 вводит эти
структуры. Без единого ABI декларации, масок и ключей компилятор в
`tools/flux-assetc`, рантайм и тесты начнут расходиться в трактовке
keywords, что приведёт к «чёрному экрану» вместо ошибки сборки.

Дополнительно требуется JSON-парсер для `shader.json`. В дереве зависимостей
уже есть `serde` и `serde_json` (dev-путь через `criterion`); выбор
`serde_json` фиксируется здесь как осознанное решение, а не транзитивная
зависимость.

## Варианты

1. **`shader.json` + `serde`/`serde_json` в runtime-ядре** (выбрано).
   Декларация — данные, а не код; валидация на загрузке; JSON — требование
   архитектуры и удобен инструментам.
2. **RON вместо JSON.** Меньше зависимостей, но расходится с зафиксированным
   именем `shader.json` и ожиданиями контент-пайплайна.
3. **Собственный парсер JSON.** Дорого, риск ошибок, нет выгоды.
4. **Только builder-API без файлов.** Не покрывает ABI контента и офлайн-
   компиляцию.

## Решение

- `ShaderDeclaration` — сериализуемая структура (`shader.json`): `name`,
  `shader_uuid`, `stages` (vertex/fragment/compute), `pipeline_variants`,
  `keywords` (наборы `pipeline`/`mesh`/`material`), `vertex_inputs`,
  `bindings`.
- Валидация на загрузке: UUID ≠ 0; есть vertex и fragment/compute; ≤ 64
  keywords (маска `u64`); ≤ 6 material-keywords; уникальные имена keywords,
  вариантов, vertex location и `(group, binding)`; вариант ссылается только
  на pipeline-keywords; формат вершины — `R32G32B32_SFLOAT` (layout v1,
  ADR-0035).
- `KeywordMask` — битовая маска, бит = индекс keyword в порядке декларации.
  Порядок фиксирован декларацией, поэтому маска детерминирована.
- `PermutationRequest` — полный ключ `pipeline × mesh × material`;
  `hash()` — FNV-1a по UUID, индексу варианта и маскам. Ключ стабилен между
  процессами и платформами.
- `ShaderCompiler` — trait компиляции в SPIR-V. Runtime-ядро не знает HLSL.
  `DeterministicCompiler` — тестовая заглушка, байткод которой **не является**
  валидным SPIR-V.
- `ShaderLibrary`: `register`, `precompile_core` (каждый pipeline-вариант без
  mesh/material keywords — fallback-ядро), `resolve` (кэш → компиляция →
  fallback), `stats`.
- Fallback-политика: неизвестный вариант, маска чужого набора, ошибка
  компилятора и превышение бюджета дают fallback-байткод и инкремент счётчика
  в `CacheStats`. Незарегистрированный UUID — типизированная ошибка.
- Кэш раздельный (ADR-0014): байткод, PSO и материал. Runtime хранит все три
  уровня в `PermutationCache`; PSO-уровень заполняет рендер после создания
  pipeline.
- Зависимость: `serde_json` добавляется в workspace для парсинга
  `shader.json`.

## Следствия

Плюсы:

- Один ABI декларации для рантайма, `flux-assetc` и тестов.
- Ошибки конфигурации типизированы и ловятся до GPU.
- Детерминированные ключи: кэш и golden-image воспроизводимы.
- Off-demand путь с fallback не блокирует кадр.

Минусы и риски:

- `serde_json` — новая прямая зависимость runtime-крейта.
- Бюджет 64 keywords и 6 material-keywords — жёсткий предел; рост требует
  нового ADR (дробление через ubo-branching).
- `DeterministicCompiler` не проверяет реальный SPIR-V; это делает DXC-путь.

Заделы:

- **I6b**: DXC офлайн через `tools/flux-assetc`, реальные SPIR-V и PSO-кадр.
- I7: FeatureRegistry порождает pipeline-набор keywords и пересборку графа.
- I8: golden-image привязывается к ключу пермутации и версии компилятора.
