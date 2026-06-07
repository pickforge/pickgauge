#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourcePath = resolve(repoRoot, "sidecars/playwright/pickgauge-playwright-sidecar.mjs");
const targetTriple =
  process.env.TAURI_ENV_TARGET_TRIPLE ||
  process.env.TARGET_TRIPLE ||
  execFileSync("rustc", ["--print", "host-tuple"], { encoding: "utf8" }).trim();
const checkOnly = process.argv.includes("--check");

if (!targetTriple.includes("linux")) {
  console.log(`Skipping Playwright sidecar preparation for unsupported target ${targetTriple}`);
  process.exit(0);
}

const targetPath = resolve(
  repoRoot,
  "src-tauri/binaries",
  `pickgauge-playwright-sidecar-${targetTriple}`,
);
const source = readFileSync(sourcePath, "utf8");

if (!source.startsWith("#!/usr/bin/env node")) {
  throw new Error("Playwright sidecar source must keep its Node shebang");
}

if (checkOnly) {
  const target = existsSync(targetPath) ? readFileSync(targetPath, "utf8") : "";

  if (target !== source) {
    throw new Error(`Generated Playwright sidecar is out of date: ${targetPath}`);
  }

  console.log(`Playwright sidecar is current for ${targetTriple}`);
  process.exit(0);
}

mkdirSync(dirname(targetPath), { recursive: true });
writeFileSync(targetPath, source);
chmodSync(targetPath, 0o755);
console.log(`Prepared Playwright sidecar: ${targetPath}`);
