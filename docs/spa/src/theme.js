import { ref, watch } from 'vue'

const STORAGE_KEY = 'fluxengine-docs-theme'

function initialTheme() {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (stored === 'light' || stored === 'dark') return stored
  } catch {
    /* localStorage недоступен (file://, приватный режим) */
  }
  return window.matchMedia?.('(prefers-color-scheme: light)').matches ? 'light' : 'dark'
}

export const theme = ref(initialTheme())

function apply(value) {
  document.documentElement.dataset.theme = value
  const meta = document.querySelector('meta[name="color-scheme"]')
  if (meta) meta.setAttribute('content', value === 'dark' ? 'dark light' : 'light dark')
}

export function toggleTheme() {
  theme.value = theme.value === 'dark' ? 'light' : 'dark'
}

watch(
  theme,
  (value) => {
    apply(value)
    try {
      localStorage.setItem(STORAGE_KEY, value)
    } catch {
      /* ignore */
    }
  },
  { immediate: true },
)
