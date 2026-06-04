#!/usr/bin/env node

import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { accessSync, constants, existsSync, mkdtempSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const appImagePath = resolve(
  repoRoot,
  "src-tauri/target/release/bundle/appimage/ForgeGauge_0.1.0_amd64.AppImage",
);
const itemTimeoutMs = 12_000;
const stopTimeoutMs = 3_000;

if (process.platform !== "linux") {
  console.log(`Skipping KDE tray registration smoke on ${process.platform}`);
  process.exit(0);
}

if (!commandAvailable("qdbus")) {
  console.log("Skipping KDE tray registration smoke because qdbus is unavailable");
  process.exit(0);
}

if (!statusNotifierHostRegistered()) {
  console.log("Skipping KDE tray registration smoke because no StatusNotifier host is registered");
  process.exit(0);
}

assert.equal(existsSync(appImagePath), true, "ForgeGauge AppImage must exist");
assert.notEqual(statSync(appImagePath).mode & 0o111, 0, "ForgeGauge AppImage must be executable");

const beforeItems = registeredStatusNotifierItems();
const isolatedRoot = mkdtempSync(resolve(tmpdir(), "forgegauge-kde-tray-smoke-"));
const child = spawn(appImagePath, [], {
  detached: true,
  env: {
    ...process.env,
    XDG_CACHE_HOME: resolve(isolatedRoot, "cache"),
    XDG_CONFIG_HOME: resolve(isolatedRoot, "config"),
    XDG_DATA_HOME: resolve(isolatedRoot, "data"),
    XDG_STATE_HOME: resolve(isolatedRoot, "state"),
  },
  stdio: ["ignore", "pipe", "pipe"],
});
let stdout = "";
let stderr = "";

child.stdout.setEncoding("utf8");
child.stderr.setEncoding("utf8");
child.stdout.on("data", (chunk) => {
  stdout += chunk;
});
child.stderr.on("data", (chunk) => {
  stderr += chunk;
});

try {
  const item = await waitForForgeGaugeTrayItem(beforeItems, child);

  assertSanitizedProcessOutput(stdout, stderr);

  const result = {
    generatedAt: new Date().toISOString(),
    appImage: {
      executable: true,
      path: repoRelative(appImagePath),
    },
    desktopSession: {
      currentDesktop: safeEnv("XDG_CURRENT_DESKTOP"),
      xdgSessionType: safeEnv("XDG_SESSION_TYPE"),
    },
    statusNotifier: {
      hostRegistered: true,
      itemId: item.id,
      itemPath: item.objectPath,
      itemStatus: item.status,
      itemTitle: item.title,
    },
    isolatedXdgDirs: true,
  };
  const serialized = `${JSON.stringify(result, null, 2)}\n`;

  assertNoHomePath(serialized);
  process.stdout.write(serialized);
} finally {
  await stopProcess(child);
  rmSync(isolatedRoot, { force: true, recursive: true });
}

async function waitForForgeGaugeTrayItem(beforeItems, child) {
  const started = Date.now();

  while (Date.now() - started < itemTimeoutMs) {
    if (child.exitCode !== null || child.signalCode !== null) {
      throw new Error("ForgeGauge exited before registering a tray item");
    }

    for (const itemAddress of registeredStatusNotifierItems()) {
      if (beforeItems.has(itemAddress)) {
        continue;
      }

      const item = inspectStatusNotifierItem(itemAddress);

      if (isForgeGaugeItem(item)) {
        return item;
      }
    }

    await delay(250);
  }

  throw new Error("Timed out waiting for ForgeGauge StatusNotifier tray registration");
}

function registeredStatusNotifierItems() {
  return new Set(
    qdbus([
      "org.kde.StatusNotifierWatcher",
      "/StatusNotifierWatcher",
      "org.kde.StatusNotifierWatcher.RegisteredStatusNotifierItems",
    ])
      .split(/\r?\n/u)
      .map((item) => item.trim())
      .filter(Boolean),
  );
}

function inspectStatusNotifierItem(itemAddress) {
  const [service, ...objectPathParts] = itemAddress.split("/");
  const objectPath = `/${objectPathParts.join("/")}`;

  return {
    id: qdbusProperty(service, objectPath, "Id"),
    objectPath,
    status: qdbusProperty(service, objectPath, "Status"),
    title: qdbusProperty(service, objectPath, "Title"),
  };
}

function qdbusProperty(service, objectPath, property) {
  return qdbus([
    service,
    objectPath,
    "org.freedesktop.DBus.Properties.Get",
    "org.kde.StatusNotifierItem",
    property,
  ]);
}

function isForgeGaugeItem(item) {
  const haystack = `${item.id} ${item.objectPath} ${item.title}`.toLowerCase();

  return haystack.includes("forgegauge") || haystack.includes("tray app main");
}

function statusNotifierHostRegistered() {
  return (
    qdbus([
      "org.kde.StatusNotifierWatcher",
      "/StatusNotifierWatcher",
      "org.freedesktop.DBus.Properties.Get",
      "org.kde.StatusNotifierWatcher",
      "IsStatusNotifierHostRegistered",
    ]) === "true"
  );
}

function commandAvailable(command) {
  return (process.env.PATH ?? "").split(":").some((directory) => {
    try {
      accessSync(resolve(directory, command), constants.X_OK);
      return true;
    } catch {
      return false;
    }
  });
}

function qdbus(args) {
  return execFileSync("qdbus", args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  }).trim();
}

async function stopProcess(child) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return;
  }

  const pid = child.pid;

  if (pid) {
    try {
      process.kill(-pid, "SIGTERM");
    } catch {
      try {
        process.kill(pid, "SIGTERM");
      } catch {
      }
    }
  }

  if (await waitForProcessExit(child, stopTimeoutMs)) {
    return;
  }

  if (pid) {
    try {
      process.kill(-pid, "SIGKILL");
    } catch {
      try {
        process.kill(pid, "SIGKILL");
      } catch {
      }
    }
  }

  await waitForProcessExit(child, 1_000);
}

function waitForProcessExit(child, milliseconds) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return Promise.resolve(true);
  }

  return new Promise((resolveExit) => {
    const timeout = setTimeout(() => resolveExit(false), milliseconds);
    child.once("exit", () => {
      clearTimeout(timeout);
      resolveExit(true);
    });
  });
}

function assertSanitizedProcessOutput(...outputs) {
  const combined = outputs.join("\n");

  assertNoHomePath(combined);
  assert.equal(/cookie|token|authorization|bearer|<html|<!doctype/iu.test(combined), false);
}

function assertNoHomePath(output) {
  const homePath = process.env.HOME;

  if (homePath) {
    assert.equal(output.includes(homePath), false, "Output must not include the home directory path");
  }
}

function safeEnv(name) {
  const value = process.env[name];

  if (!value) {
    return null;
  }

  if (!/^[A-Za-z0-9_ .:+-]{1,80}$/u.test(value)) {
    return "<redacted>";
  }

  return value;
}

function repoRelative(path) {
  return relative(repoRoot, path).replaceAll("\\", "/");
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}
