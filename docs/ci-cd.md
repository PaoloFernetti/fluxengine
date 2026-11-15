# FluxEngine — Стратегия CI/CD и релизов

Полная матрица раннеров — в [architecture.md](architecture.md), раздел
«Тестирование и CI» (ADR-0021); гейты — в [Roadmap.md](Roadmap.md).

## 1. Пайплайн PR (CPU-only)

Обязательные шаги (каждый — блокер мержа):
1. `cargo fmt --check`
2. `cargo clippy --workspace --all-targets -- -D warnings` (включая unwrap-lint)
3. `cargo test --workspace`
4. `cargo build --workspace --release` — сборка релизного профиля
5. G4 golden gate: `cargo test -p flux-graphics --test golden_gate`
   (per-runner baseline; изменение без `visual_change` — красный)
6. Shader-артефакты и PSO: `cargo test -p flux-graphics --features vulkan
   --test vulkan_pso` — закоммиченные SPIR-V валидны на любом раннере;
   реальный PSO-кадр с draw выполняется при наличии Vulkan-устройства.
   На headless-раннере тест пропускается, но только без
   `FLUX_REQUIRE_VULKAN=1`: с этой переменной отсутствие устройства — паника.
   Перекомпиляция HLSL требует DXC (`tools/flux-assetc shader --check`) и в
   PR-гейт не входит. DXC ставится командой `node tools/dxc/fetch.mjs`
   (последний релиз GitHub, SHA-256 из `digest`, идемпотентно; `--version`
   закрепляет версию) — шаг добавляется только в job, собирающий шейдеры.
7. Линтер зависимостей: lib-крейты не тянут winit/ash/gilrs (headless-правило)
8. Линтер лицензий зависимостей (cargo-deny, allowlist — см. legal.md)
9. ADR-чек: изменение контрактов/форматов/канона без ADR — красный
10. Job `vulkan-lavapipe` (ubuntu, Mesa lavapipe): RHI-юниты, PSO-смоук,
    surface/swapchain-смоук и GPU-проходы кадра с `FLUX_REQUIRE_VULKAN=1` —
    устройство обязательно, skip запрещён. Шаг `vulkan_frame` рендерит кадр,
    читает каждую стадию (G-buffer → SSAO → lighting → bloom → tonemap → TAA)
    и требует PSNR ≥ 60 дБ против CPU-эталона от readback предыдущего прохода
    (ADR-0042). Падения unit-тестов, PSO, surface и frame-шагов публикуются
    `::error`-аннотациями (имя упавшего теста и первая строка паники) — логи
    приватных прогонов читаются без доступа к аккаунту. Job входит в
    `sync-master`, поэтому red блокирует fast-forward `master`.
11. Determinism smoke (`node tools/ci/determinism-smoke.mjs`): same-binary,
    10k тиков, два прогона → один хеш; при затронутых симуляционных крейтах
12. Conventional Commits:
    `cargo run -p flux-release -- check --commits "origin/${{ github.base_ref }}..HEAD" --strict`
    — сообщения вне формата `type(module)!: subject` красят PR (ADR-0040)
13. Док-линт (`node docs/scripts/check-links.mjs`): относительные ссылки
    разрешаются с точностью до регистра; SPA собирается
    (`cd docs/spa && npm run build`) и `docs.html` в корне обновлён; отсутствие
    числовых порогов гейтов вне [Roadmap.md](Roadmap.md) и
    не-кириллических/не-латинских артефактов в тексте

    `docs.html` детерминирован: markdown нормализуется к LF при импорте
    (`docs/spa/src/docs.js`), поэтому сборка на Windows из CRLF-дерева
    побайтово совпадает с CI. Если `git diff -- docs.html` непустой при
    неизменном контенте — проверить `core.autocrlf=false` и LF в рабочем
    дереве (`.gitattributes` объявляет `eol=lf` для всех текстовых файлов).

Кэш: cargo registry + target (ключ — Cargo.lock); PSO/DXC-кэш — отдельный
кэш между ранами.

### 1.1 Ветки `dev` → `master`

Рабочая ветка — `dev`: сюда идут коммиты и сюда же смотрит `push`-триггер
workflow. `master` — стабильная ветка, обновляется только автоматическим
fast-forward-мержем из `dev` после зелёных тестов и сборки.

Защита веток — состояние GitHub-репозитория, а не файл: из локального клона
она не настраивается и не проверяется `cargo`. Поэтому пункт Roadmap 1.2
остаётся открытым до первого пуша workflow и настройки репозитория.

Порядок:

1. Запушить `dev` и `.github/workflows/ci.yml`, получить зелёный прогон на
   GitHub Actions (имена job-ов фиксированы и используются как required
   checks).
