#!/usr/bin/env node
//! Загрузка вендорного DXC (ADR-0038) в `tools/dxc/`.
//!
//! По умолчанию ставится **последний** релиз DirectXShaderCompiler
//! (`releases/latest`); версию можно закрепить. Команда идемпотентна и
//! подходит для CI:
//!
//! ```text
//! node tools/dxc/fetch.mjs                  # последний релиз
//! node tools/dxc/fetch.mjs --version v1.9.2609
//! node tools/dxc/fetch.mjs --force          # перекачать
//! ```
//!
//! Целостность: SHA-256 из `digest` ассета GitHub; если GitHub его не
//! опубликовал, хэш считается локально и сохраняется в
//! `bin/.dxc-release.json`. Windows: zip → `bin/{dxc.exe,dxcompiler.dll,
//! dxil.dll}`; Linux: tar.gz → `bin/dxc-real` + libs и обёртка `bin/dxc`
//! с `LD_LIBRARY_PATH` (весь `bin/` уже в ignore).

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmod, copyFile, mkdir, mkdtemp, readFile, readdir, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const DXC_DIR = join(ROOT, "tools", "dxc");
const BIN_DIR = join(DXC_DIR, "bin");
const METADATA = join(BIN_DIR, ".dxc-release.json");
const API_ROOT = "https://api.github.com/repos/microsoft/DirectXShaderCompiler";

function usage() {
  return [
    "Использование: node tools/dxc/fetch.mjs [--version TAG] [--force]",
    "",
    "  --version TAG  закрепить релиз (например, v1.9.2609); по умолчанию — latest",
    "  --force        перекачать, даже если версия уже установлена",
    "",
    "GitHub API без токена ограничен 60 запросами/час; GITHUB_TOKEN поднимает лимит.",
  ].join("\n");
}

function parseArgs(argv) {
  const options = { force: false, help: false, tag: null };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--force") {
      options.force = true;
    } else if (argument === "--help" || argument === "-h") {
      options.help = true;
    } else if (argument === "--version") {
      const value = argv[index + 1];
      if (!value) {
        throw new Error("--version требует тег, например v1.9.2609");
      }
      options.tag = value.startsWith("v") ? value : `v${value}`;
      index += 1;
    } else {
      throw new Error(`неизвестный аргумент: ${argument}\n\n${usage()}`);
    }
  }
  return options;
}

/** Исполняемый файл и шаблон ассета для текущей платформы. */
function platform() {
  if (process.platform === "win32") {
    return { executable: "dxc.exe", asset: /^dxc_.*\.zip$/i };
  }
  if (process.platform === "linux") {
    return { executable: "dxc", asset: /^linux_dxc_.*\.(tar\.gz|tgz)$/i };
  }
  throw new Error(
    `платформа ${process.platform} не поддерживается: DXC релизы есть для Windows и Linux (Linux-бинарник запускается в WSL)`,
  );
}

/** Файлы, которые обязаны появиться после установки. */
function requiredTargets(executable) {
  const targets = [join(BIN_DIR, executable)];
  if (process.platform === "win32") {
    targets.push(join(BIN_DIR, "dxcompiler.dll"), join(BIN_DIR, "dxil.dll"));
  } else {
    targets.push(join(BIN_DIR, "libdxcompiler.so"), join(BIN_DIR, "libdxil.so"));
  }
  return targets;
}

