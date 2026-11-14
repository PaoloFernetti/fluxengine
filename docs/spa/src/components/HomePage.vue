<script setup>
import { computed } from 'vue'
import { docById, docs } from '../docs'
import Icon from './Icon.vue'

const canon = docById('canon')?.content || ''
const adr = docById('adr')?.content || ''
const roadmap = docById('roadmap')?.content || ''
const requirements = docById('requirements')?.content || ''

const stats = computed(() => [
  {
    // flux-macros — служебный proc-macro пакет, не крейт движка (ADR-0028).
    value: (canon.match(/^\| `flux-/gm) || []).length - 1,
    label: 'крейтов движка',
    hint: '+ 2 инструмента + flux-macros',
  },
  {
    value: (roadmap.match(/^### \d+\.\d+/gm) || []).length,
    label: 'рабочих пакетов',
    hint: '7 этапов',
  },
  {
    value: (adr.match(/^\| \d{4} \|/gm) || []).length,
    label: 'ADR',
    hint: 'решения зафиксированы',
  },
  {
    value: (requirements.match(/^\| M\d+ \|/gm) || []).length,
    label: 'Must-требований',
    hint: 'с критериями готовности',
  },
])

const quickLinks = [
  {
    id: 'vision',
    icon: 'eye',
    title: 'Концепция',
    text: 'Зачем движок, для кого и что входит в v1.0.',
  },
  {
    id: 'roadmap',
    icon: 'route',
    title: 'Roadmap',
    text: '7 этапов, 60 пакетов, гейты G1–G7.',
  },
  {
    id: 'architecture',
    icon: 'layers',
    title: 'Архитектура',
    text: 'Полный план: подсистемы, потоки, решения.',
  },
  {
    id: 'canon',
    icon: 'book',
    title: 'Канон',
    text: 'Имена крейтов и глоссарий терминов.',
  },
]

const principles = [
  {
    icon: 'check',
    title: 'Детерминизм — инвариант',
    text: 'Fixed tick, seeded-RNG, канон i64 и CI-харнесс, а не лозунг.',
  },
  {
    icon: 'layers',
    title: 'Один источник истины',
    text: 'Канон позиций, SaveState, VRAM-бюджет и имена — в одном месте.',
  },
  {
    icon: 'workflow',
    title: 'Контракты важнее реализации',
    text: 'Подсистемы не знают друг друга, только trait-контракты.',
  },
  {
    icon: 'activity',
    title: 'Graceful degradation',
    text: 'Прокси-ресурсы, fallback-шейдеры и pressure-лестница VRAM.',
  },
  {
    icon: 'sparkles',
    title: 'Своё там, где важен опыт',
    text: 'Рендер, ECS, физика — свои; кодеки и окно — готовые.',
  },
]

const docCount = computed(() => docs.filter((doc) => doc.id !== 'index').length)
</script>

<template>
  <div class="home">
    <section class="hero">
      <div class="hero-glow" aria-hidden="true" />
      <div class="hero-badges">
        <span class="pill"><span class="status-dot" /> v1.0 · этап 1</span>
        <span class="pill pill-muted">Rust · Windows · DX12 + Vulkan</span>
      </div>
      <h1>
        FluxEngine
        <span class="hero-gradient">документация</span>
      </h1>
      <p class="hero-lead">
        Игровой движок на Rust: собственный RHI, архетипная ECS, физика на f64,
        контент-пайплайн <code>.fpf</code>, сеть, анимации, AI, аудио, UI и
        редактор сцены.
      </p>
      <div class="hero-actions">
        <router-link :to="{ name: 'doc', params: { id: 'roadmap' } }" class="btn-primary">
          <Icon name="route" :size="18" />
          Открыть Roadmap
        </router-link>
        <router-link :to="{ name: 'doc', params: { id: 'architecture' } }" class="btn-ghost">
          <Icon name="layers" :size="18" />
          Архитектура
        </router-link>
      </div>
    </section>

    <section class="stats">
      <div v-for="stat in stats" :key="stat.label" class="stat-card">
        <strong>{{ stat.value }}</strong>
        <span>{{ stat.label }}</span>
        <small>{{ stat.hint }}</small>
      </div>
    </section>

    <section class="section">
      <div class="section-head">
        <h2>С чего начать</h2>
        <span class="section-hint">{{ docCount }} документов · markdown как источник</span>
      </div>
      <div class="card-grid">
        <router-link
          v-for="link in quickLinks"
          :key="link.id"
          :to="{ name: 'doc', params: { id: link.id } }"
          class="card"
        >
          <span class="card-icon"><Icon :name="link.icon" :size="20" /></span>
          <strong>{{ link.title }}</strong>
          <p>{{ link.text }}</p>
          <span class="card-arrow"><Icon name="arrow-right" :size="16" /></span>
        </router-link>
      </div>
    </section>

    <section class="section">
      <div class="section-head">
        <h2>Принципы</h2>
        <span class="section-hint">не обсуждаются без ADR</span>
      </div>
      <div class="principle-grid">
        <div v-for="principle in principles" :key="principle.title" class="principle">
          <span class="principle-icon"><Icon :name="principle.icon" :size="18" /></span>
          <div>
            <strong>{{ principle.title }}</strong>
            <p>{{ principle.text }}</p>
          </div>
        </div>
      </div>
    </section>

    <section class="section">
      <div class="status-panel">
        <div class="status-main">
          <span class="status-dot" />
          <div>
            <strong>Текущий этап — 1: скелет workspace</strong>
            <p>
              Гейт G1: окно и цикл кадра, параллельные job-ы, 0 аллокаций после
              прогрева, 0 unwrap в lib.
            </p>
          </div>
        </div>
        <router-link :to="{ name: 'doc', params: { id: 'roadmap' } }" class="btn-ghost small">
          Этапы и гейты
          <Icon name="chevron-right" :size="16" />
        </router-link>
      </div>
    </section>
  </div>
</template>
