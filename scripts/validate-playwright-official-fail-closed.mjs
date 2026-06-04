#!/usr/bin/env node

import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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
const launchTimeoutMs = 30_000;
const stopTimeoutMs = 3_000;
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
const validationRoot = mkdtempSync(resolve(tmpdir(), "forgegauge-official-fail-closed-"));
const validFailClosedStates = new Set([
  "logged_out",
  "mfa_required",
  "captcha_or_bot_check",
  "network_unavailable",
  "timed_out",
  "unexpected_ui",
]);
const officialUsageRequests = [
  {
    service: "codex",
    url: "https://chatgpt.com/codex/cloud/settings/analytics",
    profileLabel: "codex-profile",
  },
  {
    service: "claude",
    url: "https://claude.ai/new#settings/usage",
    profileLabel: "claude-profile",
  },
];
const sensitiveOutputPattern =
  /\b(set-cookie|cookie:|authorization:|bearer\s+[A-Za-z0-9._~+/-]+=*|session[_-]?token)\b/iu;
let validationRootCleanupFailed = false;

try {
  const results = [];

  for (const request of officialUsageRequests) {
    const profileRoot = resolve(validationRoot, request.service);

    prepareDisabledStoragePreferences(profileRoot);
    results.push(await validateHeadlessRefresh({ ...request, profileRoot }));
  }

  for (const request of officialUsageRequests) {
    const profileRoot = resolve(validationRoot, `${request.service}-network-unavailable`);

    prepareDisabledStoragePreferences(profileRoot);
    results.push(
      await validateHeadlessRefresh({
        ...request,
        args: [...launchArgs, "--proxy-server=http://127.0.0.1:9"],
        expectedPageState: "network_unavailable",
        profileRoot,
        scenario: "network_unavailable",
      }),
    );
  }

  const report = JSON.stringify(
    {
      generatedAt: new Date().toISOString(),
      backend: "playwright-headed-chromium-sidecar",
      desktopSession: {
        currentDesktop: safeEnv("XDG_CURRENT_DESKTOP"),
        xdgSessionType: safeEnv("XDG_SESSION_TYPE"),
      },
      os: osReleaseSummary(),
      targetTriple,
      services: results,
    },
    null,
    2,
  );

  verifySanitizedReport(report);
  console.log(report);
} finally {
  rmSync(validationRoot, { force: true, recursive: true });
  validationRootCleanupFailed = existsSync(validationRoot);
}

if (validationRootCleanupFailed) {
  throw new Error("Temporary official fail-closed validation root must be removed");
}

async function validateHeadlessRefresh({
  args = launchArgs,
  expectedPageState = null,
  profileLabel,
  profileRoot,
  scenario = "blank_profile",
  service,
  url,
}) {
  const response = await runSidecarRefresh({ args, service, url, profileLabel, profileRoot });

  if (
    response.ok !== true ||
    response.status !== "checked" ||
    response.action !== "refreshUsage" ||
    response.service !== service ||
    response.profileLabel !== profileLabel ||
    response.headless !== true ||
    response.argCount !== args.length
  ) {
    throw new Error(`Unexpected ${service} headless refresh response`);
  }

  if (!validFailClosedStates.has(response.pageState) && response.pageState !== "usage") {
    throw new Error(`${service} headless refresh returned an unsupported page state`);
  }

  if (response.pageState === "usage") {
    throw new Error(`${service} blank profile unexpectedly reached authenticated usage state`);
  }

  if (expectedPageState && response.pageState !== expectedPageState) {
    throw new Error(`${service} ${scenario} returned ${response.pageState}`);
  }

  return {
    failClosedState: response.pageState,
    headlessRefresh: true,
    profileLabel,
    sanitizedOutput: true,
    scenario,
    service,
    visibleBrowserRequired: false,
  };
}

