# FluxEngine

Игровой движок на Rust для разработки игр: собственный RHI (Vulkan/DX12),
архетипная ECS, физика на f64, контент-пайплайн `.fpf`, сетевой слой,
скриптинг (Lua), анимации, AI, аудио, UI и редактор сцены.

## Документация

Вся проектная документация — в [`docs/`](docs/README.md).
Готовый single-file сайт: [`docs.html`](docs.html) — открывается локально
без сервера.

| Что | Где |
|---|---|
| Концепция и границы | [docs/vision.md](docs/vision.md) |
| Требования (MoSCoW) | [docs/requirements.md](docs/requirements.md) |
| Архитектура (HLD) | [docs/architecture.md](docs/architecture.md) |
| Канон имён и глоссарий | [docs/canon.md](docs/canon.md) |
| Данные и контракты | [docs/contracts.md](docs/contracts.md) |
| Roadmap разработки | [docs/Roadmap.md](docs/Roadmap.md) |
| ADR-процесс и решения | [docs/adr/README.md](docs/adr/README.md) |
| Стандарты разработки | [docs/development-standards.md](docs/development-standards.md) |
| Правила репозитория | [docs/repository-rules.md](docs/repository-rules.md) |
| Настройка окружения | [docs/setup.md](docs/setup.md) |
| Тестирование | [docs/testing.md](docs/testing.md) |
| CI/CD | [docs/ci-cd.md](docs/ci-cd.md) |
| Логи/метрики/ошибки | [docs/observability.md](docs/observability.md) |
| Юридический минимум | [docs/legal.md](docs/legal.md) |

## Быстрый старт

Workspace: 16 крейтов движка + 2 инструмента. Vulkan SDK для сборки не нужен —
графические бэкенды закрыты feature-гейтами.

```bat
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Мини-срез «движок дышит» (окно, clear color, два job на кадр, dev-overlay):

```bat
cargo run -p flux-engine --example minimal
```

Проверки документации и headless-правила:

```bat
node docs/scripts/check-links.mjs
node tools/ci/check-headless-deps.mjs
```

Подробно (Vulkan SDK, DX12, документационный SPA) — [docs/setup.md](docs/setup.md).

## Статус

Архитектурная фаза завершена (ADR-0001…0027). Этап 1 Roadmap
([docs/Roadmap.md](docs/Roadmap.md)) реализован: workspace, `flux-math`,
`flux-core`, фасад `flux-engine`, мини-срез M1 и заделы форматов.
Следующий этап — 2: архетипная ECS и `SchemaRegistry`.

## Лицензия

MIT — см. [LICENSE](LICENSE).
