<script setup>
import { computed, nextTick, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { searchIndex } from '../docs'
import Icon from './Icon.vue'

const props = defineProps({
  modelValue: { type: Boolean, default: false },
})
const emit = defineEmits(['update:modelValue'])

const router = useRouter()
const query = ref('')
const active = ref(0)
const inputEl = ref(null)

const results = computed(() => {
  const q = query.value.trim().toLowerCase()
  if (!q) return searchIndex.slice(0, 8)

  const found = []
  for (const item of searchIndex) {
    const title = item.title.toLowerCase()
    const description = item.description.toLowerCase()
    const position = item.text.toLowerCase().indexOf(q)

    let score = 0
    if (title.includes(q)) score += 100
    if (description.includes(q)) score += 40
    if (position >= 0) score += 10
    if (!score) continue

    const start = Math.max(0, position - 70)
    const snippet =
      position >= 0 ? item.text.slice(start, start + 180) : item.description

    found.push({ ...item, score, snippet })
  }

  return found.sort((a, b) => b.score - a.score).slice(0, 12)
})

watch(
  () => props.modelValue,
  async (open) => {
    if (!open) return
    query.value = ''
    active.value = 0
    await nextTick()
    inputEl.value?.focus()
  },
)

watch(results, () => {
  active.value = 0
})

function close() {
  emit('update:modelValue', false)
}

function go(item) {
  if (!item) return
  router.push({ name: 'doc', params: { id: item.id } })
  close()
}

function onKeydown(event) {
  const total = results.value.length
  if (!total) return

  if (event.key === 'ArrowDown') {
    event.preventDefault()
    active.value = (active.value + 1) % total
  } else if (event.key === 'ArrowUp') {
    event.preventDefault()
    active.value = (active.value - 1 + total) % total
  } else if (event.key === 'Enter') {
    event.preventDefault()
    go(results.value[active.value])
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="modelValue" class="search-overlay" @click.self="close">
      <div class="search-modal" role="dialog" aria-modal="true" aria-label="Поиск по документации">
        <div class="search-input-row">
          <Icon name="search" :size="18" />
          <input
            ref="inputEl"
            v-model="query"
            type="search"
            placeholder="Поиск по документации…"
            autocomplete="off"
            spellcheck="false"
            @keydown="onKeydown"
          />
          <kbd>Esc</kbd>
        </div>

        <div v-if="results.length" class="search-results">
          <button
            v-for="(item, i) in results"
            :key="item.id"
            type="button"
            :class="['search-result', { active: i === active }]"
            @mouseenter="active = i"
            @click="go(item)"
          >
            <span class="search-result-icon"><Icon :name="item.icon" :size="16" /></span>
            <span class="search-result-body">
              <span class="search-result-top">
                <strong>{{ item.title }}</strong>
                <span class="badge">{{ item.group }}</span>
              </span>
              <span class="search-result-snippet">{{ item.snippet }}</span>
            </span>
          </button>
        </div>

        <div v-else class="search-empty">
          Ничего не найдено. Попробуйте «детерминизм», «гейт», «fpf» или «SaveState».
        </div>

        <div class="search-footer">
          <span><kbd>↑</kbd><kbd>↓</kbd> навигация</span>
          <span><kbd>Enter</kbd> открыть</span>
          <span><kbd>Ctrl K</kbd> закрыть</span>
        </div>
      </div>
    </div>
  </Teleport>
</template>
