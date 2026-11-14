# FluxEngine — Логирование, мониторинг, ошибки

Консолидация решений из [architecture.md](architecture.md): flux-core/logging,
flux-core/profiling, паник-политика (ADR-0012), панель давления VRAM.

## 1. Логирование

- Библиотека: `tracing` + `tracing-subscriber`; фасад в flux-core/logging.
- Каналы (targets): `render`, `ecs`, `physics`, `anim`, `ai`, `audio`,
  `net`, `asset`, `script`, `editor`, `core`.
- Уровни: ERROR — сломан инвариант/потеря данных; WARN — деградация
  (fallback, давление VRAM, отклонённый upload); INFO — жизненный цикл
  (загрузка тайла, старт сцены); DEBUG — диагностика подсистем;
  TRACE — горячие пути (только точечно, с зонами профайлера).
- Фильтрация в рантайме: `RUST_LOG=render=debug,net=info`; каналы
  не отключаются целиком в release — ERROR/WARN всегда.
- Формат: структурированный (key=value), без персональных данных.
- Правила: не логировать в горячих путях без необходимости; каждый WARN/ERROR
  содержит контекст (сущность/UUID/тайл); «молчаливых» ошибок нет
  (orphaned-политика логирует отбросы).

## 2. Метрики (profiling/telemetry)

- Frametime: P50/P99, пропущенные present-интервалы, delta запрошенного/
  фактического present-времени.
- По подсистемам: тик симуляции, extract, загрузка тайла, применение дельты,
  компиляция пермутации, flush-стриминг.
- VRAM: total/reserved/used/pressure (Ok/Warning/Critical/Overflow) —
  переходы уровней = события + маркеры профайлера.
- CPU: загрузка воркеров, длина очередей job system.
- Сеть: RTT, loss, bandwidth per client, расхождения дельт.
- Экспорт: Chrome Trace / Tracy; dev-overlay (debug-draw) с ключевыми
  метриками с этапа 2.

## 3. Ошибки

- Ожидаемый отказ → `Result` с пер-крейтовым enum (thiserror);
  `anyhow` только в бинарниках.
- Нарушение инварианта → паника; матрица поведения:
  dev-игра — красный экран + backtrace + breadcrumbs, кадр aborted;
  PIE — остановка play, откат к edit; editor edit — дамп + выход;
  release — дамп через CrashHandler + чистый выход.
- Паника в job: catch_unwind на воркере, статус Panicked поднимается к
  корню кадра; зависимые системы не исполняются; воркер жив.
- FFI/RT-audio: catch_unwind на входе в чужой стек; аудио — silent-output
  + error state; Drop не паникует.
- Ошибки загрузки ассетов: fallback-ресурс (прокси/fallback-шейдер),
  лог WARN, продолжение работы; битые ссылки — отчёт в редакторе.

## 4. Crash-репорты

- CrashHandler (flux-core/crash): единый паник-хук, breadcrumbs
  (lock-free ring, последние N событий), минидамп-формат кроссплатформенный,
  per-OS реализация в flux-platform/crash (Windows — MiniDumpWriteDump).
- Dev: дамп не пишется, полный backtrace в консоль.
- Prod: дамп + breadcrumbs сохраняются локально; отправка — отдельная
  подсистема телеметрии (позже), только с согласия пользователя (см. legal.md).

## 5. Правила добавления метрик

- Новая метрика: канал + единица измерения + бюджет (если горячий путь).
- Метрика без бюджета — не гейт (наблюдение), с бюджетом — perf-гейт.
- Метрики, влияющие на решения (pressure, flush pending), обязаны иметь
  событие/маркер — «невидимых» деградаций быть не должно.