2. Включить protection для `master`: прямые push запрещены, кроме
   `github-actions[bot]` (иначе автoмерж не сможет обновить ветку).
   Для `dev` достаточно запрета force-push и удаления.

Обязательные параметры для `master`:

| Параметр | Значение |
|---|---|
| Require a pull request before merging | нет: в `master` пушит только `github-actions[bot]`; люди работают через PR в `dev` |
| Require status checks to pass | `fmt + clippy + test + build (CPU-only)`, `cargo-deny (licenses, advisories)`, `Vulkan on lavapipe (software GPU)`, `conventional commits (flux-release)`, `docs links + SPA build` |
| Require branches to be up to date | да |
| Do not allow force pushes | да |
| Do not allow deletions | да |
| Include administrators | нет (иначе автoмерж упрётся в protection) |

Через GitHub CLI (ставится отдельно, в репозиторий не входит):

```bash
gh api -X PUT "repos/PaoloFernetti/fluxengine/branches/master/protection" \
  -F required_status_checks.strict=true \
  -F 'required_status_checks.contexts[]=fmt + clippy + test + build (CPU-only)' \
  -F 'required_status_checks.contexts[]=cargo-deny (licenses, advisories)' \
  -F 'required_status_checks.contexts[]=Vulkan on lavapipe (software GPU)' \
  -F 'required_status_checks.contexts[]=conventional commits (flux-release)' \
  -F 'required_status_checks.contexts[]=docs links + SPA build' \
  -F enforce_admins=false \
  -F required_pull_request_reviews= \
  -F restrictions= \
  -F allow_force_pushes=false \
  -F allow_deletions=false
```

Проверка: `gh api repos/PaoloFernetti/fluxengine/branches/master/protection`.

Для `dev`:

```bash
gh api -X PUT "repos/PaoloFernetti/fluxengine/branches/dev/protection" \
  -F required_status_checks= \
  -F enforce_admins=false \
  -F required_pull_request_reviews= \
  -F restrictions= \
  -F allow_force_pushes=false \
  -F allow_deletions=false
```

#### Автoмерж и `github-actions[bot]`

Важно: список «Allow specified actors to bypass required pull requests»
действует только на **merge PR**, а не на прямой `git push`. `github-actions[bot]`
не выбирается там как отдельный актор. Для прямого push из job-а `release`
нужно одно из:

1. **Не включать «Require a pull request before merging» для `master`**
   (рекомендуется для этой схемы). Тогда `GITHUB_TOKEN` с
   `permissions: contents: write` пушит напрямую; required status checks
   остаются для PR, force-push и удаление запрещены.
2. **Оставить PR-требование и пушить PAT администратора.** Создать
   fine-grained PAT с `Contents: Read and write`, положить в секрет
   `RELEASE_TOKEN`; в `actions/checkout` указать
   `token: ${{ secrets.RELEASE_TOKEN || secrets.GITHUB_TOKEN }}`; в protection
   выключить «Do not allow bypassing the above settings»
   (`enforce_admins=false`). Тогда push идёт от пользователя-админа, а не от
   `github-actions[bot]`. В этом репозитории так и настроено: `RELEASE_TOKEN`
   с fallback на `GITHUB_TOKEN`, он же используется как `GH_TOKEN`.
   PAT-push может запускать workflow повторно; от повторного релиза защищает
   условие job-а по сообщению `chore(release):`.
3. **Rulesets + GitHub App.** Создать GitHub App с `Contents: write`,
   установить на репозиторий, добавить его в bypass-список ruleset для
   `master` и выпускать installation token через
   `actions/create-github-app-token`.

Проверка действующих правил:

```bash
gh api repos/PaoloFernetti/fluxengine/branches/master/protection
```

## 2. Nightly

| Раннер | Что | Условие |
|---|---|---|
| GPU-матрица | Полный golden на NVIDIA + AMD + Intel, validation layers | красный → блокирует мержи |
| Determinism full | 10k+ тиков, cross-binary, cross-platform (linux/windows) | красный → блокер |
| Fuzz-фарм | Непрерывный прогон .fpf/wire/десериализаторов; новые крэши → regression | красный → блокер |
| Perf | Бюджетные бенчи (порог регрессии — Roadmap, тренды в артефактах) | красный → блокер |
| Golden-сейвы | Сейвы всех релизов грузятся | красный → блокер |

## 3. Weekly

- Soak 24–48 ч: headless-симуляция с ботами; метрики RSS/frametime/дельты;
  тренд в артефактах.
- ASan-прогон юнит+интеграционных тестов.
- Полный фаззинг-корпус.

## 4. Релизный процесс

