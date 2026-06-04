#!/usr/bin/env node

import { execFileSync, spawn } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  lstatSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const targetTriple = execFileSync("rustc", ["--print", "host-tuple"], {
  encoding: "utf8",
}).trim();
const sidecarPath = resolve(
  repoRoot,
  "src-tauri/binaries",
  `forgegauge-playwright-sidecar-${targetTriple}`,
);
const launchArgs = [
  "--disable-save-password-bubble",
  "--disable-password-manager-reauthentication",
  "--disable-features=AutofillServerCommunication",
  "--no-first-run",
];
const disabledStoragePreferences = {
  autofill: {
    credit_card_enabled: false,
    enabled: false,
    profile_enabled: false,
  },
  credentials_enable_autosignin: false,
  credentials_enable_service: false,
  profile: {
    password_manager_allow_show_passwords: false,
    password_manager_enabled: false,
  },
};
const defaultProfileStoreNames = ["Cookies", "Login Data", "Web Data", "Preferences"];
const sensitiveOutputPatterns = [
  {
    label: "auth or cookie material",
    pattern:
      /\b(set-cookie|cookie:|authorization:|bearer\s+[A-Za-z0-9._~+/-]+=*|access[_-]?token|refresh[_-]?token|session[_-]?(id|token)|csrf)\b/iu,
  },
  {
    label: "raw page markup",
    pattern: /<!doctype|<html|<body|<script/iu,
  },
];
const validationRoot = mkdtempSync(resolve(tmpdir(), "forgegauge-sidecar-profiles-"));

try {
  const profileRoots = new Map();
  const defaultProfileFixtures = prepareDefaultBrowserProfileFixtures(
    resolve(validationRoot, "fake-home"),
  );

  for (const request of [
    {
      service: "codex",
      url: "https://chatgpt.com/codex/cloud/settings/analytics",
      profileLabel: "codex-profile",
      profileRoot: resolve(validationRoot, "codex"),
      defaultProfileFixtures,
    },
    {
      service: "claude",
      url: "https://claude.ai/new#settings/usage",
      profileLabel: "claude-profile",
      profileRoot: resolve(validationRoot, "claude"),
      defaultProfileFixtures,
    },
  ]) {
    profileRoots.set(request.service, request.profileRoot);
    await validateLaunch(request);
  }

  if (profileRoots.get("codex") === profileRoots.get("claude")) {
    throw new Error("Codex and Claude sidecar validation profiles must be distinct");
  }

  console.log("Playwright sidecar kept Codex and Claude validation profiles isolated");
} finally {
  rmSync(validationRoot, { force: true, recursive: true });
}

async function validateLaunch({ service, url, profileLabel, profileRoot, defaultProfileFixtures }) {
  assertNonDefaultProfile(profileRoot);
  prepareDisabledStoragePreferences(profileRoot);
  await runLaunch({ service, url, profileLabel, profileRoot, defaultProfileFixtures });
  verifyDisabledStoragePreferences(profileRoot, service, defaultProfileFixtures.defaultRoots);
  verifyNoDefaultProfileImport(profileRoot, service, defaultProfileFixtures);

  const sentinelPath = resolve(profileRoot, "forgegauge-profile-sentinel.txt");
  writeFileSync(sentinelPath, `${service}\n`);

  await runLaunch({ service, url, profileLabel, profileRoot, defaultProfileFixtures });
  verifyDisabledStoragePreferences(profileRoot, service, defaultProfileFixtures.defaultRoots);
  verifyNoDefaultProfileImport(profileRoot, service, defaultProfileFixtures);

  if (!existsSync(sentinelPath)) {
    throw new Error(`${service} sidecar profile did not persist across relaunch`);
  }

  console.log(
    `Playwright sidecar persisted ${service} isolated profile with disabled storage preferences, no default profile import, and sanitized output`,
  );
}

