#!/usr/bin/env node
//! Загрузка вендорного DXC (ADR-0038) в `tools/dxc/`.
//!
//! Команда идемпотентна и подходит для CI:
//!
//! ```text
//! node tools/dxc/fetch.mjs            # скачать, если ещё нет
//! node tools/dxc/fetch.mjs --force    # перекачать
//! ```
//!
//! Версия закреплена, архивы проверяются по SHA-256. Windows: zip →
//! `tools/dxc/bin/{dxc.exe,dxcompiler.dll,dxil.dll}`; Linux: tar.gz →
//! `tools/dxc/bin/dxc-real` + `libdxcompiler.so|libdxil.so` и генерируемая
//! обёртка `bin/dxc`, выставляющая `LD_LIBRARY_PATH` (весь каталог `bin/`
//! уже в ignore, отдельные правила не нужны).

import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmod, copyFile, mkdir, mkdtemp, readdir, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
const DXC_DIR = join(ROOT, "tools", "dxc");
const BIN_DIR = join(DXC_DIR, "bin");

/** Закреплённый релиз DXC: версия, ассеты и SHA-256 архивов. */
const RELEASE = {
  version: "v1.9.2609",
  windows: {
    asset: "dxc_2026_09_29.zip",
    sha256: "AD31B1FC8443175D204F77A611FDB3EF2EC42759BDC2F1167368DE24A4A7E7F1",
  },
  linux: {
    asset: "linux_dxc_2026_09_28.x86_x64.tar.gz",
    sha256: "96FAADC7F5C282D2FFDA49804BEB4C3EE38127BC252B723234E3C5CDF7AA39A1",
  },
};

function usage() {
  return [
    "Использование: node tools/dxc/fetch.mjs [--force]",
    "",
    "  --force   перекачать архивы, даже если DXC уже установлен",
    `  релиз     ${RELEASE.version} (Windows zip / Linux tar.gz)`,
  ].join("\n");
}

/** Конфигурация ассета для текущей платформы. */
function platformConfig() {
  if (process.platform === "win32") {
    return {
      ...RELEASE.windows,
      url: `https://github.com/microsoft/DirectXShaderCompiler/releases/download/${RELEASE.version}/${RELEASE.windows.asset}`,
      executable: "dxc.exe",
    };
  }
  if (process.platform === "linux") {
    return {
      ...RELEASE.linux,
      url: `https://github.com/microsoft/DirectXShaderCompiler/releases/download/${RELEASE.version}/${RELEASE.linux.asset}`,
      executable: "dxc",
    };
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

async function download(config, destination) {
  const response = await fetch(config.url);
  if (!response.ok) {
    throw new Error(`не удалось скачать ${config.url}: HTTP ${response.status}`);
  }
  const bytes = Buffer.from(await response.arrayBuffer());
  const hash = createHash("sha256").update(bytes).digest("hex").toUpperCase();
  if (hash !== config.sha256) {
    throw new Error(
      `SHA-256 не совпал для ${config.asset}:\n  ожидался ${config.sha256}\n  получен  ${hash}`,
    );
  }
  await writeFile(destination, bytes);
  console.log(`скачано: ${config.asset} (${(bytes.length / (1024 * 1024)).toFixed(1)} МиБ)`);
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

/** Запускает `dxc --version`, подставляя каталог библиотек на Linux. */
function verify(config) {
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
  console.log(`проверка: ${result.stdout.trim().split("\n")[0]}`);
}

async function main() {
  const args = process.argv.slice(2);
  if (args.includes("--help") || args.includes("-h")) {
    console.log(usage());
    return;
  }
  const force = args.includes("--force");
  const unknown = args.filter((argument) => argument !== "--force");
  if (unknown.length > 0) {
    throw new Error(`неизвестный аргумент: ${unknown[0]}\n\n${usage()}`);
  }

  const config = platformConfig();
  const targets = requiredTargets(config.executable);
  const ready = (await Promise.all(targets.map(exists))).every(Boolean);
  if (ready && !force) {
    console.log(`DXC ${RELEASE.version} уже установлен: ${targets[0]}`);
    verify(config);
    return;
  }

  const staging = await mkdtemp(join(tmpdir(), "flux-dxc-"));
  try {
    const archive = join(staging, config.asset);
    await download(config, archive);
    const extracted = join(staging, "unpacked");
    await mkdir(extracted, { recursive: true });
    extract(archive, extracted);
    const installed = await installFiles(extracted);
    if (installed.length === 0) {
      throw new Error("в архиве не найдены файлы DXC — проверьте раскладку релиза");
    }
    console.log(`установлено: ${installed.join(", ")}`);
    verify(config);
  } finally {
    await rm(staging, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(`fetch DXC: ${error.message}`);
  process.exitCode = 1;
});
