# 0040. flux-release: semantic-release для Rust-workspace

- Статус: Accepted
- Дата: 2026-11-14
- Затрагивает: tools/flux-release, CI, canon
- Supersedes: —

## Контекст

Версии и `CHANGELOG.md` ведутся вручную: номер версии правится в
`Cargo.toml`, changelog — отдельным коммитом, тег — третьим. Это даёт
расхождения (версия в манифесте ≠ тег), пропущенные breaking changes и
недетерминированные changelog-секции. В JS-экосистеме эту задачу решает
`semantic-release`: коммиты в формате Conventional Commits определяют bump,
changelog и тег.

Для Rust нужен свой инструмент: `semantic-release` не умеет
`Cargo.toml`/workspace, path-зависимости и `cargo publish`, а
`cargo-release` не строит changelog из коммитов и не поддерживает
semantic-release-конфиг.

## Варианты

1. **Свой `tools/flux-release`** (выбрано). Полный контроль над Cargo-спецификой
   (workspace, path-зависимости, publish), Conventional Commits, TOML-конфиг,
   импорт `.releaserc.json` (subset), без Node в CI.
2. **semantic-release + плагины.** Тянет Node и плагины для Cargo; workspace и
   path-зависимости всё равно требуют своего кода; changelog-формат жёсткий.
3. **cargo-release.** Хорош для bump/tag/publish, но не парсит коммиты и не
   генерирует changelog; пришлось бы комбинировать два инструмента.
4. **Скрипт на Node.** Дублирует Rust-инструменты, ломает CPU-only Rust-пайплайн
   и не имеет типизированного API.

## Решение

`tools/flux-release` — третий инструмент workspace (после flux-assetc и
flux-build), CLI + библиотека.

- **Commits:** Conventional Commits 1.0 — `type(scope)!: subject`, body и
  footers `BREAKING CHANGE:`/`BREAKING-CHANGE:`. Merge-коммиты, release-коммиты
  (`chore(release)`) и маркеры `[skip release]` исключаются.
- **Bump:** `feat` → minor, `fix`/`perf`/`revert` → patch, `!`/breaking → major.
  Для `0.x` breaking по умолчанию даёт minor (конфигурируемо). Pre-release
  каналы `alpha`/`beta`/`rc` с автоинкрементом номера.
- **Версия:** `semver`; чтение и запись `[workspace.package].version` и
  `[package].version` через `toml_edit` (форматирование сохраняется). Режимы
  `fixed` (одна версия) и `independent` (несколько пакетов); path-зависимости
  с `version` обновляются для публикации.
- **Changelog:** Keep a Changelog; секции Breaking/Added/Fixed/Changed/
  Performance/Documentation/Internal; ссылки на коммиты; локализация en/ru;
  встроенный шаблон и `--template`. Вставка идемпотентна: существующая версия
  не дублируется.
- **Git:** через CLI (`git`), без libgit2: `log`/`status`/`tag`/`commit`/`push`;
  annotated tag; `--no-verify`, `--no-git-checks`; диапазон `--from`/`--to`;
  `--tag-prefix`. Защита: грязное дерево, существующий тег, отсутствие коммитов.
- **Публикация (opt-in):** `cargo package`/`cargo publish --dry-run`/`cargo
  publish` в порядке зависимостей; `cargo-semver-checks` — отдельный opt-in
  вызов, его отсутствие не ломает релиз без флага.
- **GitHub Release (opt-in):** notes из changelog + `gh release create` при
  наличии `GITHUB_TOKEN`/`gh`.
- **CLI:** `release`, `version`, `changelog`, `check`, `init`, `doctor`;
  `--dry-run` (ничего не пишет) и `--json` (машинный вывод для CI).
  `check --commits <range>` — PR-гейт Conventional Commits.
- **CI-режим:** `--ci`, `GITHUB_TOKEN`, ветка из окружения; повторный запуск
  идемпотентен (тег/версия уже есть → no-op с кодом 0).
- **Конфиг:** `.releaserc.toml`; при отсутствии читается `.releaserc.json`
  (subset: `branches`, `tagFormat`), остальные поля — из умолчаний. Валидация
  (`check`, `doctor`) до любых изменений.

Зависимости: `semver`, `toml_edit`, `serde`, `serde_json`, `thiserror`,
`clap` (derive). `tempfile` — dev-only для тестов. Все — MIT/Apache-2.0,
allowlist `deny.toml` не меняется.

## Следствия

Плюсы:

- Один источник истины для версии: манифест, changelog и тег меняются атомарно.
- Changelog детерминирован: сортировка по типу/scope, дата из git, без Node.
- Workspace и path-зависимости поддержаны штатно; fixed/independent — конфигом.
- `check --commits` ловит нарушения Conventional Commits до мержа.
- `--dry-run`/`--json` позволяют встроить инструмент в CI без побочных эффектов.

Минусы и риски:

- Парсер Conventional Commits — наша ответственность (тесты обязательны).
- `cargo publish` и `cargo-semver-checks` зависят от внешних инструментов;
  без флага релиз не ломается.
- Монорепо-режим `independent` сложнее fixed: порядок публикации по графу
  зависимостей нужно поддерживать при изменении workspace.
- `Cargo.lock` намеренно не переписывается: обновление выполняет `cargo`
  при следующей сборке/публикации.

Заделы:

- `flux-release init` создаёт конфиг и `CHANGELOG.md`.
- PR-гейт `flux-release check --commits origin/main..HEAD`.
- Release-job по тегу/`workflow_dispatch` с `--ci --publish`.
