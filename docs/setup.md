# FluxEngine — Настройка окружения (Setup / Onboarding)

Цель: с чистой Windows-машины поднять проект и прогнать тесты.

> **Статус: workspace этапа 1 собран.** 16 крейтов + 2 инструмента, команды
> `-p flux-*` работают; гейт G1 пройден локально (fmt, clippy, тесты,
> headless-линтер, док-линт). Следующие этапы — [Roadmap.md](Roadmap.md).

## 1. Предварительные требования

| Компонент | Версия | Зачем |
|---|---|---|
| Windows | 10/11 x64 | release-платформа |
| Rust (rustup) | stable + MSVC toolchain | сборка |
| Visual Studio Build Tools | с «Desktop development with C++» | MSVC линкер, Windows SDK (DX12) |
| Vulkan SDK | актуальный | validation layers, DXC, dev-бэкенд |
| GPU-драйверы | актуальные | Vulkan/DX12 |
| Node.js | LTS | только для SPA-документации (docs/) |
| Git | актуальный | LFS опционально (golden-бинарники) |

Проверка: `rustc --version`, `cargo --version`, `vulkaninfo --summary`,
`node --version`.

## 2. Клонирование и первичная сборка

```bat
git clone <repo-url> fluxengine
cd fluxengine
cargo build --workspace
cargo test --workspace
```

Особенности:
- По умолчанию flux-graphics собирается без бэкендов (feature off) —
  быстрый build без Vulkan SDK.
- С Vulkan: `cargo build -p flux-graphics --features vulkan`.
- С DX12: `cargo build -p flux-graphics --features dx12` (требует Windows SDK).
- Headless-тесты рендера (lavapipe) — отдельный профиль CI; локально:
  установить Mesa lavapipe и запустить `cargo test -p flux-graphics --features vulkan`.

## 3. Запуск

```bat
cargo run -p flux-engine --example minimal   :: мини-срез: окно + цикл кадра
cargo run -p flux-assetc -- --help           :: компилятор ассетов
cargo run -p flux-build -- build dev         :: сборка контента (после этапа 5)
```

Переменные окружения (dev):
- `RUST_LOG=info` — уровень логов (каналы: `RUST_LOG=render=debug,ecs=trace`);
- `RUST_BACKTRACE=full` — полные бэктрейсы (dev-политика падений);
- `FLUX_GPU_BACKEND=vulkan|dx12` — выбор бэкенда (по умолчанию из config);
- `FLUX_ASSET_ROOT=<path>` — корень исходников ассетов (dev).

## 4. Документация (SPA)

```bat
cd docs\spa
npm install
npm run dev      :: http://localhost:5175/docs.html
npm run build    :: single-file сборка → docs.html в корне проекта
```

Примечание: SPA импортирует markdown напрямую; отдельных data-файлов нет.
Готовый `docs.html` открывается локально без сервера.

## 5. Тесты и проверки

```bat
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo bench -p flux-ecs        :: бенчмарк-гейт ECS (после этапа 2)
```

## 6. Чистая машина — чек-лист «я смогу запустить через полгода»

- [ ] Установлен rustup stable + MSVC Build Tools
- [ ] `cargo build --workspace` проходит без Vulkan SDK (feature off)
- [ ] `cargo test --workspace` зелёный
- [ ] Документация открывается локально (`cd docs && npm i && npm run dev`)
- [ ] Указаны все env-переменные dev-режима
- [ ] Известно, где лежат golden-ассеты и кэши (вне репозитория)

## 7. Типовые проблемы

| Симптом | Причина | Решение |
|---|---|---|
| `link.exe not found` | нет MSVC Build Tools | установить C++ workload |
| `Vulkan not found` | нет Vulkan SDK | установить SDK или собрать без feature |
| DX12-сборка падает | нет Windows SDK | установить SDK 10 |
| lavapipe-тесты падают | нет Mesa | установить lavapipe или пропустить GPU-тесты |
| Медленная сборка | debug-профиль | `cargo build --release` для бенчей |
| `node_modules` в git | .gitignore | проверить, не форс-аддили |