async function exists(path) {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

async function allExist(paths) {
  return (await Promise.all(paths.map(exists))).every(Boolean);
}

async function githubJson(url) {
  const headers = {
    Accept: "application/vnd.github+json",
    "User-Agent": "fluxengine-dxc-fetch",
  };
  if (process.env.GITHUB_TOKEN) {
    headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  }
  const response = await fetch(url, { headers });
  if (!response.ok) {
    throw new Error(`GitHub API вернул HTTP ${response.status} для ${url}`);
  }
  return response.json();
}

/** Метаданные релиза: тег, ассет, URL и ожидаемый SHA-256 (если опубликован). */
async function resolveRelease(tag) {
  const url = tag
    ? `${API_ROOT}/releases/tags/${encodeURIComponent(tag)}`
    : `${API_ROOT}/releases/latest`;
  const release = await githubJson(url);
  const config = platform();
  const asset = (release.assets ?? []).find((candidate) => config.asset.test(candidate.name));
  if (!asset) {
    throw new Error(`в релизе ${release.tag_name} нет ассета для ${process.platform}`);
  }
  const digest =
    typeof asset.digest === "string" && /^sha256:[0-9a-f]{64}$/i.test(asset.digest)
      ? asset.digest.slice("sha256:".length).toUpperCase()
      : null;
  return { tag: release.tag_name, name: asset.name, url: asset.browser_download_url, sha256: digest };
}

async function readMetadata() {
  try {
    return JSON.parse(await readFile(METADATA, "utf8"));
  } catch {
    return null;
  }
}

async function writeMetadata(release, sha256) {
  const metadata = {
    tag: release.tag,
    asset: release.name,
    sha256,
    installedAt: new Date().toISOString(),
  };
  await writeFile(METADATA, `${JSON.stringify(metadata, null, 2)}\n`);
}

/** Скачивает архив, проверяет SHA-256 и возвращает фактический хэш. */
async function download(release, destination) {
  const response = await fetch(release.url);
  if (!response.ok) {
    throw new Error(`не удалось скачать ${release.url}: HTTP ${response.status}`);
  }
  const bytes = Buffer.from(await response.arrayBuffer());
  const sha256 = createHash("sha256").update(bytes).digest("hex").toUpperCase();
  if (release.sha256 && sha256 !== release.sha256) {
    throw new Error(
      `SHA-256 не совпал для ${release.name}:\n  ожидался ${release.sha256}\n  получен  ${sha256}`,
    );
  }
  if (!release.sha256) {
    console.warn(`SHA-256 не опубликован GitHub; локальный хэш ${sha256} сохранён в метаданных`);
  }
  await writeFile(destination, bytes);
  console.log(
    `скачано: ${release.name} (${(bytes.length / (1024 * 1024)).toFixed(1)} МиБ, ${release.tag})`,
  );
  return sha256;
}

function extract(archive, destination) {
  let result;
  if (process.platform === "win32") {
    const quoted = archive.replaceAll("'", "''");
    const target = destination.replaceAll("'", "''");
    result = spawnSync(
      "powershell",
      [
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        `Expand-Archive -Force -LiteralPath '${quoted}' -DestinationPath '${target}'`,
      ],
      { stdio: "inherit" },
    );
  } else {
    result = spawnSync("tar", ["-xzf", archive, "-C", destination], { stdio: "inherit" });
  }
  if (result.error) {
    throw new Error(`не удалось распаковать ${archive}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(`распаковка ${archive} завершилась с кодом ${result.status}`);
  }
}

/** Копирует один файл, создавая каталог назначения. */
async function install(source, target, mode) {
  if (!(await exists(source))) {
    return false;
  }
  await mkdir(dirname(target), { recursive: true });
  await copyFile(source, target);
  if (mode !== undefined) {
    await chmod(target, mode);
  }
  return true;
}

/** Копирует файлы из распакованного релиза в `tools/dxc/`. */
async function installFiles(extracted) {
  const installed = [];
  if (process.platform === "win32") {
    const source = join(extracted, "bin", "x64");
    for (const name of ["dxc.exe", "dxcompiler.dll", "dxil.dll", "dxv.exe"]) {
      if (await install(join(source, name), join(BIN_DIR, name))) {
        installed.push(`bin/${name}`);
      }
    }
  } else {
    if (await install(join(extracted, "bin", "dxc"), join(BIN_DIR, "dxc-real"), 0o755)) {
      installed.push("bin/dxc-real");
    }
    for (const name of ["libdxcompiler.so", "libdxil.so"]) {
      if (await install(join(extracted, "lib", name), join(BIN_DIR, name))) {
        installed.push(`bin/${name}`);
      }
    }
    // Обёртка: релизный dxc ищет библиотеки рядом/в ../lib; держим всё в bin/.
    const wrapper = [
      "#!/bin/sh",
      'DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)',
      'LD_LIBRARY_PATH="$DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" exec "$DIR/dxc-real" "$@"',
      "",
    ].join("\n");
    await writeFile(join(BIN_DIR, "dxc"), wrapper, { mode: 0o755 });
    installed.push("bin/dxc (wrapper)");
  }
  // Лицензии поставки: LICENSE-*.txt / LICENCE-*.txt / ThirdPartyNotices*.txt.
  // Кладём в bin/licenses/ — каталог bin/ уже в ignore, корень не засоряется.
  for (const entry of await readdir(extracted)) {
    if (/^(licen[cs]e.*|thirdparty.*)\.txt$/i.test(entry)) {
      if (await install(join(extracted, entry), join(BIN_DIR, "licenses", entry))) {
        installed.push(`bin/licenses/${entry}`);
      }
    }
  }
  return installed;
}

/** Возвращает строку `dxc --version`, подставляя каталог библиотек на Linux. */
function dxcVersion(config) {
  const executable = join(BIN_DIR, config.executable);
  const env = { ...process.env };
  if (process.platform !== "win32") {
    env.LD_LIBRARY_PATH = [BIN_DIR, env.LD_LIBRARY_PATH].filter(Boolean).join(":");
  }
  const result = spawnSync(executable, ["--version"], { encoding: "utf8", env });
  if (result.error) {
    throw new Error(`не удалось запустить ${executable}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(
      `dxc --version завершился с кодом ${result.status}: ${(result.stderr || result.stdout).trim()}`,
    );
  }
  return result.stdout.trim();
}

function verify(config) {
  const version = dxcVersion(config);
  console.log(`проверка: ${version.split("\n")[0]}`);
}

/**
 * Установлен ли уже запрошенный релиз.
 *
 * Источник истины — `bin/.dxc-release.json`; если его нет (старая установка),
 * версия сверяется с выводом `dxc --version` и метаданные создаются.
 */
async function installedMatches(release, config) {
  if (!(await allExist(requiredTargets(config.executable)))) {
    return false;
  }
  const metadata = await readMetadata();
  if (metadata?.tag === release.tag) {
    return true;
  }
  try {
    const version = dxcVersion(config);
    if (version.includes(release.tag.replace(/^v/, ""))) {
      await writeMetadata(release, metadata?.sha256 ?? "");
      return true;
    }
  } catch {
    // Повреждённая установка: перекачаем.
  }
  return false;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    console.log(usage());
    return;
  }

  const config = platform();
  let release;
  try {
    release = await resolveRelease(options.tag);
  } catch (error) {
    // Сеть/GitHub недоступны: если DXC уже стоит, работаем с ним.
    if (await allExist(requiredTargets(config.executable))) {
      console.warn(`не удалось получить релиз (${error.message}); использую установленный DXC`);
      verify(config);
      return;
    }
    throw error;
  }

  if (!options.force && (await installedMatches(release, config))) {
    console.log(`DXC ${release.tag} уже установлен: ${join(BIN_DIR, config.executable)}`);
    verify(config);
    return;
  }

  const staging = await mkdtemp(join(tmpdir(), "flux-dxc-"));
  try {
    const archive = join(staging, release.name);
    const sha256 = await download(release, archive);
    const extracted = join(staging, "unpacked");
    await mkdir(extracted, { recursive: true });
    extract(archive, extracted);
    const installed = await installFiles(extracted);
    if (installed.length === 0) {
      throw new Error("в архиве не найдены файлы DXC — проверьте раскладку релиза");
    }
    console.log(`установлено: ${installed.join(", ")}`);
    verify(config);
    await writeMetadata(release, sha256);
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(`fetch DXC: ${error.message}`);
  process.exitCode = 1;
});
