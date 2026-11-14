# FluxEngine — документация

Здесь сосредоточена вся проектная документация: концепция, требования,
архитектура, контракты, стандарты, тестирование, CI/CD, ADR и SPA-сайт.

## Соответствие обязательным документам

| # | Документ | Файл | Статус |
|---|---|---|---|
| 1 | Концепция и границы проекта | [vision.md](vision.md) | ✅ создан |
| 2 | Требования (MoSCoW + DoD) | [requirements.md](requirements.md) | ✅ создан |
| 3 | Архитектура (HLD): стек, модули, диаграммы, владение, ограничения | [architecture.md](architecture.md) + [canon.md](canon.md) | ✅ есть |
| 4 | Схема данных и контракты, версии API | [contracts.md](contracts.md) | ✅ создан (сводка) |
| — | Реализованный API крейтов (дополнительно) | [api.md](api.md) | ✅ пополняется после каждой итерации |
| 5 | Стандарты разработки | [development-standards.md](development-standards.md) | ✅ создан |
| 6 | Правила работы с репозиторием | [repository-rules.md](repository-rules.md) | ✅ создан |
| 7 | Настройка окружения (Onboarding) | [setup.md](setup.md) | ✅ создан |
| 8 | План тестирования | [testing.md](testing.md) | ✅ создан (детали — в плане) |
| 9 | CI/CD и релизы | [ci-cd.md](ci-cd.md) | ✅ создан |
| 10 | Roadmap и декомпозиция | [Roadmap.md](Roadmap.md) | ✅ есть (гейты вместо дедлайнов) |
| 11 | ADR | [adr/README.md](adr/README.md) | ✅ есть (ADR-0001…0033) |
| 12 | Логирование, мониторинг, ошибки | [observability.md](observability.md) | ✅ создан |
| 13 | Юридический минимум | [legal.md](legal.md) | ✅ создан |

## Документация SPA

Исходники — `docs/spa` (Vue 3 + Vite). SPA импортирует markdown напрямую
через `?raw`: копий контента нет, расхождение невозможно.

```bat
cd docs/spa
npm install
npm run dev      # http://localhost:5175/docs.html
npm run build    # single-file сборка → docs.html в корне проекта
npm run preview  # предпросмотр собранной версии
```

Готовый `docs.html` открывается двойным кликом (hash-роутинг, без сервера).

## Структура

```
docs/
├── vision.md                # концепция и границы
├── requirements.md          # MoSCoW-требования с критериями готовности
├── architecture.md          # полный архитектурный план
├── canon.md                 # канон имён + глоссарий
├── contracts.md             # данные и форматы обмена + версионирование
├── api.md                   # реализованный API крейтов (пополняется по итерациям)
├── development-standards.md # стиль, коммиты, ревью
├── repository-rules.md      # trunk-based, релизы, защита main
├── setup.md                 # поднять проект с нуля
├── testing.md               # уровни, критические сценарии, гейты
├── ci-cd.md                 # пайплайн, nightly, релиз, откат
├── observability.md         # логи, метрики, ошибки, crash-репорты
├── legal.md                 # лицензии, данные, контент
├── Roadmap.md               # этапы, WP, майлстоуны, гейты, трассировка
├── adr/README.md            # ADR-процесс + индекс решений
├── README.md                # этот файл
├── scripts/                 # check-links.mjs (регистрочувствительные ссылки)
└── spa/                     # SPA-документация (Vue 3 + Vite)
    ├── docs.html            # entry для dev/build
    ├── package.json
    ├── vite.config.js       # single-file → docs.html в корне проекта
    └── src/
        ├── docs.js          # ?raw-импорты всех markdown
        ├── markdown.js      # markdown-it, якоря, task-lists, highlight
        ├── mermaid.js       # рендеринг диаграмм
        ├── router.js        # hash-роутинг для file://
        ├── theme.js         # светлая/тёмная тема
        ├── styles.css
        └── components/      # Sidebar, TopBar, DocPage, HomePage, SearchPalette, Icon
```

## Правила синхронизации

- Источник истины — markdown-файлы в `docs/`. SPA импортирует их напрямую
  (`?raw`), поэтому копий контента нет: правка markdown сразу видна после
  пересборки `docs.html`.
- Приоритет при расхождении — [canon.md](canon.md) (канон имён).
- Любое изменение контрактов/форматов/канона — через [ADR-процесс](adr/README.md).
- Roadmap намеренно не содержит временных оценок: движение по гейтам
  (G1–G7), а не по датам.
- **Числовые пороги гейтов** (PSNR, миллисекунды, проценты, часы) хранятся
  только в [Roadmap.md](Roadmap.md); остальные документы ссылаются на номер
  гейта. Дублирование чисел — дефект документации.
- SPA отображает все документы из `docs/`. Новый раздел = markdown-файл +
  запись в `docs/spa/src/docs.js`.
- Каждый документ имеет владельца и статус (таблица ниже). Документ без
  владельца не считается действующим.
- Проверка: `node docs/scripts/check-links.mjs` — регистрочувствительная
  проверка относительных ссылок (ловит коллизии вида `architecture.md` /
  `ARCHITECTURE.md`).

## Владение и статус документов

Статусы: **Living** — обновляется по мере работы; **Stable** — меняется только
через ADR или по триггеру (канон, контракты, процесс).

| Документ | Владелец | Статус |
|---|---|---|
| [vision.md](vision.md) | архитектор | Stable |
| [requirements.md](requirements.md) | архитектор | Living |
| [architecture.md](architecture.md) | архитектор | Living |
| [canon.md](canon.md) | архитектор | Living (изменения — через ADR) |
| [contracts.md](contracts.md) | архитектор | Living |
| [api.md](api.md) | архитектор + владельцы крейтов | Living (после каждой итерации) |
| [development-standards.md](development-standards.md) | владелец проекта | Stable |
| [repository-rules.md](repository-rules.md) | владелец проекта | Stable |
| [setup.md](setup.md) | инфраструктура/CI | Living |
| [testing.md](testing.md) | инфраструктура/CI | Living |
| [ci-cd.md](ci-cd.md) | инфраструктура/CI | Living |
| [observability.md](observability.md) | flux-core | Living |
| [legal.md](legal.md) | владелец проекта | Stable |
| [Roadmap.md](Roadmap.md) | архитектор | Living |
| [adr/README.md](adr/README.md) | архитектор | Living |