async function runSidecarRefresh({ args, service, url, profileLabel, profileRoot }) {
  const child = spawn(sidecarPath, [], {
    detached: true,
    env: sidecarLaunchEnvironment(),
    stdio: ["pipe", "pipe", "pipe"],
  });
  const timeout = setTimeout(() => {
    signalProcessGroup(child.pid, "SIGTERM");
  }, launchTimeoutMs);
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
        action: "refreshUsage",
        backend: "playwright-headed-chromium-sidecar",
        service,
        url,
        profileLabel,
        userDataDir: profileRoot,
        headless: true,
        args,
      })}\n`,
    );
    child.stdin.end();

    const response = await waitForSidecarResponse(() => stdout);
    verifySanitizedOutput({ args, profileRoot, service, stderr, stdout, url });

    return response;
  } finally {
    clearTimeout(timeout);
    await stopProcessGroup(child);
  }
}

async function waitForSidecarResponse(readStdout) {
  const started = Date.now();

  while (Date.now() - started < launchTimeoutMs) {
    const line = readStdout()
      .split(/\r?\n/u)
      .find((candidate) => candidate.trim().length > 0);

    if (line) {
      return JSON.parse(line);
    }

    await new Promise((resolveDelay) => setTimeout(resolveDelay, 100));
  }

  throw new Error("Timed out waiting for Playwright sidecar refresh response");
}

async function waitForExit(child) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return true;
  }

  return new Promise((resolveExit) => {
    const timeout = setTimeout(() => resolveExit(false), stopTimeoutMs);
    child.once("exit", () => {
      clearTimeout(timeout);
      resolveExit(true);
    });
  });
}

async function stopProcessGroup(child) {
  const pid = child.pid;

  if (!pid) {
    return;
  }

  signalProcessGroup(pid, "SIGTERM");

  if (await waitForExit(child)) {
    return;
  }

  signalProcessGroup(pid, "SIGKILL");
  await waitForExit(child);
}

function signalProcessGroup(pid, signal) {
  if (!pid) {
    return;
  }

  try {
    process.kill(-pid, signal);
  } catch {}
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

function sidecarLaunchEnvironment() {
  const env = {
    ...process.env,
  };

  if (!env.PLAYWRIGHT_BROWSERS_PATH) {
    env.PLAYWRIGHT_BROWSERS_PATH = resolve(homedir(), ".cache/ms-playwright");
  }

  return env;
}

function verifySanitizedOutput({ args, profileRoot, service, stderr, stdout, url }) {
  const output = `${stdout}\n${stderr}`;

  for (const fragment of [profileRoot, url, ...args].filter(Boolean)) {
    if (output.includes(fragment)) {
      throw new Error(`${service} headless refresh output leaked sensitive launch data`);
    }
  }

  if (sensitiveOutputPattern.test(output)) {
    throw new Error(`${service} headless refresh output leaked auth material`);
  }

  if (/<!doctype|<html|<body|<script/iu.test(output)) {
    throw new Error(`${service} headless refresh output leaked page markup`);
  }
}

function verifySanitizedReport(report) {
  for (const fragment of [
    validationRoot,
    ...officialUsageRequests.map((request) => request.url),
    ...launchArgs,
    process.env.HOME,
  ].filter(Boolean)) {
    if (report.includes(fragment)) {
      throw new Error("Official fail-closed report leaked sensitive launch data");
    }
  }

  if (sensitiveOutputPattern.test(report)) {
    throw new Error("Official fail-closed report leaked auth material");
  }

  if (/<!doctype|<html|<body|<script/iu.test(report)) {
    throw new Error("Official fail-closed report leaked page markup");
  }
}

function osReleaseSummary() {
  try {
    const values = Object.fromEntries(
      readFileSync("/etc/os-release", "utf8")
        .split(/\r?\n/u)
        .map((line) => line.match(/^([A-Z_]+)=(.*)$/u))
        .filter(Boolean)
        .map((match) => [match[1], unquoteOsReleaseValue(match[2])]),
    );

    return {
      id: safeValue(values.ID),
      name: safeValue(values.PRETTY_NAME ?? values.NAME),
    };
  } catch {
    return {
      id: null,
      name: null,
    };
  }
}

function unquoteOsReleaseValue(value) {
  return value.replace(/^"(.*)"$/u, "$1");
}

function safeEnv(name) {
  return safeValue(process.env[name]);
}

function safeValue(value) {
  if (!value) {
    return null;
  }

  if (!/^[A-Za-z0-9_ .:+/-]{1,80}$/u.test(value)) {
    return "<redacted>";
  }

  return value;
}
