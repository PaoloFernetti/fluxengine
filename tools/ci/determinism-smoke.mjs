#!/usr/bin/env node
// Same-binary детерминизм (ADR-0033): 10k тиков, два прогона, один хеш.
//
// Запуск: node tools/ci/determinism-smoke.mjs

import { execFileSync } from "node:child_process";

try {
  execFileSync(
    "cargo",
    ["test", "-p", "flux-engine", "--test", "determinism", "--", "--nocapture"],
    { stdio: "inherit" },
  );
} catch (error) {
  console.error("determinism-smoke: failed");
  process.exit(error.status ?? 1);
}

console.log("determinism-smoke: ok (same-binary, 10k ticks)");
