<script setup>
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { docs, docById, resolveDocLink } from '../docs'
import { md } from '../markdown'
import { renderMermaid } from '../mermaid'
import { theme } from '../theme'
import Icon from './Icon.vue'

const route = useRoute()
const router = useRouter()

const doc = computed(() => docById(route.params.id))
const html = computed(() => (doc.value ? md.render(doc.value.content) : ''))

const contentEl = ref(null)
const toc = ref([])
const activeId = ref('')
const progress = ref(0)

const index = computed(() => docs.findIndex((item) => item.id === doc.value?.id))
const prev = computed(() => (index.value > 0 ? docs[index.value - 1] : null))
const next = computed(() =>
  index.value >= 0 && index.value < docs.length - 1 ? docs[index.value + 1] : null,
)

const readingTime = computed(() => {
  if (!doc.value) return 0
  const words = doc.value.content
    .replace(/[#>*_~|`-]/g, ' ')
    .split(/\s+/)
    .filter(Boolean).length
  return Math.max(1, Math.round(words / 190))
})

let observer = null

function scrollToId(id) {
  if (!id) return
  const el = document.getElementById(id)
  if (!el) return
  const top = el.getBoundingClientRect().top + window.scrollY - 88
  window.scrollTo({ top, behavior: 'smooth' })
}

async function enhance() {
  await nextTick()
  const root = contentEl.value
  if (!root) return

  root.querySelectorAll('table').forEach((table) => {
    if (table.parentElement?.classList.contains('table-wrap')) return
    const wrap = document.createElement('div')
    wrap.className = 'table-wrap'
    table.before(wrap)
    wrap.append(table)
  })

  root.querySelectorAll('pre').forEach((pre) => {
    if (pre.closest('.mermaid') || pre.parentElement?.classList.contains('code-block')) return
    const wrap = document.createElement('div')
    wrap.className = 'code-block'
    pre.before(wrap)
    wrap.append(pre)

    const button = document.createElement('button')
    button.type = 'button'
    button.className = 'copy-btn'
    button.textContent = 'Копировать'
    button.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(pre.innerText)
        button.textContent = 'Скопировано'
        window.setTimeout(() => {
          button.textContent = 'Копировать'
        }, 1400)
      } catch {
        /* clipboard может быть недоступен на file:// */
      }
    })
    wrap.append(button)
  })

  root.querySelectorAll('a[href^="http"]').forEach((link) => {
    link.target = '_blank'
    link.rel = 'noreferrer noopener'
  })

  toc.value = [...root.querySelectorAll('h2, h3')]
    .map((heading) => ({
      id: heading.id,
      level: Number(heading.tagName[1]),
      text: heading.textContent.replace(/^#\s*/, '').trim(),
    }))
    .filter((item) => item.id)

  await renderMermaid(root, theme.value)
  setupObserver()

  if (route.query.h) scrollToId(String(route.query.h))
}

function setupObserver() {
  observer?.disconnect()
  const headings = contentEl.value?.querySelectorAll('h2, h3')
  if (!headings?.length) return

  observer = new IntersectionObserver(
    (entries) => {
      const visible = entries
        .filter((entry) => entry.isIntersecting)
        .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0]
      if (visible) activeId.value = visible.target.id
    },
    { rootMargin: '-96px 0px -68% 0px', threshold: [0, 1] },
  )
  headings.forEach((heading) => observer.observe(heading))
}

function handleClick(event) {
  const link = event.target.closest('a')
  if (!link) return
  const href = link.getAttribute('href')
  if (!href) return

  if (href.startsWith('#')) {
    event.preventDefault()
    scrollToId(href.slice(1))
    return
  }
  if (/^[a-z]+:/i.test(href)) return

  if (href.includes('.md')) {
    event.preventDefault()
    const [path, hash] = href.split('#')
    const id = resolveDocLink(path)
    if (id) {
      router.push({ name: 'doc', params: { id }, query: hash ? { h: hash } : {} })
    }
    return
  }

  // Не-markdown относительные ссылки (LICENSE и т.п.) в single-file сборке недоступны.
  event.preventDefault()
}

function onScroll() {
  const scrollable = document.documentElement.scrollHeight - window.innerHeight
  progress.value = scrollable > 0 ? Math.min(1, window.scrollY / scrollable) : 0
}

watch(
  () => route.params.id,
  async () => {
    activeId.value = ''
    window.scrollTo({ top: 0 })
    await enhance()
  },
)

watch(theme, async () => {
  await renderMermaid(contentEl.value, theme.value)
})

onMounted(async () => {
  window.addEventListener('scroll', onScroll, { passive: true })
  onScroll()
  await enhance()
})

onBeforeUnmount(() => {
  window.removeEventListener('scroll', onScroll)
  observer?.disconnect()
})
</script>

<template>
  <div v-if="!doc" class="not-found">
    <Icon name="alert" :size="40" />
    <h1>Документ не найден</h1>
    <p>Возможно, ссылка устарела. Вернитесь к обзору документации.</p>
    <router-link to="/" class="btn-primary">На главную</router-link>
  </div>

  <article v-else class="doc">
    <div class="reading-progress" aria-hidden="true">
      <span :style="{ width: `${progress * 100}%` }" />
    </div>

    <header class="doc-header">
      <div class="doc-kicker">
        <Icon :name="doc.icon" :size="15" />
        <span>{{ doc.kicker }}</span>
      </div>
      <h1>{{ doc.title }}</h1>
      <p class="doc-lead">{{ doc.description }}</p>
      <div class="doc-meta">
        <span><Icon name="clock" :size="14" />{{ readingTime }} мин чтения</span>
        <span><Icon name="file" :size="14" /><code>docs/{{ doc.file }}</code></span>
        <span class="badge">{{ doc.groupLabel }}</span>
      </div>
    </header>

    <div class="doc-layout">
      <div ref="contentEl" class="prose" v-html="html" @click="handleClick" />

      <aside v-if="toc.length" class="toc" aria-label="Содержание страницы">
        <div class="toc-title">На этой странице</div>
        <a
          v-for="item in toc"
          :key="item.id"
          :href="`#${item.id}`"
          :class="['toc-link', `lvl-${item.level}`, { active: activeId === item.id }]"
          @click.prevent="scrollToId(item.id)"
        >
          {{ item.text }}
        </a>
      </aside>
    </div>

    <nav class="doc-nav" aria-label="Соседние документы">
      <router-link
        v-if="prev"
        :to="{ name: 'doc', params: { id: prev.id } }"
        class="doc-nav-link prev"
      >
        <Icon name="arrow-left" :size="18" />
        <span>
          <small>Назад</small>
          {{ prev.title }}
        </span>
      </router-link>
      <span v-else class="doc-nav-spacer" />

      <router-link
        v-if="next"
        :to="{ name: 'doc', params: { id: next.id } }"
        class="doc-nav-link next"
      >
        <span>
          <small>Далее</small>
          {{ next.title }}
        </span>
        <Icon name="arrow-right" :size="18" />
      </router-link>
    </nav>
  </article>
</template>
