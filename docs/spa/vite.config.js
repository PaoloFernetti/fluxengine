import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { viteSingleFile } from 'vite-plugin-singlefile'
import { fileURLToPath, URL } from 'node:url'

const root = fileURLToPath(new URL('.', import.meta.url))

export default defineConfig({
  root,
  base: './',
  plugins: [
    vue(),
    viteSingleFile({ removeViteModuleLoader: true }),
  ],
  build: {
    // Сборка в dist; postbuild копирует единственный docs.html в корень проекта.
    outDir: 'dist',
    emptyOutDir: true,
    cssCodeSplit: false,
    assetsInlineLimit: 100_000_000,
    reportCompressedSize: false,
    rollupOptions: {
      input: fileURLToPath(new URL('./docs.html', import.meta.url)),
      output: { inlineDynamicImports: true },
    },
  },
  server: {
    port: 5175,
    open: '/docs.html',
  },
})
