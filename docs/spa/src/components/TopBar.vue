<script setup>
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { docById } from '../docs'
import { theme, toggleTheme } from '../theme'
import Icon from './Icon.vue'

defineEmits(['menu', 'search'])

const route = useRoute()
const current = computed(() => docById(route.params.id))
</script>

<template>
  <header class="topbar">
    <button class="icon-btn menu-btn" type="button" aria-label="Меню" @click="$emit('menu')">
      <Icon name="menu" :size="20" />
    </button>

    <nav class="breadcrumb" aria-label="Хлебные крошки">
      <router-link to="/" class="crumb">Документация</router-link>
      <template v-if="current">
        <Icon name="chevron-right" :size="14" class="crumb-sep" />
        <span class="crumb crumb-current">{{ current.nav }}</span>
      </template>
    </nav>

    <div class="topbar-actions">
      <button class="search-trigger" type="button" @click="$emit('search')">
        <Icon name="search" :size="16" />
        <span class="search-label">Поиск</span>
        <kbd>Ctrl K</kbd>
      </button>

      <button
        class="icon-btn"
        type="button"
        :aria-label="theme === 'dark' ? 'Светлая тема' : 'Тёмная тема'"
        @click="toggleTheme"
      >
        <Icon :name="theme === 'dark' ? 'sun' : 'moon'" :size="18" />
      </button>

      <a
        class="icon-btn repo-link"
        href="https://github.com/PaoloFernetti/fluxengine"
        target="_blank"
        rel="noreferrer noopener"
        aria-label="Репозиторий на GitHub"
      >
        <Icon name="github" :size="18" />
      </a>
    </div>
  </header>
</template>
