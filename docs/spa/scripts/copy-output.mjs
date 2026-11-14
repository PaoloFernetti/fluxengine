// Копирует single-file сборку docs/spa/dist/docs.html в корень проекта.
// Запускается после `vite build` (npm run build).

import { copyFile, stat } from 'node:fs/promises'
import { fileURLToPath, URL } from 'node:url'

const source = fileURLToPath(new URL('../dist/docs.html', import.meta.url))
const target = fileURLToPath(new URL('../../../docs.html', import.meta.url))

await copyFile(source, target)
const { size } = await stat(target)
console.log(`docs.html → корень проекта (${(size / 1024 / 1024).toFixed(2)} МБ)`)
