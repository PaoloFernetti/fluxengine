# flux-engine

Фасад, точка сборки и главный цикл кадра. Единственный крейт, который знает
конкретные подсистемы; остальные видят только контракты `flux-core/contracts`.

Канон: [`docs/canon.md`](../../docs/canon.md). План: [`docs/architecture.md`](../../docs/architecture.md),
раздел «Фасад и точка сборки». Roadmap: WP 1.5–1.6.

## Ответственность одной строкой

Композиция подсистем, `EngineBuilder`, `Engine::{tick, frame, acquire}`,
plugin-система и dev-overlay.

## Контракт цикла (ADR-0027)

Цикл владеет вызывающий, а не движок:

```rust
let mut engine = EngineBuilder::new()
    .with_plugin(GamePlugin)
    .build()?;

loop {
    pump_events();          // обязанность вызывающего
    engine.tick();          // 0..N фиксированных шагов
    engine.frame();         // extract → record → submit → present
    engine.acquire();       // ожидание GPU (редактор может пропускать)
}
```

`EngineBuilder::run()` — тонкая обёртка для игр без собственного event loop
(headless, сервер, soak-тесты). Окно и event loop движок не трогает.

## Dev-overlay и M3 (этап 3, I4)

[`overlay`](src/overlay.rs) — read-only снимок `OverlayData { sim_tick, frame,
worker_threads, entities }` и `summary()` для заголовка окна/лога. Собирается
из `TimeContext`, последнего `FrameStats` и `JobSystem::worker_count()`; вызов
не меняет тик и порядок систем, поэтому безопасен в детерминизм-прогонах.
`Engine::set_entity_count` позволяет игре сообщить число сущностей.

Пример [`examples/m3.rs`](examples/m3.rs): `winit` 0.30 `ApplicationHandler` +
`softbuffer` 0.4. Каждый `RedrawRequested` двигает `DeterminismHarness` на тик,
рисует прямоугольник и обновляет заголовок `tick=… hash=…` со сводкой overlay.
Запуск: `cargo run -p flux-engine --example m3`.

## Детерминизм-харнесс (этап 3, I3)

[`determinism`](src/determinism.rs) — `DeterminismHarness` с seeded-ботом:
10k тиков, хеш по каноническому `SaveJournal`-диффу (дельты по UUID, поля по
`field_uuid`), `StateHash::diff` печатает первую различающуюся сущность/поле.
CI-шаг: `node tools/ci/determinism-smoke.mjs` (same-binary).

## ECS-интеграция KCC (этап 3, I2)

Модуль [`simulation`](src/simulation.rs) — единственное место, где физика
встречается с ECS:

- `CharacterBody` — компонент-обёртка (`CharacterState` + `CharacterConfig` +
  `Intention` + `hit`);
- `PhysicsStep` — ресурс фиксированного шага;
- `TileGrid` — ресурс геометрии;
- `controller_system` (`Stage::UPDATE`) и `controller_post_system`
  (`Stage::POST`); пост-система сбрасывает one-shot `jump` и `hit`.

`flux-physics` остаётся чистым (канон: physics → core); ECS-обвязка живёт в
фасаде. Порядок тика: Input → Controller → Physics → ControllerPost (ADR-0024).

## Границы

- Не содержит геймплейной логики — только плагины и ECS-системы.
- Не знает деталей RHI — только контракт `Renderer`.
- Не редактор: `flux-editor` — отдельное приложение поверх этого крейта.
