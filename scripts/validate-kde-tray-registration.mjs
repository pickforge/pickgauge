#!/usr/bin/env node

import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import {
  accessSync,
  constants,
  existsSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const appImagePath = resolve(
  repoRoot,
  "src-tauri/target/release/bundle/appimage/ForgeGauge_0.1.0_amd64.AppImage",
);
const itemTimeoutMs = 12_000;
const menuTimeoutMs = 5_000;
const configTimeoutMs = 5_000;
const rotationTimeoutMs = 18_000;
const stopTimeoutMs = 3_000;
const trayIconPngSignature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const trayServiceColors = {
  Codex: [37, 99, 235, 255],
  "Claude Code": [199, 91, 18, 255],
};

if (process.platform !== "linux") {
  console.log(`Skipping KDE tray registration smoke on ${process.platform}`);
  process.exit(0);
}

if (!commandAvailable("qdbus")) {
  console.log("Skipping KDE tray registration smoke because qdbus is unavailable");
  process.exit(0);
}

if (!commandAvailable("gdbus")) {
  console.log("Skipping KDE tray registration smoke because gdbus is unavailable");
  process.exit(0);
}

if (!commandAvailable("xdotool")) {
  console.log("Skipping KDE tray registration smoke because xdotool is unavailable");
  process.exit(0);
}

if (!commandAvailable("xprop")) {
  console.log("Skipping KDE tray registration smoke because xprop is unavailable");
  process.exit(0);
}

if (!commandAvailable("xmessage")) {
  console.log("Skipping KDE tray registration smoke because xmessage is unavailable");
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
let child = launchAppImage(isolatedRoot);

try {
  const item = await waitForForgeGaugeTrayItem(beforeItems, child);
  const menuItems = await waitForTrayMenuItems(item);
  const initialConfig = await waitForPersistedConfig(isolatedRoot);
  const window = await validateTrayWindowLifecycle(item, child, menuItems);
  const menu = await validateTrayMenuQuit(item, child, menuItems);
  const initialStdout = child.stdoutText();
  const initialStderr = child.stderrText();
  const gaugeRotation = await validateTrayGaugeRotation({
    configPath: initialConfig.path,
    isolatedRoot,
  });
  const settingsPersistence = await validateSettingsPersistence({
    configPath: initialConfig.path,
    isolatedRoot,
  });

  assertSanitizedProcessOutput(initialStdout, initialStderr);

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
      gaugeRotation,
      menu,
      settingsPersistence,
      window,
    },
    isolatedXdgDirs: true,
  };
  const serialized = `${JSON.stringify(result, null, 2)}\n`;

  assertNoHomePath(serialized);
  process.stdout.write(serialized);
} finally {
  await stopProcess(child);
  await removeTempDir(isolatedRoot);
}

