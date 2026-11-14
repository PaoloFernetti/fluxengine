// Проверка относительных markdown-ссылок.
//
// Зачем: имена файлов, различающиеся только регистром (architecture.md vs
// ARCHITECTURE.md), неразличимы на Windows, но ломаются на Linux/CI. Проверка
// сравнивает имена с точностью до регистра, поэтому ловит такие коллизии.
//
// Запуск: node docs/scripts/check-links.mjs

import { readdir, readFile, stat } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = path.resolve(fileURLToPath(new URL('../../', import.meta.url)))
const docsRoot = path.join(repoRoot, 'docs')
const SKIP_DIRS = new Set(['node_modules', 'dist', '.git'])

async function collectMarkdown(dir) {
  const result = []
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (SKIP_DIRS.has(entry.name)) continue
      result.push(...(await collectMarkdown(path.join(dir, entry.name))))
    } else if (entry.isFile() && entry.name.endsWith('.md')) {
      result.push(path.join(dir, entry.name))
    }
  }
  return result
}

async function exactCaseExists(target) {
  try {
    const info = await stat(target)
    if (!info.isFile()) return false
  } catch {
    return false
  }
  const dir = path.dirname(target)
  const name = path.basename(target)
  const entries = await readdir(dir)
  return entries.includes(name)
}

const files = [...(await collectMarkdown(docsRoot)), path.join(repoRoot, 'README.md')]
const problems = []
let checked = 0

for (const file of files) {
  const text = await readFile(file, 'utf8')
  const linkPattern = /\]\(([^)\s]+?\.md)(#[^)]*)?\)/g
  for (const match of text.matchAll(linkPattern)) {
    const rawTarget = match[1]
    if (/^[a-z]+:/i.test(rawTarget) || rawTarget.startsWith('/')) continue
    checked += 1
    const target = path.resolve(path.dirname(file), rawTarget)
    if (!(await exactCaseExists(target))) {
      const rel = path.relative(repoRoot, file)
      problems.push(`${rel}: битая ссылка -> ${rawTarget}`)
    }
  }
}

if (problems.length) {
  console.error(`Найдено проблем: ${problems.length} (проверено ссылок: ${checked})`)
  for (const problem of problems) console.error(`  ${problem}`)
  process.exit(1)
}

console.log(`Ссылки в порядке: проверено ${checked} в ${files.length} файлах`)