Версия, CHANGELOG, тег и GitHub Release автоматизированы `tools/flux-release`
(ADR-0040); ручными остаются проверки, shipping-сборка и артефакты.

1. Зелёный полный CI на `dev` + soak без трендов.
2. `cargo run -p flux-release -- release --dry-run --json` — проверить
   следующую версию, bump и запись changelog без записи файлов.
3. `cargo run -p flux-release -- release` — SemVer bump, версия в
   `Cargo.toml`, вставка в `CHANGELOG.md`, commit `chore(release): vX.Y.Z`,
   annotated tag. Pre-release — `--prerelease alpha|beta|rc`.
4. `tools/flux-build` собирает shipping-профиль; **детерминированный билд**:
   артефакт хешируется, повторная сборка на другом агенте — байт-в-байт
   (тест целостности).
5. Публикация — opt-in: `--push` (ветка + тег), `--publish`
   (`cargo publish`, при необходимости `--dry-run`), `--github-release`
   (notes из changelog, нужен `GITHUB_TOKEN`/`GH_TOKEN` и `gh`).
6. Release-манифест (хеши паков, версии форматов) + артефакты: паки,
   манифест, символы (PDB) — вложением релиза.

### 4.1 CI-режим релиза

- Workflow `.github/workflows/ci.yml`, job `release`: запускается после
  зелёных `rust` (fmt/clippy/test/build), `licenses` и `docs` при push в
  `dev` или вручную (`workflow_dispatch`); требует `contents: write`.
- Команда: `cargo run -p flux-release -- release --ci --push
  --github-release`; `--publish` включается отдельно, чтобы обычный релиз не
  публиковал крейты.
- Синхронизация `master` — отдельный job `sync-master` (не шаг `release`):
  `git checkout -B master origin/master`, `git merge --ff-only dev`,
  `git push origin master`. Он зависит от `release` и запускается при
  `always() && !cancelled()`, поэтому выполняется и на release-коммите, где
  сам `release` пропущен условием. Merge только fast-forward; расхождение
  веток = ошибка job-а и Issue.
- Коммит `chore(release): vX.Y.Z` исключается из повторного релиза условием
  job-а `release`. При push от PAT workflow запускается снова: job `release`
  пропускается по сообщению коммита, а `sync-master` доводит `master` до
  release-коммита.
- Отмена прогонов: `concurrency.cancel-in-progress` включён только для
  `pull_request`. Push в `dev` не отменяет собственный прогон — иначе
  release-job убивает себя после пуша тега и не успевает создать GitHub
  Release и обновить `master`.
- Идемпотентность: повторный запуск при существующем теге не двигает версию,
  но `--github-release` дозаполняет отсутствующий GitHub Release (проверка
  `gh release view` / `GET /releases/tags/<tag>` перед созданием), поэтому
  ретрай workflow безопасен и восстанавливает недоделанный релиз.
- PR-гейт (`release-check`) вызывает только `check --strict` и ничего не
  пишет; релиз с PR не запускается.
- `master` должна разрешать push от `github-actions[bot]`
  (branch protection: bypass для Actions или отдельный токен).

### 4.2 Issues при красном CI

Job `report-failure` запускается при любом падении `rust`/`licenses`/`docs`/
`release` на ветке `dev`:

- создаёт Issue `CI failure on dev: <short sha>` с ссылкой на run, коммитом и
  автором;
- если Issue для этого коммита уже открыт — добавляет комментарий, а не
  дублирует;
- после исправления и нового push в `dev` workflow проходит заново.

Права job-а: `issues: write`. Закрытие Issue — вручную после зелёного прогона.

## 5. Откат (rollback)

- Движок: откат тега не практикуется; фикс — новый PATCH-релиз.
- Контент/сейвы: read-time миграции (ADR-0023) позволяют откатить игру на
  предыдущую версию без порчи сейвов — это штатный сценарий.
- Паки: дельта-патчи строятся от предыдущего релиза; откат — публикация
  полного пака предыдущей версии (детерминизм гарантирует совместимость).
- Красный nightly после релиза — hotfix-ветка от тега, PATCH-релиз.

## 6. Артефакты падений (обязательны)

Автоматически прикладываются к PR/nightly:
- golden-диффы (тепловые карты);
- fuzz-кейсы (минимальные воспроизведения);
- perf-тренды (графики);
- soak-графики (RSS/frametime);
- логи с breadcrumbs (без персональных данных).

## 7. Принципы

- Один пайплайн для всех платформ — только флаги (никаких форков конфигов).
- Красный nightly блокирует мержи (red-green).
- Секреты — только в секретах CI; подписи артефактов — отдельный шаг.
