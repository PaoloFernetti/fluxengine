<script setup>
import { computed } from 'vue'
import { docs, groups } from '../docs'
import Icon from './Icon.vue'

defineProps({
  open: { type: Boolean, default: false },
})
defineEmits(['close'])

const grouped = computed(() =>
  groups
    .map((group) => ({
      ...group,
      items: docs.filter((doc) => doc.group === group.id),
    }))
    .filter((group) => group.items.length),
)
</script>

<template>
  <div v-if="open" class="sidebar-backdrop" @click="$emit('close')" />

  <aside :class="['sidebar', { open }]">
    <router-link to="/" class="brand" @click="$emit('close')">
      <span class="brand-mark" aria-hidden="true">F</span>
      <span class="brand-text">
        <strong>FluxEngine</strong>
        <small>документация</small>
      </span>
    </router-link>

    <nav class="nav" aria-label="Документация">
      <router-link to="/" class="nav-link nav-home" @click="$emit('close')">
        <Icon name="home" :size="16" />
        <span>Обзор</span>
      </router-link>

      <div v-for="group in grouped" :key="group.id" class="nav-group">
        <div class="nav-group-label">
          <Icon :name="group.icon" :size="14" />
          <span>{{ group.label }}</span>
        </div>
        <router-link
          v-for="doc in group.items"
          :key="doc.id"
          :to="{ name: 'doc', params: { id: doc.id } }"
          class="nav-link"
          @click="$emit('close')"
        >
          <Icon :name="doc.icon" :size="16" />
          <span>{{ doc.nav }}</span>
        </router-link>
      </div>
    </nav>

    <div class="sidebar-footer">
      <span class="status-dot" />
      <div>
        <strong>v1.0 · этап 1</strong>
        <small>Скелет workspace</small>
      </div>
    </div>
  </aside>
</template>
