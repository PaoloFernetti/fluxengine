// Единственный источник контента — markdown-файлы в docs/.
// Они импортируются как строки (?raw), поэтому SPA не может «разъехаться»
// с документацией: копий контента в JS нет.

import vision from '../../vision.md?raw'
import requirements from '../../requirements.md?raw'
import architecture from '../../architecture.md?raw'
import canon from '../../canon.md?raw'
import contracts from '../../contracts.md?raw'
import api from '../../api.md?raw'
import roadmap from '../../Roadmap.md?raw'
import adr from '../../adr/README.md?raw'
import adr0028 from '../../adr/0028-flux-macros.md?raw'
import adr0029 from '../../adr/0029-ecs-deps-and-budget.md?raw'
import adr0030 from '../../adr/0030-schema-and-delta-format.md?raw'
import adr0031 from '../../adr/0031-input-abstraction.md?raw'
import adr0032 from '../../adr/0032-collision-minimum.md?raw'
import adr0033 from '../../adr/0033-headless-platform-determinism.md?raw'
import standards from '../../development-standards.md?raw'
import repoRules from '../../repository-rules.md?raw'
import setup from '../../setup.md?raw'
import testing from '../../testing.md?raw'
import ciCd from '../../ci-cd.md?raw'
import observability from '../../observability.md?raw'
import legal from '../../legal.md?raw'
import docsIndex from '../../README.md?raw'

export const groups = [
  { id: 'start', label: 'Начало', icon: 'compass' },
  { id: 'arch', label: 'Архитектура', icon: 'layers' },
  { id: 'plan', label: 'План', icon: 'route' },
  { id: 'decisions', label: 'Решения', icon: 'gavel' },
  { id: 'process', label: 'Процессы', icon: 'workflow' },
  { id: 'practice', label: 'Практика', icon: 'terminal' },
  { id: 'legal', label: 'Правовое', icon: 'scale' },
  { id: 'index', label: 'Индекс', icon: 'book' },
]