async function runLaunch({ service, url, profileLabel, profileRoot, defaultProfileFixtures }) {
  const child = spawn(sidecarPath, [], {
    detached: true,
    env: sidecarLaunchEnvironment(defaultProfileFixtures.fakeHome),
    stdio: ["pipe", "pipe", "pipe"],
  });
  const timeout = setTimeout(() => {
    stopProcessGroup(child.pid);
  }, 30_000);
  let stdout = "";
  let stderr = "";

  try {
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });

    child.stdin.write(
      `${JSON.stringify({
        protocolVersion: 1,
        action: "launchLogin",
        backend: "playwright-headed-chromium-sidecar",
        service,
        url,
        profileLabel,
        userDataDir: profileRoot,
        headless: false,
        args: launchArgs,
      })}\n`,
    );
    child.stdin.end();

    const response = await waitForLaunchResponse(() => stdout);

    if (
      response.ok !== true ||
      response.status !== "launched" ||
      response.service !== service ||
      response.profileLabel !== profileLabel ||
      response.argCount !== launchArgs.length
    ) {
      throw new Error(`Unexpected ${service} launch response: ${JSON.stringify(response)}`);
    }

    verifySanitizedSidecarOutput({
      defaultProfileFixtures,
      profileRoot,
      service,
      stderr,
      stdout,
      url,
    });

    if (!existsSync(profileRoot)) {
      throw new Error(`${service} sidecar did not create the requested profile directory`);
    }

    console.log(`Playwright sidecar launched ${service} with an isolated temporary profile`);
  } finally {
    clearTimeout(timeout);
    stopProcessGroup(child.pid);
    await waitForExit(child);
  }
}

async function waitForLaunchResponse(readStdout) {
  const started = Date.now();

  while (Date.now() - started < 30_000) {
    const line = readStdout()
      .split(/\r?\n/u)
      .find((candidate) => candidate.trim().length > 0);

    if (line) {
      return JSON.parse(line);
    }

    await new Promise((resolveDelay) => setTimeout(resolveDelay, 100));
  }

  throw new Error("Timed out waiting for Playwright sidecar launch response");
}

async function waitForExit(child) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return;
  }

  await new Promise((resolveExit) => {
    const timeout = setTimeout(resolveExit, 3_000);
    child.once("exit", () => {
      clearTimeout(timeout);
      resolveExit();
    });
  });
}

function stopProcessGroup(pid) {
  if (!pid) {
    return;
  }

  try {
    process.kill(-pid, "SIGTERM");
  } catch {
  }
}

function assertNonDefaultProfile(profileRoot) {
  for (const defaultRoot of defaultBrowserProfileRoots()) {
    if (profileRoot === defaultRoot || profileRoot.startsWith(`${defaultRoot}/`)) {
      throw new Error("Sidecar validation profile must not use a default browser profile");
    }
  }
}

function prepareDisabledStoragePreferences(profileRoot) {
  const defaultProfileDir = resolve(profileRoot, "Default");
  mkdirSync(defaultProfileDir, { recursive: true, mode: 0o700 });
  writeFileSync(
    resolve(defaultProfileDir, "Preferences"),
    `${JSON.stringify(disabledStoragePreferences, null, 2)}\n`,
    { mode: 0o600 },
  );
}

function verifyDisabledStoragePreferences(profileRoot, service, extraDefaultRoots = []) {
  const preferencesPath = resolve(profileRoot, "Default", "Preferences");
  const preferences = JSON.parse(readFileSync(preferencesPath, "utf8"));

  for (const path of [
    ["credentials_enable_service"],
    ["credentials_enable_autosignin"],
    ["profile", "password_manager_enabled"],
    ["profile", "password_manager_allow_show_passwords"],
    ["autofill", "profile_enabled"],
    ["autofill", "credit_card_enabled"],
  ]) {
    if (preferenceAtPath(preferences, path) !== false) {
      throw new Error(`${service} sidecar enabled ${path.join(".")} after launch`);
    }
  }

  for (const path of [["autofill", "enabled"]]) {
    if (preferenceAtPath(preferences, path) === true) {
      throw new Error(`${service} sidecar enabled ${path.join(".")} after launch`);
    }
  }

  const serialized = JSON.stringify(preferences);
  for (const defaultRoot of [...defaultBrowserProfileRoots(), ...extraDefaultRoots]) {
    if (serialized.includes(defaultRoot)) {
      throw new Error(`${service} sidecar preferences reference a default browser profile`);
    }
  }
}

function preferenceAtPath(preferences, path) {
  return path.reduce((value, key) => value?.[key], preferences);
}

function defaultBrowserProfileRoots() {
  return defaultBrowserProfileRootsForHome(
    homedir(),
    process.env.XDG_CONFIG_HOME || resolve(homedir(), ".config"),
  );
}

