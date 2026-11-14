#!/usr/bin/env node
// Приёмка состава workspace по именованному перечню (ADR-0028).
//
// Числовой критерий («18 пакетов») ломается при каждом легитимном добавлении.
// Здесь перечень — источник истины: 16 крейтов движка + 3 инструмента +
// служебный flux-macros. Ни одного лишнего пакета.
//
// Запуск: node tools/ci/check-workspace-members.mjs

import { execFileSync } from "node:child_process";

const EXPECTED = [
  // 16 крейтов движка
  "flux-ai",
  "flux-anim",
  "flux-asset",
  "flux-audio",
  "flux-core",
  "flux-ecs",
  "flux-editor",
  "flux-engine",
  "flux-graphics",
  "flux-math",
  "flux-net",
  "flux-physics",
  "flux-platform",
  "flux-scene",
  "flux-script",
  "flux-ui",
  // 3 инструмента
  "flux-assetc",
  "flux-build",
  "flux-release",
  // служебный proc-macro пакет (не единица канона)
  "flux-macros",
].sort();

function main() {
  let metadata;
  try {
    const output = execFileSync(
      "cargo",
      ["metadata", "--format-version", "1", "--no-deps"],
      { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] },
    );
    metadata = JSON.parse(output);
  } catch (error) {
    console.error("check-workspace-members: failed to read cargo metadata");
    console.error(error.message);
    process.exit(1);
  }

  const actual = metadata.packages.map((pkg) => pkg.name).sort();
  const missing = EXPECTED.filter((name) => !actual.includes(name));
  const extra = actual.filter((name) => !EXPECTED.includes(name));

  if (missing.length > 0 || extra.length > 0) {
    console.error("Workspace members do not match the canon list (ADR-0028)");
    if (missing.length > 0) {
      console.error(`  missing: ${missing.join(", ")}`);
    }
    if (extra.length > 0) {
      console.error(`  unexpected: ${extra.join(", ")}`);
    }
    console.error(
      "Update the canon (docs/canon.md) and this list together — a new package needs an ADR.",
    );
    process.exit(1);
  }

  console.log(
    `check-workspace-members: ok (${actual.length} packages: 16 engine + 3 tools + flux-macros)`,
  );
}

main();
