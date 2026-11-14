#!/usr/bin/env node
// Headless-правило (Roadmap WP 1.2): lib-крейты не тянут оконные и GPU-зависимости.
//
// winit/gilrs разрешены только платформенному слою, ash — только графическому,
// редактору — как приложению. dev- и build-зависимости примерами не считаются:
// окно в примере не делает крейт оконным.
//
// Запуск: node tools/ci/check-headless-deps.mjs

import { execFileSync } from "node:child_process";

const FORBIDDEN = ["winit", "ash", "ash-window", "gilrs"];

// Симуляция не должна зависеть от платформы: raw-события не входят в тик,
// единственный вход — Intention (ADR-0031/ADR-0033).
const SIMULATION = new Set(["flux-core", "flux-math", "flux-ecs", "flux-physics"]);

const ALLOWED = new Map([
  ["flux-platform", new Set(["winit", "gilrs"])],
  ["flux-graphics", new Set(["ash", "ash-window"])],
  ["flux-editor", new Set(FORBIDDEN)],
]);

function cargoMetadata() {
  const output = execFileSync(
    "cargo",
    ["metadata", "--format-version", "1", "--no-deps"],
    { encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] },
  );
  return JSON.parse(output);
}

function main() {
  let metadata;
  try {
    metadata = cargoMetadata();
  } catch (error) {
    console.error("check-headless-deps: failed to read cargo metadata");
    console.error(error.message);
    process.exit(1);
  }

  const violations = [];

  for (const pkg of metadata.packages) {
    if (SIMULATION.has(pkg.name)) {
      for (const dep of pkg.dependencies) {
        if (dep.kind === null && dep.name === "flux-platform") {
          violations.push(`${pkg.name} -> flux-platform (raw input must not reach simulation)`);
        }
      }
    }

    const allowed = ALLOWED.get(pkg.name) ?? new Set();
    for (const dep of pkg.dependencies) {
      // kind: null — normal, "dev" — dev, "build" — build.
      if (dep.kind !== null) {
        continue;
      }
      if (!FORBIDDEN.includes(dep.name)) {
        continue;
      }
      if (!allowed.has(dep.name)) {
        violations.push(`${pkg.name} -> ${dep.name}`);
      }
    }
  }

  if (violations.length > 0) {
    console.error(
      "Headless-rule violated: lib crates must not depend on windowing/GPU crates, simulation must not depend on flux-platform",
    );
    for (const violation of violations) {
      console.error(`  ${violation}`);
    }
    process.exit(1);
  }

  console.log(
    `check-headless-deps: ok (${metadata.packages.length} packages, forbidden: ${FORBIDDEN.join(", ")}, simulation: ${[...SIMULATION].join(", ")})`,
  );
}

main();