// Убираем первый H1: заголовок страницы рендерит DocPage из метаданных.
function stripTitle(markdown) {
  return markdown.replace(/^#\s+.*\r?\n+/, '')
}

function makeDoc({ id, file, title, nav, group, icon, kicker, description, raw }) {
  return {
    id,
    file,
    title,
    nav: nav || title,
    group,
    groupLabel: groups.find((g) => g.id === group)?.label || '',
    icon,
    kicker,
    description,
    content: stripTitle(raw),
    raw,
  }
}

export const docs = [
  makeDoc({
    id: 'vision',
    file: 'vision.md',
    title: 'Концепция и границы проекта',
    nav: 'Концепция',
    group: 'start',
    icon: 'eye',
    kicker: 'Vision / Product Brief',
    description: 'Зачем нужен движок, для кого он, что входит в v1.0 и что сознательно отложено.',
    raw: vision,
  }),
  makeDoc({
    id: 'requirements',
    file: 'requirements.md',
    title: 'Требования (MoSCoW)',
    nav: 'Требования',
    group: 'start',
    icon: 'list',
    kicker: 'Requirements',
    description: '11 Must, 12 Should, Could и Won\'t — с измеримыми критериями готовности.',
    raw: requirements,
  }),
  makeDoc({
    id: 'architecture',
    file: 'architecture.md',
    title: 'Архитектурный план',
    nav: 'Архитектура',
    group: 'arch',
    icon: 'layers',
    kicker: 'High-Level Design',
    description: 'Полный план: 16 крейтов, 8 групп подсистем, потоки данных и сквозные решения.',
    raw: architecture,
  }),
  makeDoc({
    id: 'canon',
    file: 'canon.md',
    title: 'Канон имён и глоссарий',
    nav: 'Канон',
    group: 'arch',
    icon: 'book',
    kicker: 'Naming Canon',
    description: 'Единственный источник истины по именам крейтов и терминам движка.',
    raw: canon,
  }),
  makeDoc({
    id: 'contracts',
    file: 'contracts.md',
    title: 'Схема данных и контракты',
    nav: 'Контракты',
    group: 'arch',
    icon: 'database',
    kicker: 'Data Contracts',
    description: 'Модель данных, форматы обмена, версии API и правила обратной совместимости.',
    raw: contracts,
  }),
  makeDoc({
    id: 'api',
    file: 'api.md',
    title: 'API (реализовано)',
    nav: 'API',
    group: 'arch',
    icon: 'code',
    kicker: 'Implemented API',
    description: 'Публичные контракты реализованных крейтов; пополняется после каждой итерации плана.',
    raw: api,
  }),
  makeDoc({
    id: 'roadmap',
    file: 'Roadmap.md',
    title: 'Roadmap разработки',
    nav: 'Roadmap',
    group: 'plan',
    icon: 'route',
    kicker: 'Delivery Plan',
    description: '7 этапов, 60 рабочих пакетов, майлстоуны M1–M7 и гейты G1–G7.',
    raw: roadmap,
  }),
  makeDoc({
    id: 'adr',
    file: 'adr/README.md',
    title: 'Architecture Decision Records',
    nav: 'ADR',
    group: 'decisions',
    icon: 'gavel',
    kicker: 'Decision Records',
    description: 'Процесс принятия решений, жизненный цикл и индекс ADR-0001…0033.',
    raw: adr,
  }),
  makeDoc({
    id: 'adr-0028',
    file: 'adr/0028-flux-macros.md',
    title: 'ADR-0028. Служебный proc-macro пакет flux-macros',
    nav: 'ADR-0028',
    group: 'decisions',
    icon: 'gavel',
    kicker: 'Decision Record',
    description: 'Почему flux-macros — честный пакет workspace и как меняется приёмка канона.',
    raw: adr0028,
  }),
  makeDoc({
    id: 'adr-0029',
    file: 'adr/0029-ecs-deps-and-budget.md',
    title: 'ADR-0029. Внешние зависимости этапа 2 и бюджет тика ECS',
    nav: 'ADR-0029',
    group: 'decisions',
    icon: 'gavel',
    kicker: 'Decision Record',
    description: 'Один ADR на внешний набор этапа 2 и метод измерения гейта G2.',
    raw: adr0029,
  }),
  makeDoc({
    id: 'adr-0030',
    file: 'adr/0030-schema-and-delta-format.md',
    title: 'ADR-0030. Формат SchemaRegistry и ComponentDelta',
    nav: 'ADR-0030',
    group: 'decisions',
    icon: 'gavel',
    kicker: 'Decision Record',
    description: 'field_uuid, SchemaRef, единая дельта для сейва, сети и undo.',
    raw: adr0030,
  }),
  makeDoc({
    id: 'adr-0031',
    file: 'adr/0031-input-abstraction.md',
    title: 'ADR-0031. Input abstraction: scancode-модель, ActionValue, контексты, Intention',
    nav: 'ADR-0031',
    group: 'decisions',
    icon: 'gavel',
    kicker: 'Decision Record',
    description: 'Четыре слоя ввода и Intention как единственный вход симуляции.',
    raw: adr0031,
  }),
  makeDoc({
    id: 'adr-0032',
    file: 'adr/0032-collision-minimum.md',
    title: 'ADR-0032. Collision-минимум: тайлы, капсула/луч, свипы, детерминированный обход',
    nav: 'ADR-0032',
    group: 'decisions',
    icon: 'gavel',
    kicker: 'Decision Record',
    description: 'Тайловый grid и свипы без solver для KCC-минимума.',
    raw: adr0032,
  }),
  makeDoc({
    id: 'adr-0033',
    file: 'adr/0033-headless-platform-determinism.md',
    title: 'ADR-0033. Headless-платформа и детерминизм-харнесс: feature-гейт, same-binary CI',
    nav: 'ADR-0033',
    group: 'decisions',
    icon: 'gavel',
    kicker: 'Decision Record',
    description: 'winit/gilrs за feature-гейтами; CI без окон, 10k тиков same-binary.',
    raw: adr0033,
  }),
  makeDoc({
    id: 'standards',
    file: 'development-standards.md',
    title: 'Стандарты разработки',
    nav: 'Стандарты',
    group: 'process',
    icon: 'code',
    kicker: 'Engineering Standards',
    description: 'Именование, структура крейтов, Rust-стиль, коммиты и правила ревью.',
    raw: standards,
  }),
  makeDoc({
    id: 'repo',
    file: 'repository-rules.md',
    title: 'Правила работы с репозиторием',
    nav: 'Репозиторий',
    group: 'process',
    icon: 'git',
    kicker: 'Repository Rules',
    description: 'Trunk-based, что попадает в main, SemVer, релизы и защита ветки.',
    raw: repoRules,
  }),
  makeDoc({
    id: 'ci',
    file: 'ci-cd.md',
    title: 'CI/CD и релизы',
    nav: 'CI/CD',
    group: 'process',
    icon: 'rocket',
    kicker: 'CI / CD',
    description: 'PR-пайплайн, nightly, weekly, релизный процесс и откат.',
    raw: ciCd,
  }),
  makeDoc({
    id: 'observability',
    file: 'observability.md',
    title: 'Логирование, мониторинг, ошибки',
    nav: 'Наблюдаемость',
    group: 'process',
    icon: 'activity',
    kicker: 'Observability',
    description: 'Каналы логов, метрики, матрица паник и crash-репорты.',
    raw: observability,
  }),
  makeDoc({
    id: 'setup',
    file: 'setup.md',
    title: 'Настройка окружения',
    nav: 'Окружение',
    group: 'practice',
    icon: 'terminal',
    kicker: 'Onboarding',
    description: 'С чистой Windows-машины до сборки, тестов и запуска документации.',
    raw: setup,
  }),
  makeDoc({
    id: 'testing',
    file: 'testing.md',
    title: 'План тестирования',
    nav: 'Тестирование',
    group: 'practice',
    icon: 'check',
    kicker: 'Test Strategy',
    description: 'Уровни тестов, 10 критических сценариев и регрессионная политика.',
    raw: testing,
  }),
  makeDoc({
    id: 'legal',
    file: 'legal.md',
    title: 'Юридический минимум',
    nav: 'Правовое',
    group: 'legal',
    icon: 'scale',
    kicker: 'Legal',
    description: 'Лицензия MIT, allowlist зависимостей, данные, контент и секреты.',
    raw: legal,
  }),
  makeDoc({
    id: 'index',
    file: 'README.md',
    title: 'Индекс документации',
    nav: 'Индекс',
    group: 'index',
    icon: 'folder',
    kicker: 'Documentation Map',
    description: 'Карта документов, правила синхронизации и владельцы.',
    raw: docsIndex,
  }),
]

export const docById = (id) => docs.find((doc) => doc.id === id) || null

// Ссылки внутри markdown относительные (`canon.md`, `adr/README.md`).
// Преобразуем их в id документа SPA.
export function resolveDocLink(href) {
  if (!href) return null
  const clean = href.split('#')[0].replace(/^\.\//, '')
  if (!clean.endsWith('.md')) return null
  if (clean.includes('adr/')) return 'adr'
  const base = clean.split('/').filter(Boolean).pop()
  if (!base) return null
  if (base.toLowerCase() === 'readme.md') return 'index'
  const doc = docs.find((item) => item.file.toLowerCase() === base.toLowerCase())
  return doc ? doc.id : null
}

function plainText(markdown) {
  return markdown
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/`[^`]*`/g, ' ')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/[#>*_~|]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
}

export const searchIndex = docs.map((doc) => ({
  id: doc.id,
  title: doc.title,
  description: doc.description,
  group: doc.groupLabel,
  icon: doc.icon,
  text: plainText(doc.content),
}))
