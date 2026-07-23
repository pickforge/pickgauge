#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(fileURLToPath(new URL("..", import.meta.url)));
const args = process.argv.slice(2);
const appImageIndex = args.indexOf("--appimage");
if (appImageIndex === -1 || !args[appImageIndex + 1]) {
  console.error("Usage: node scripts/validate-headless-appimage.mjs --appimage <path>");
  process.exit(2);
}

const appImage = resolve(args[appImageIndex + 1]);
const packageVersion = JSON.parse(readFileSync(join(repoRoot, "package.json"), "utf8")).version;
const home = mkdtempSync(join(tmpdir(), "pickgauge-appimage-headless-"));

function run(commandArgs) {
  const env = {
    ...process.env,
    APPIMAGE_EXTRACT_AND_RUN: "1",
    HOME: home,
    XDG_CONFIG_HOME: join(home, "config"),
    XDG_DATA_HOME: join(home, "data"),
    XDG_CACHE_HOME: join(home, "cache"),
  };
  delete env.DISPLAY;
  delete env.WAYLAND_DISPLAY;
  delete env.XAUTHORITY;

  const result = spawnSync(appImage, commandArgs, {
    encoding: "utf8",
    env,
    timeout: 60_000,
  });
  const label = commandArgs.join(" ");
  if (result.error) throw new Error(`${label} could not start: ${result.error.message}`);
  if (result.signal) throw new Error(`${label} ended with signal ${result.signal}`);
  if (result.status !== 0) {
    throw new Error(`${label} exited ${result.status}; stderr: ${JSON.stringify(result.stderr)}`);
  }
  if (result.stderr !== "") {
    throw new Error(`${label} wrote stderr: ${JSON.stringify(result.stderr)}`);
  }
  return result.stdout;
}

try {
  const version = run(["--version"]);
  const expectedVersion = `pickgauge ${packageVersion}\n`;
  if (version !== expectedVersion) {
    throw new Error(`--version output mismatch: expected ${JSON.stringify(expectedVersion)}, got ${JSON.stringify(version)}`);
  }

  const humanUsage = run(["usage"]);
  const humanHeader = humanUsage.split(/\r?\n/, 1)[0].trim().split(/\s+/);
  const expectedHeader = ["Service", "Plan", "5h", "Week", "Resets", "Source", "Staleness"];
  if (humanHeader.length !== expectedHeader.length || humanHeader.some((column, index) => column !== expectedHeader[index])) {
    throw new Error(
      `usage table header mismatch: expected ${JSON.stringify(expectedHeader)}, got ${JSON.stringify(humanHeader)}`,
    );
  }

  const usageOutput = run(["usage", "--json"]);
  let usage;
  try {
    usage = JSON.parse(usageOutput);
  } catch (error) {
    throw new Error(`usage --json returned malformed JSON: ${error.message}`);
  }
  if (usage?.version !== 1) {
    throw new Error(`usage --json schema must be v1, got ${JSON.stringify(usage?.version)}`);
  }
  if (!Array.isArray(usage.services)) {
    throw new Error("usage --json services must be an array");
  }

  console.log(`Validated headless AppImage: ${appImage}`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
} finally {
  rmSync(home, { recursive: true, force: true });
}
