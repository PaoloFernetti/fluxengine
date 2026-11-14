<script setup>
import { ref, watch, onMounted, onBeforeUnmount } from 'vue'
import { useRoute } from 'vue-router'
import Sidebar from './components/Sidebar.vue'
import TopBar from './components/TopBar.vue'
import SearchPalette from './components/SearchPalette.vue'

const route = useRoute()
const sidebarOpen = ref(false)
const searchOpen = ref(false)

watch(
  () => route.fullPath,
  () => {
    sidebarOpen.value = false
  },
)

function onKeydown(event) {
  const key = event.key.toLowerCase()
  if ((event.ctrlKey || event.metaKey) && key === 'k') {
    event.preventDefault()
    searchOpen.value = !searchOpen.value
  }
  if (event.key === 'Escape') {
    searchOpen.value = false
    sidebarOpen.value = false
  }
}

onMounted(() => window.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => window.removeEventListener('keydown', onKeydown))
</script>

<template>
  <div class="shell">
    <Sidebar :open="sidebarOpen" @close="sidebarOpen = false" />

    <div class="shell-main">
      <TopBar @menu="sidebarOpen = true" @search="searchOpen = true" />

      <main class="page">
        <router-view />
      </main>

      <footer class="site-footer">
        <span>FluxEngine · документация</span>
        <span class="dot">·</span>
        <span>markdown — единственный источник истины</span>
        <span class="dot">·</span>
        <span>MIT</span>
      </footer>
    </div>
  </div>

  <SearchPalette v-model="searchOpen" />
</template>