function launchAppImage(isolatedRoot) {
  const launched = spawn(appImagePath, [], {
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

  launched.stdout.setEncoding("utf8");
  launched.stderr.setEncoding("utf8");
  launched.stdout.on("data", (chunk) => {
    stdout += chunk;
  });
  launched.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  launched.stdoutText = () => stdout;
  launched.stderrText = () => stderr;

  return launched;
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
    address: itemAddress,
    id: qdbusProperty(service, objectPath, "Id"),
    menuPath: `${objectPath}/Menu`,
    objectPath,
    service,
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

async function validateTrayGaugeRotation({ configPath, isolatedRoot }) {
  const deterministicConfig = readConfig(configPath);

  writeConfig(configPath, {
    ...deterministicConfig,
    enabledServices: {
      codex: true,
      claude: true,
    },
    providers: {
      ...deterministicConfig.providers,
      localEnabled: false,
      webEnabled: false,
    },
    intervals: {
      ...deterministicConfig.intervals,
      gaugeSwitchSeconds: 5,
    },
  });

  const beforeRestartItems = registeredStatusNotifierItems();

  child = launchAppImage(isolatedRoot);

  try {
    const item = await waitForForgeGaugeTrayItem(beforeRestartItems, child);
    const menuItems = await waitForTrayMenuItems(item);
    const observedServices = await waitForTrayIconServices(item);

    await validateTrayMenuQuit(item, child, menuItems);
    assertSanitizedProcessOutput(child.stdoutText(), child.stderrText());

    return {
      deterministicLocalProvidersDisabled: true,
      iconUpdatedOverDbus: true,
      observedServices,
      rotatesBetweenEnabledServices: true,
    };
  } finally {
    await stopProcess(child);
  }
}

async function waitForTrayIconServices(item) {
  const seenServices = new Set();
  const started = Date.now();

  while (Date.now() - started < rotationTimeoutMs) {
    const service = trayIconService(item);

    if (service) {
      seenServices.add(service);
    }

    if (seenServices.has("Codex") && seenServices.has("Claude Code")) {
      return ["Codex", "Claude Code"];
    }

    await delay(250);
  }

  throw new Error("Timed out waiting for tray icon gauge rotation");
}

function trayIconService(item) {
  try {
    const iconPath = qdbusProperty(item.service, item.objectPath, "IconName");

    if (!isTrayIconPath(iconPath)) {
      return null;
    }

    const rgba = decodePngRgba(readFileSync(iconPath));

    for (const [service, color] of Object.entries(trayServiceColors)) {
      if (rgbaContainsColor(rgba, color)) {
        return service;
      }
    }
  } catch {
  }

  return null;
}

function isTrayIconPath(path) {
  return /^\/run\/user\/\d+\/tray-icon\/tray-icon-main-\d+\.png$/u.test(path) && existsSync(path);
}

function rgbaContainsColor(rgba, [red, green, blue, alpha]) {
  for (let index = 0; index < rgba.length; index += 4) {
    if (
      rgba[index] === red &&
      rgba[index + 1] === green &&
      rgba[index + 2] === blue &&
      rgba[index + 3] === alpha
    ) {
      return true;
    }
  }

  return false;
}

function decodePngRgba(png) {
  assert.equal(png.subarray(0, 8).equals(trayIconPngSignature), true, "Tray icon must be a PNG");

  let offset = 8;
  let width = 0;
  let height = 0;
  let bitDepth = 0;
  let colorType = 0;
  const idatChunks = [];

  while (offset < png.length) {
    const length = png.readUInt32BE(offset);
    const type = png.subarray(offset + 4, offset + 8).toString("ascii");
    const data = png.subarray(offset + 8, offset + 8 + length);

    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
      assert.equal(data[10], 0, "Tray icon PNG must use deflate compression");
      assert.equal(data[11], 0, "Tray icon PNG must use standard filtering");
      assert.equal(data[12], 0, "Tray icon PNG must be non-interlaced");
    } else if (type === "IDAT") {
      idatChunks.push(data);
    } else if (type === "IEND") {
      break;
    }

    offset += 12 + length;
  }

  assert.equal(bitDepth, 8, "Tray icon PNG must use 8-bit channels");
  assert.equal(colorType, 6, "Tray icon PNG must use RGBA pixels");

  const bytesPerPixel = 4;
  const stride = width * bytesPerPixel;
  const inflated = inflateSync(Buffer.concat(idatChunks));
  const rgba = Buffer.alloc(width * height * bytesPerPixel);
  let sourceOffset = 0;
  let outputOffset = 0;
  let previousRow = Buffer.alloc(stride);

  for (let y = 0; y < height; y += 1) {
    const filter = inflated[sourceOffset];
    const rawRow = inflated.subarray(sourceOffset + 1, sourceOffset + 1 + stride);
    const row = Buffer.alloc(stride);

    for (let index = 0; index < stride; index += 1) {
      const left = index >= bytesPerPixel ? row[index - bytesPerPixel] : 0;
      const up = previousRow[index] ?? 0;
      const upLeft = index >= bytesPerPixel ? previousRow[index - bytesPerPixel] : 0;

      row[index] = (rawRow[index] + pngFilterValue(filter, left, up, upLeft)) & 0xff;
    }

    row.copy(rgba, outputOffset);
    previousRow = row;
    sourceOffset += stride + 1;
    outputOffset += stride;
  }

  return rgba;
}

function pngFilterValue(filter, left, up, upLeft) {
  if (filter === 0) {
    return 0;
  }

  if (filter === 1) {
    return left;
  }

  if (filter === 2) {
    return up;
  }

  if (filter === 3) {
    return Math.floor((left + up) / 2);
  }

  if (filter === 4) {
    return paethPredictor(left, up, upLeft);
  }

  throw new Error(`Unsupported PNG filter ${filter}`);
}

function paethPredictor(left, up, upLeft) {
  const estimate = left + up - upLeft;
  const leftDistance = Math.abs(estimate - left);
  const upDistance = Math.abs(estimate - up);
  const upLeftDistance = Math.abs(estimate - upLeft);

  if (leftDistance <= upDistance && leftDistance <= upLeftDistance) {
    return left;
  }

  if (upDistance <= upLeftDistance) {
    return up;
  }

  return upLeft;
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

async function validateTrayWindowLifecycle(item, child, menuItems) {
  const showItem = findMenuItem(menuItems, "Show ForgeGauge");

  assert.ok(showItem, "Tray menu must expose Show ForgeGauge");

  const visibleBeforeShow = visibleForgeGaugeWindowIds();

  assert.equal(
    visibleBeforeShow.length,
    0,
    "Isolated AppImage launch must start without a visible ForgeGauge window",
  );

  triggerTrayMenuItem(item, showItem);

  const firstWindowId = await waitForVisibleForgeGaugeWindow(visibleBeforeShow);
  const firstWindowTitle = xdotool(["getwindowname", firstWindowId]);
  const windowHints = validatePopupWindowHints(firstWindowId);

  assert.equal(firstWindowTitle, "ForgeGauge", "Show menu item must open ForgeGauge window");

  await validateFocusLossHidesPopup(firstWindowId);
  assert.equal(child.exitCode, null, "Focus loss must not exit ForgeGauge");
  assert.equal(child.signalCode, null, "Focus loss must not signal ForgeGauge");
  assert.equal(
    await waitForTrayItemRegistered(item.address),
    true,
    "Tray item must remain registered after focus loss",
  );

  const visibleBeforeCloseCheck = visibleForgeGaugeWindowIds();

  assert.equal(
    visibleBeforeCloseCheck.length,
    0,
    "Focus loss must leave no visible ForgeGauge window before close-check reshow",
  );

  triggerTrayMenuItem(item, showItem);

  const closeCheckWindowId = await waitForVisibleForgeGaugeWindow(visibleBeforeCloseCheck);
  const closeCheckWindowTitle = xdotool(["getwindowname", closeCheckWindowId]);

  assert.equal(
    closeCheckWindowTitle,
    "ForgeGauge",
    "Show menu item must reopen ForgeGauge window after focus loss",
  );

  xdotool(["windowclose", closeCheckWindowId]);

  assert.equal(
    await waitForWindowHidden(closeCheckWindowId),
    true,
    "Window close request must remove the visible ForgeGauge window",
  );
  assert.equal(child.exitCode, null, "Window close request must not exit ForgeGauge");
  assert.equal(child.signalCode, null, "Window close request must not signal ForgeGauge");
  assert.equal(
    await waitForTrayItemRegistered(item.address),
    true,
    "Tray item must remain registered after window close request",
  );

  const visibleBeforeReshow = visibleForgeGaugeWindowIds();

  assert.equal(
    visibleBeforeReshow.length,
    0,
    "Window close request must leave no visible ForgeGauge window before reshow",
  );

  triggerTrayMenuItem(item, showItem);

  const secondWindowId = await waitForVisibleForgeGaugeWindow(visibleBeforeReshow);
  const secondWindowTitle = xdotool(["getwindowname", secondWindowId]);

  assert.equal(secondWindowTitle, "ForgeGauge", "Show menu item must reopen ForgeGauge window");

  return {
    closeKeepsProcessRunning: true,
    closeKeepsTrayRegistered: true,
    focusLossHidesPopup: true,
    focusLossKeepsProcessRunning: true,
    focusLossKeepsTrayRegistered: true,
    initialVisibleWindowCount: visibleBeforeShow.length,
    reshowAfterClose: true,
    showMenuOpensWindow: true,
    skipTaskbarHint: windowHints.skipTaskbar,
    staysAboveHint: windowHints.staysAbove,
    windowTitle: secondWindowTitle,
  };
}

async function validateFocusLossHidesPopup(windowId) {
  const focusTarget = await launchFocusTarget();

  try {
    xdotool(["windowactivate", focusTarget.windowId]);

    assert.equal(
      await waitForWindowHidden(windowId),
      true,
      "Focus loss must hide the ForgeGauge popup",
    );
  } finally {
    await stopProcess(focusTarget.child);
  }
}

async function launchFocusTarget() {
  const targetTitle = "FocusSmokeTarget";
  const child = spawn(
    "xmessage",
    ["-name", targetTitle, "-title", targetTitle, "-buttons", "OK:0", "focus smoke"],
    {
      detached: true,
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  const windowId = await waitForVisibleWindowByName(`^${targetTitle}$`, []);

  return { child, windowId };
}

function validatePopupWindowHints(windowId) {
  const state = xprop(["-id", windowId, "_NET_WM_STATE"]);

  assert.match(
    state,
    /_NET_WM_STATE_SKIP_TASKBAR/u,
    "ForgeGauge popup must request skip-taskbar window state",
  );
  assert.match(
    state,
    /_NET_WM_STATE_(ABOVE|STAYS_ON_TOP)/u,
    "ForgeGauge popup must request above/stays-on-top window state",
  );

  return {
    skipTaskbar: true,
    staysAbove: true,
  };
}

async function validateTrayMenuQuit(item, child, menuItems) {
  const showItem = findMenuItem(menuItems, "Show ForgeGauge");
  const quitItem = findMenuItem(menuItems, "Quit");

  assert.ok(showItem, "Tray menu must expose Show ForgeGauge");
  assert.ok(quitItem, "Tray menu must expose Quit");

  triggerTrayMenuItem(item, quitItem);

  assert.equal(
    await waitForProcessExit(child, stopTimeoutMs),
    true,
    "Tray Quit menu item must terminate ForgeGauge",
  );
  assert.equal(child.exitCode, 0, "Tray Quit menu item must exit ForgeGauge successfully");
  assert.equal(
    await waitForTrayItemUnregistered(item.address),
    true,
    "Tray item must unregister after Quit",
  );

  return {
    quitExitsApp: true,
    quitItemLabel: quitItem.label,
    showItemLabel: showItem.label,
    trayItemUnregisteredAfterQuit: true,
  };
}

async function validateSettingsPersistence({ configPath, isolatedRoot }) {
  const firstConfig = readConfig(configPath);

  assert.equal(firstConfig.version, 4, "Default persisted config must use current schema version");
  assert.equal(firstConfig.enabledServices.codex, true, "Default config must enable Codex");
  assert.equal(firstConfig.enabledServices.claude, true, "Default config must enable Claude");

  const updatedConfig = {
    ...firstConfig,
    enabledServices: {
      ...firstConfig.enabledServices,
      codex: false,
      claude: true,
    },
    intervals: {
      ...firstConfig.intervals,
      gaugeSwitchSeconds: 5,
    },
  };

  writeConfig(configPath, updatedConfig);

  const beforeRestartItems = registeredStatusNotifierItems();

  child = launchAppImage(isolatedRoot);

  try {
    const item = await waitForForgeGaugeTrayItem(beforeRestartItems, child);
    const menuItems = await waitForTrayMenuItems(item);
    const restartedConfig = await waitForPersistedConfig(isolatedRoot);

    assert.deepEqual(
      restartedConfig.config.enabledServices,
      updatedConfig.enabledServices,
      "Restarted app must preserve persisted service toggles",
    );
    assert.equal(
      restartedConfig.config.intervals.gaugeSwitchSeconds,
      updatedConfig.intervals.gaugeSwitchSeconds,
      "Restarted app must preserve persisted gauge interval",
    );
    await validateTrayMenuQuit(item, child, menuItems);
    assertSanitizedProcessOutput(child.stdoutText(), child.stderrText());

    return {
      configCreatedOnFirstLaunch: true,
      persistedServiceTogglesPreservedAfterRestart: true,
      persistedGaugeIntervalPreservedAfterRestart: true,
      persistedConfigSurvivesPackagedRestart: true,
    };
  } finally {
    await stopProcess(child);
  }
}

async function waitForPersistedConfig(isolatedRoot) {
  const started = Date.now();

  while (Date.now() - started < configTimeoutMs) {
    const path = findConfigFile(resolve(isolatedRoot, "config"));

    if (path) {
      const config = readConfig(path);

      if (config?.enabledServices && config?.intervals) {
        return { config, path };
      }
    }

    await delay(100);
  }

  throw new Error("Timed out waiting for ForgeGauge to persist config");
}

function findConfigFile(root) {
  if (!existsSync(root)) {
    return null;
  }

  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = resolve(root, entry.name);

    if (entry.isDirectory()) {
      const found = findConfigFile(path);

      if (found) {
        return found;
      }
    } else if (entry.isFile() && entry.name === "config.json") {
      return path;
    }
  }

  return null;
}

function readConfig(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function writeConfig(path, config) {
  writeFileSync(path, `${JSON.stringify(config, null, 2)}\n`, { mode: 0o600 });
}

function triggerTrayMenuItem(item, menuItem) {
  gdbusCall([
    "--dest",
    item.service,
    "--object-path",
    item.menuPath,
    "--method",
    "com.canonical.dbusmenu.Event",
    String(menuItem.id),
    "clicked",
    "<''>",
    "0",
  ]);
}

async function waitForTrayMenuItems(item) {
  const started = Date.now();
  let menuItems = [];

  while (Date.now() - started < menuTimeoutMs) {
    menuItems = trayMenuItems(item);

    if (findMenuItem(menuItems, "Show ForgeGauge") && findMenuItem(menuItems, "Quit")) {
      return menuItems;
    }

    await delay(100);
  }

  return menuItems;
}

async function waitForTrayItemUnregistered(itemAddress) {
  const started = Date.now();

  while (Date.now() - started < stopTimeoutMs) {
    if (!registeredStatusNotifierItems().has(itemAddress)) {
      return true;
    }

    await delay(100);
  }

  return false;
}

async function waitForTrayItemRegistered(itemAddress) {
  const started = Date.now();

  while (Date.now() - started < stopTimeoutMs) {
    if (registeredStatusNotifierItems().has(itemAddress)) {
      return true;
    }

    await delay(100);
  }

  return false;
}

function trayMenuItems(item) {
  const items = [];

  for (let id = 0; id <= 10; id += 1) {
    const label = trayMenuItemLabel(item, id);

    if (label) {
      items.push({ id, label });
    }
  }

  return items;
}

function trayMenuItemLabel(item, id) {
  try {
    const output = gdbusCall([
      "--dest",
      item.service,
      "--object-path",
      item.menuPath,
      "--method",
      "com.canonical.dbusmenu.GetProperty",
      String(id),
      "label",
    ]);
    const match = output.match(/^\(<'([^']*)'>,\)$/u);

    return match?.[1] ?? null;
  } catch {
    return null;
  }
}

function findMenuItem(menuItems, label) {
  return menuItems.find((item) => item.label === label) ?? null;
}

function gdbusCall(args) {
  return execFileSync("gdbus", ["call", "--session", ...args], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

async function waitForVisibleForgeGaugeWindow(previousWindowIds) {
  return waitForVisibleWindowByName("^ForgeGauge$", previousWindowIds);
}

async function waitForVisibleWindowByName(name, previousWindowIds) {
  const started = Date.now();
  const previous = new Set(previousWindowIds);

  while (Date.now() - started < menuTimeoutMs) {
    const windowId = visibleWindowIdsByName(name).find((id) => !previous.has(id));

    if (windowId) {
      return windowId;
    }

    await delay(100);
  }

  throw new Error("Timed out waiting for visible ForgeGauge window");
}

async function waitForWindowHidden(windowId) {
  const started = Date.now();

  while (Date.now() - started < stopTimeoutMs) {
    if (!visibleForgeGaugeWindowIds().includes(windowId)) {
      return true;
    }

    await delay(100);
  }

  return false;
}

function visibleForgeGaugeWindowIds() {
  return visibleWindowIdsByName("^ForgeGauge$");
}

function visibleWindowIdsByName(name) {
  try {
    return xdotool(["search", "--onlyvisible", "--name", name])
      .split(/\r?\n/u)
      .map((item) => item.trim())
      .filter(Boolean);
  } catch {
    return [];
  }
}

function xdotool(args) {
  return execFileSync("xdotool", args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

function xprop(args) {
  return execFileSync("xprop", args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();
}

async function stopProcess(child) {
  const pid = child.pid;

  if (pid) {
    signalProcessGroup(pid, "SIGTERM");
  }

  if (child.exitCode === null && child.signalCode === null) {
    await waitForProcessExit(child, stopTimeoutMs);
  }

  if (pid) {
    signalProcessGroup(pid, "SIGKILL");
  }

  await waitForProcessExit(child, 1_000);
  await delay(250);
}

function signalProcessGroup(pid, signal) {
  try {
    process.kill(-pid, signal);
  } catch {
    try {
      process.kill(pid, signal);
    } catch {
    }
  }
}

async function removeTempDir(path) {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    rmSync(path, { force: true, recursive: true });

    if (!existsSync(path)) {
      return;
    }

    await delay(250);
  }

  assert.equal(existsSync(path), false, "Temporary smoke directory must be removed");
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
