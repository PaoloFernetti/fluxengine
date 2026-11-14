// Одноразовый генератор заготовок крейтов и инструментов (WP 1.1).
// Не перезаписывает существующие файлы.

import { mkdir, writeFile, access } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(fileURLToPath(new URL('../..', import.meta.url)))

const crates = [
  ['flux-ecs', 'Архетипная ECS, SchemaRegistry, планировщик', 2,
    'world, archetype, query, command, scheduler, schema, save'],
  ['flux-graphics', 'RHI, Vulkan/DX12, Render Graph, PBR', 4,
    'rhi, rhi::vulkan, rhi::dx12, render, shaders'],
  ['flux-platform', 'Окно, display, input, системные сервисы', 3,
    'window, display, input, cursor, clipboard, text, paths, power, devices, crash'],
  ['flux-asset', 'Контейнер .fpf, реестр, Handle, стриминг', 5,
    'container, registry, handle, streaming'],
  ['flux-scene', 'Сцены, partition, streaming, prefab, save', 5,
    'scene, partition, streaming, prefab, entity, serialize, save'],
  ['flux-physics', 'Физика f64, collision, KCC', 3,
    'collision, kcc, broadphase, narrowphase, solver, ccd, joints'],
  ['flux-anim', 'Анимации: FSM, sampling, blending, IK, skinning', 6,
    'components, systems, physics, network, render'],
  ['flux-ai', 'Navmesh, pathfinding, BT/HFSM, crowd', 6,
    'nav, path, behavior, crowd'],
  ['flux-script', 'Lua 5.4, ScriptVM, песочница', 6,
    'core, lua, api, registry'],
  ['flux-net', 'Relevance, репликация, транспорт', 5,
    'relevance, replication, ownership, transport, session, topology, persistence'],
  ['flux-audio', 'Сигнальный граф, микшер, 3D-звук', 6,
    'device, mixer, spatial, dsp, codec, streaming, events'],
  ['flux-ui', 'Immediate UI, текст, HUD', 6,
    'immediate, draw, text, input, widgets, localization, egui_bridge'],
  ['flux-editor', 'Редактор сцены: viewport, gizmo, undo, PIE', 7,
    'app, viewport, gizmo, selection, inspect, undo, browser, mode'],
]

const tools = [
  ['flux-assetc', 'Компилятор ассетов: KTX2, меши, Opus, .fpf', 5],
  ['flux-build', 'Оркестратор сборки: DAG, кэш, пакинг', 5],
]

async function exists(file) {
  try {
    await access(file)
    return true
  } catch {
    return false
  }
}

async function writeIfMissing(file, content) {
  if (await exists(file)) return false
  await mkdir(path.dirname(file), { recursive: true })
  await writeFile(file, content, 'utf8')
  return true
}

function manifest(name, description, kind) {
  const lines = [
    '[package]',
    `name = "${name}"`,
    'version.workspace = true',
    'edition.workspace = true',
    'rust-version.workspace = true',
    'license.workspace = true',
    'repository.workspace = true',
    'authors.workspace = true',
    `description = "${description}"`,
    'publish = false',
    '',
    '[lints]',
    'workspace = true',
    '',
  ]
  if (kind === 'bin') {
    lines.push('[dependencies]', '')
  }
  return lines.join('\n')
}

function readme(name, description) {
  return [
    `# ${name}`,
    '',
    description + '.',
    '',
    'Канон: [docs/canon.md](../../docs/canon.md). План: [docs/architecture.md](../../docs/architecture.md).',
    '',
  ].join('\n')
}

function libStub(name, description, stage, modules) {
  return [
    `//! ${description}.`,
    '//!',
    '//! Канон: `docs/canon.md`. План: `docs/architecture.md`.',
    '//!',
    `//! TODO(этап ${stage}): модули ${modules}.`,
    '',
    '#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]',
    '',
  ].join('\n')
}

let created = 0

for (const [name, description, stage, modules] of crates) {
  const dir = path.join(root, 'crates', name)
  if (await writeIfMissing(path.join(dir, 'Cargo.toml'), manifest(name, description, 'lib'))) created++
  if (await writeIfMissing(path.join(dir, 'README.md'), readme(name, description))) created++
  if (await writeIfMissing(path.join(dir, 'src', 'lib.rs'), libStub(name, description, stage, modules))) created++
}

for (const [name, description, stage] of tools) {
  const dir = path.join(root, 'tools', name)
  if (await writeIfMissing(path.join(dir, 'Cargo.toml'), manifest(name, description, 'bin'))) created++
  if (await writeIfMissing(path.join(dir, 'README.md'), readme(name, description))) created++
  if (
    await writeIfMissing(
      path.join(dir, 'src', 'main.rs'),
      [
        `//! ${description}.`,
        '//!',
        `//! TODO(этап ${stage}): реализация инструмента.`,
        '',
        'fn main() {',
        `    println!("${name}: не реализовано (этап ${stage})");`,
        '}',
        '',
      ].join('\n'),
    )
  ) {
    created++
  }
}

console.log(`Создано файлов: ${created}`)