function defaultBrowserProfileRootsForHome(home, xdgConfigHome = resolve(home, ".config")) {
  return [
    resolve(xdgConfigHome, "google-chrome"),
    resolve(xdgConfigHome, "chromium"),
  ];
}

function prepareDefaultBrowserProfileFixtures(fakeHome) {
  const fakeConfigHome = resolve(fakeHome, ".config");
  const defaultRoots = defaultBrowserProfileRootsForHome(fakeHome, fakeConfigHome);
  const sentinels = [];
  const sentinelFileNames = [];

  for (const [rootIndex, defaultRoot] of defaultRoots.entries()) {
    const defaultProfileDir = resolve(defaultRoot, "Default");
    mkdirSync(defaultProfileDir, { recursive: true, mode: 0o700 });

    for (const storeName of defaultProfileStoreNames) {
      const sentinel = `forgegauge-default-profile-sentinel-${rootIndex}-${storeName}`;
      writeFileSync(resolve(defaultProfileDir, storeName), `${sentinel}\n`, { mode: 0o600 });
      sentinels.push(sentinel);
    }

    const sentinelFileName = `forgegauge-default-profile-sentinel-${rootIndex}.txt`;
    writeFileSync(resolve(defaultProfileDir, sentinelFileName), `${sentinelFileName}\n`, {
      mode: 0o600,
    });
    sentinelFileNames.push(sentinelFileName);
  }

  return {
    defaultRoots,
    fakeHome,
    sentinelFileNames,
    sentinels,
  };
}

function sidecarLaunchEnvironment(fakeHome) {
  const env = {
    ...process.env,
    HOME: fakeHome,
    XDG_CACHE_HOME: resolve(fakeHome, ".cache"),
    XDG_CONFIG_HOME: resolve(fakeHome, ".config"),
  };

  if (!env.PLAYWRIGHT_BROWSERS_PATH) {
    env.PLAYWRIGHT_BROWSERS_PATH = resolve(homedir(), ".cache/ms-playwright");
  }

  return env;
}

function verifyNoDefaultProfileImport(profileRoot, service, defaultProfileFixtures) {
  for (const sentinelFileName of defaultProfileFixtures.sentinelFileNames) {
    if (profileContainsFileName(profileRoot, sentinelFileName)) {
      throw new Error(`${service} sidecar imported a default browser profile file`);
    }
  }

  for (const storeName of defaultProfileStoreNames) {
    const storePath = resolve(profileRoot, "Default", storeName);
    if (fileContainsAnySentinel(storePath, defaultProfileFixtures.sentinels)) {
      throw new Error(`${service} sidecar imported default browser ${storeName}`);
    }
  }
}

function verifySanitizedSidecarOutput({
  defaultProfileFixtures,
  profileRoot,
  service,
  stderr,
  stdout,
  url,
}) {
  const output = `${stdout}\n${stderr}`;
  const sensitiveFragments = [
    profileRoot,
    url,
    ...launchArgs,
    defaultProfileFixtures.fakeHome,
    ...defaultProfileFixtures.defaultRoots,
    ...defaultProfileFixtures.sentinels,
    ...defaultProfileFixtures.sentinelFileNames,
  ].filter((fragment) => fragment.length > 0);

  for (const fragment of sensitiveFragments) {
    if (output.includes(fragment)) {
      throw new Error(`${service} sidecar output leaked sensitive launch data`);
    }
  }

  for (const { label, pattern } of sensitiveOutputPatterns) {
    if (pattern.test(output)) {
      throw new Error(`${service} sidecar output leaked ${label}`);
    }
  }
}

function profileContainsFileName(root, fileName) {
  if (!existsSync(root)) {
    return false;
  }

  for (const entry of readdirSync(root)) {
    const entryPath = resolve(root, entry);
    const stat = lstatSync(entryPath);

    if (entry === fileName) {
      return true;
    }

    if (stat.isDirectory() && profileContainsFileName(entryPath, fileName)) {
      return true;
    }
  }

  return false;
}

function fileContainsAnySentinel(filePath, sentinels) {
  if (!existsSync(filePath)) {
    return false;
  }

  const content = readFileSync(filePath);
  return sentinels.some((sentinel) => content.includes(sentinel));
}
