#!/usr/bin/env node

import { execFileSync, spawnSync } from "node:child_process";
import { statSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform !== "linux") {
  console.log(`Skipping Linux Playwright sidecar package validation on ${process.platform}`);
  process.exit(0);
}

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const targetTriple = execFileSync("rustc", ["--print", "host-tuple"], {
  encoding: "utf8",
}).trim();
const sidecarPath = resolve(
  repoRoot,
  "src-tauri/binaries",
  `pickgauge-playwright-sidecar-${targetTriple}`,
);
const mode = statSync(sidecarPath).mode;

if ((mode & 0o111) === 0) {
  throw new Error(`Generated Playwright sidecar is not executable: ${sidecarPath}`);
}

const request = {
  protocolVersion: 1,
  action: "launchLogin",
  backend: "playwright-headed-chromium-sidecar",
  service: "codex",
  url: "https://chatgpt.com/codex/cloud/settings/analytics",
  profileLabel: "codex-profile",
  userDataDir: "/tmp/pickgauge-sidecar-validation/codex",
  headless: false,
  args: [
    "--disable-save-password-bubble",
    "--disable-password-manager-reauthentication",
    "--disable-features=AutofillServerCommunication",
    "--no-first-run",
  ],
};
const result = spawnSync(sidecarPath, ["--dry-run"], {
  input: `${JSON.stringify(request)}\n`,
  encoding: "utf8",
});

if (result.status !== 0) {
  throw new Error("Generated Playwright sidecar dry-run failed");
}

const response = JSON.parse(result.stdout.trim());

if (
  response.ok !== true ||
  response.status !== "accepted" ||
  response.backend !== request.backend ||
  response.service !== request.service ||
  response.profileLabel !== request.profileLabel ||
  response.argCount !== request.args.length
) {
  throw new Error("Generated Playwright sidecar returned an unexpected dry-run response");
}

if (result.stdout.includes(request.userDataDir) || result.stdout.includes(request.args[0])) {
  throw new Error("Generated Playwright sidecar dry-run leaked launch input");
}

console.log(`Generated Playwright sidecar dry-run passed for ${targetTriple}`);
