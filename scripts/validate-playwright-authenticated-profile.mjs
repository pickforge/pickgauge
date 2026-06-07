#!/usr/bin/env node

import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import {
  existsSync,
  lstatSync,
  readFileSync,
  readdirSync,
} from "node:fs";
import { homedir } from "node:os";
import { dirname, isAbsolute, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const targetTriple = execFileSync("rustc", ["--print", "host-tuple"], {
  encoding: "utf8",
}).trim();
const sidecarPath = resolve(
  repoRoot,
  "src-tauri/binaries",
  `pickgauge-playwright-sidecar-${targetTriple}`,
);
const launchTimeoutMs = 30_000;
const stopTimeoutMs = 3_000;
const profileInspectionEntryLimit = 4_096;
const logInspectionByteLimit = 2 * 1024 * 1024;
const profileMarkerFileName = ".pickgauge-profile.json";
const profileMarkerSchemaVersion = 1;
const appIdentifier = "com.pickforge.pickgauge";
const launchArgs = [
  "--disable-save-password-bubble",
  "--disable-password-manager-reauthentication",
  "--disable-features=AutofillServerCommunication",
  "--no-first-run",
];
const serviceDefinitions = [
  {
    argName: "--codex-profile",
    envName: "PICKGAUGE_AUTH_CODEX_PROFILE_ROOT",
    profileLabel: "codex-profile",
    service: "codex",
    url: "https://chatgpt.com/codex/cloud/settings/analytics",
  },
  {
    argName: "--claude-profile",
    envName: "PICKGAUGE_AUTH_CLAUDE_PROFILE_ROOT",
    profileLabel: "claude-profile",
    service: "claude",
    url: "https://claude.ai/new#settings/usage",
  },
];
const failClosedStates = new Set([
  "logged_out",
  "mfa_required",
  "captcha_or_bot_check",
  "network_unavailable",
  "timed_out",
  "unexpected_ui",
]);
const sensitiveOutputPatterns = [
  /\b(?:set-cookie|cookie|authorization)\s*:|\bbearer\s+[A-Za-z0-9._~+/-]+=*/iu,
  /\b(access[_-]?token|refresh[_-]?token|id[_-]?token|session[_-]?(id|token)|csrf)\b/iu,
  /\bsk-[A-Za-z0-9]{20,}\b/u,
  /<!doctype|<html|<body|<script/iu,
  /\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b/iu,
];

try {
  await main();
} catch (error) {
  printSanitizedFailure(error);
  process.exitCode = 1;
}

async function main() {
  const options = parseOptions(process.argv.slice(2));

  if (options.help) {
    printHelp();
    return;
  }

  validateSharedProfileRoot(options.profileRoot);

  const requests = serviceDefinitions
    .map((definition) => ({
      ...definition,
      profileRoot: profileRootForService(definition, options),
    }))
    .filter((request) => request.profileRoot);

  if (requests.length === 0) {
    throw new Error(
      "Provide at least one authenticated profile root with --profile-root, --codex-profile, --claude-profile, PICKGAUGE_AUTH_PROFILE_ROOT, PICKGAUGE_AUTH_CODEX_PROFILE_ROOT, or PICKGAUGE_AUTH_CLAUDE_PROFILE_ROOT",
    );
  }

  const results = [];

  for (const request of requests) {
    results.push(await validateAuthenticatedProfile(request, options));
  }

  const logInspection = inspectSanitizedLogFile(options, requests);

  const output = JSON.stringify(
    {
      generatedAt: new Date().toISOString(),
      backend: "playwright-headed-chromium-sidecar",
      desktopSession: {
        currentDesktop: safeEnv("XDG_CURRENT_DESKTOP"),
        xdgSessionType: safeEnv("XDG_SESSION_TYPE"),
      },
      os: osReleaseSummary(),
      targetTriple,
      logInspection,
      services: results,
    },
    null,
    2,
  );

  verifySanitizedReport(output, requests);
  console.log(output);
}

async function validateAuthenticatedProfile(request, options) {
  validateProfileRoot(request);
  const profileMarker = inspectProfileMarker(request, options);
  const preflightProfileStorage = inspectChromiumProfileStorage(request.profileRoot);
  const preflightDisabledStoragePreferences = inspectDisabledStoragePreferences(request.profileRoot);
  const preflightDefaultProfileReferences = inspectDefaultProfileReferences(request.profileRoot);

  assertProfileStorageSafety(
    request,
    options,
    preflightProfileStorage,
    preflightDisabledStoragePreferences,
    preflightDefaultProfileReferences,
  );

  const response = await runSidecarRefresh(request);

  assert.equal(response.ok, true, `${request.service} headless refresh must be accepted`);
  assert.equal(response.status, "checked", `${request.service} headless refresh must be checked`);
  assert.equal(response.action, "refreshUsage", `${request.service} action must be refreshUsage`);
  assert.equal(response.service, request.service, `${request.service} response service mismatch`);
  assert.equal(
    response.profileLabel,
    request.profileLabel,
    `${request.service} response profile label mismatch`,
  );
  assert.equal(response.headless, true, `${request.service} refresh must be headless`);

  if (response.pageState !== "usage" && !failClosedStates.has(response.pageState)) {
    throw new Error(`${request.service} returned unsupported page state`);
  }

  if (options.requireUsage && response.pageState !== "usage") {
    throw new Error(`${request.service} authenticated profile did not reach usage state`);
  }

  const profileStorage = inspectChromiumProfileStorage(request.profileRoot);
  const disabledStoragePreferences = inspectDisabledStoragePreferences(request.profileRoot);
  const defaultProfileReferences = inspectDefaultProfileReferences(request.profileRoot);

  assertProfileStorageSafety(
    request,
    options,
    profileStorage,
    disabledStoragePreferences,
    defaultProfileReferences,
  );

  const sessionStorageArtifactsPresent =
    profileStorage.cookieStoreFiles > 0 || profileStorage.siteStorageEntries > 0;
  const authenticatedSessionEvidencePresent =
    response.pageState === "usage" && sessionStorageArtifactsPresent;

  if (
    options.requireSessionStorageArtifacts &&
    !authenticatedSessionEvidencePresent
  ) {
    throw new Error(
      `${request.service} authenticated session state or storage artifacts are missing`,
    );
  }

  return {
    authenticatedSessionEvidencePresent,
    autofillStoreFilesAbsent: profileStorage.autofillStoreFiles === 0,
    credentialStoreFilesAbsent: profileStorage.credentialStoreFiles === 0,
    defaultProfileReferences,
    failClosedState: response.pageState === "usage" ? null : response.pageState,
    headlessRefresh: true,
    profileLabel: request.profileLabel,
    profileMarker,
    profileStorage,
    sanitizedOutput: true,
    service: request.service,
    sessionStorageArtifactsPresent,
    usageFieldsVisible: response.pageState === "usage" ? response.visibleFields : [],
    usageStateReached: response.pageState === "usage",
    visibleBrowserRequired: false,
    disabledStoragePreferences,
  };
}

function assertProfileStorageSafety(
  request,
  options,
  profileStorage,
  disabledStoragePreferences,
  defaultProfileReferences,
) {
  if (profileStorage.symlinkEntries > 0) {
    throw new Error(`${request.service} authenticated profile contains symlink entries`);
  }

  if (profileStorage.entryLimitReached) {
    throw new Error(`${request.service} authenticated profile inspection reached the entry limit`);
  }

  if (options.requireNoCredentialStoreFiles && profileStorage.credentialStoreFiles > 0) {
    throw new Error(`${request.service} profile contains credential store files`);
  }

  if (options.requireNoAutofillStoreFiles && profileStorage.autofillStoreFiles > 0) {
    throw new Error(`${request.service} profile contains autofill store files`);
  }

  if (options.requireDisabledPreferences && !disabledStoragePreferences.allDisabled) {
    throw new Error(`${request.service} profile does not preserve disabled storage preferences`);
  }

  if (options.requireNoDefaultProfileReferences && !defaultProfileReferences.absent) {
    throw new Error(`${request.service} profile preferences reference a default browser profile`);
  }
}

function parseOptions(args) {
  const profileRoots = new Map();
  const options = {
    help: false,
    profileRoot: process.env.PICKGAUGE_AUTH_PROFILE_ROOT || null,
    profileRoots,
    requireDisabledPreferences: false,
    requireNoAutofillStoreFiles: false,
    requireNoCredentialStoreFiles: false,
    requireNoDefaultProfileReferences: false,
    requireSanitizedLogFile: false,
    requireSessionStorageArtifacts: false,
    requireUsage: false,
    logPath: process.env.PICKGAUGE_AUTH_LOG_PATH || null,
    allowUnmarkedTestProfile: false,
  };

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];

    if (arg === "--help" || arg === "-h") {
      options.help = true;
      continue;
    }

    if (arg === "--require-usage") {
      options.requireUsage = true;
      continue;
    }

    if (arg === "--require-disabled-storage-preferences") {
      options.requireDisabledPreferences = true;
      continue;
    }

    if (arg === "--require-no-credential-store-files") {
      options.requireNoCredentialStoreFiles = true;
      continue;
    }

    if (arg === "--require-no-autofill-store-files") {
      options.requireNoAutofillStoreFiles = true;
      continue;
    }

    if (arg === "--require-no-default-profile-references") {
      options.requireNoDefaultProfileReferences = true;
      continue;
    }

    if (arg === "--require-session-storage-artifacts") {
      options.requireSessionStorageArtifacts = true;
      continue;
    }

    if (arg === "--require-sanitized-log-file") {
      options.requireSanitizedLogFile = true;
      continue;
    }

    if (arg === "--allow-unmarked-test-profile") {
      options.allowUnmarkedTestProfile = true;
      continue;
    }

    if (arg === "--profile-root") {
      const value = args[index + 1];

      if (!value || value.startsWith("--")) {
        throw new Error("--profile-root requires an absolute browser profile root path");
      }

      if (!isAbsolute(value)) {
        throw new Error("--profile-root requires an absolute browser profile root path");
      }

      options.profileRoot = value;
      index += 1;
      continue;
    }

    if (arg === "--log-file") {
      const value = args[index + 1];

      if (!value || value.startsWith("--")) {
        throw new Error("--log-file requires an absolute log file path");
      }

      options.logPath = value;
      index += 1;
      continue;
    }

    const definition = serviceDefinitions.find((candidate) => candidate.argName === arg);

    if (!definition) {
      throw new Error(`Unsupported argument: ${arg}`);
    }

    const value = args[index + 1];

    if (!value || value.startsWith("--")) {
      throw new Error(`${arg} requires an absolute profile path`);
    }

    profileRoots.set(definition.service, value);
    index += 1;
  }

  return options;
}

function profileRootForService(definition, options) {
  if (options.profileRoot && !isAbsolute(options.profileRoot)) {
    throw new Error("PICKGAUGE_AUTH_PROFILE_ROOT must be an absolute browser profile root path");
  }

  return (
    options.profileRoots.get(definition.service) ??
    process.env[definition.envName] ??
    (options.profileRoot ? resolve(options.profileRoot, definition.service) : null)
  );
}

function validateSharedProfileRoot(profileRoot) {
  if (!profileRoot) {
    return;
  }

  if (!isAbsolute(profileRoot)) {
    throw new Error("Shared browser profile root must be absolute");
  }

  if (!existsSync(profileRoot)) {
    throw new Error("Shared browser profile root does not exist");
  }

  const stat = lstatSync(profileRoot);

  if (stat.isSymbolicLink()) {
    throw new Error("Shared browser profile root must not be a symlink");
  }

  if (!stat.isDirectory()) {
    throw new Error("Shared browser profile root must be a directory");
  }

  for (const defaultRoot of defaultBrowserProfileRoots()) {
    if (profileRoot === defaultRoot || profileRoot.startsWith(`${defaultRoot}/`)) {
      throw new Error("Shared browser profile root must not be a default browser profile");
    }
  }
}

function printHelp() {
  console.log(`Usage:
  npm --silent run smoke:auth-profile -- --profile-root /absolute/browser-profiles --log-file /absolute/pickgauge.log --require-usage --require-session-storage-artifacts --require-sanitized-log-file --require-disabled-storage-preferences --require-no-credential-store-files --require-no-autofill-store-files --require-no-default-profile-references
  npm --silent run smoke:auth-profile -- --codex-profile /absolute/profile --claude-profile /absolute/profile --log-file /absolute/pickgauge.log --require-usage --require-session-storage-artifacts --require-sanitized-log-file --require-disabled-storage-preferences --require-no-credential-store-files --require-no-autofill-store-files --require-no-default-profile-references

Environment:
  PICKGAUGE_AUTH_PROFILE_ROOT=/absolute/browser-profiles
  PICKGAUGE_AUTH_CODEX_PROFILE_ROOT=/absolute/profile
  PICKGAUGE_AUTH_CLAUDE_PROFILE_ROOT=/absolute/profile
  PICKGAUGE_AUTH_LOG_PATH=/absolute/pickgauge.log

The command runs headless refresh checks only. Profile roots must contain PickGauge
ownership markers unless --allow-unmarked-test-profile is used for disposable tests.
Use npm --silent or environment variables for real profile paths so npm does not echo
CLI arguments before the helper starts. The helper emits sanitized JSON without
profile paths, official URLs, cookies, tokens, auth headers, browser storage
contents, or page markup. Use --require-sanitized-log-file with a normal app log
file to prove authenticated smoke runs did not log sensitive auth or page content.`);
}

function validateProfileRoot({ profileRoot, service }) {
  if (!isAbsolute(profileRoot)) {
    throw new Error(`${service} profile root must be absolute`);
  }

  if (!existsSync(profileRoot)) {
    throw new Error(`${service} profile root does not exist`);
  }

  const stat = lstatSync(profileRoot);

  if (stat.isSymbolicLink()) {
    throw new Error(`${service} profile root must not be a symlink`);
  }

  if (!stat.isDirectory()) {
    throw new Error(`${service} profile root must be a directory`);
  }

  for (const defaultRoot of defaultBrowserProfileRoots()) {
    if (profileRoot === defaultRoot || profileRoot.startsWith(`${defaultRoot}/`)) {
      throw new Error(`${service} profile root must not be a default browser profile`);
    }
  }
}

function inspectProfileMarker({ profileRoot, service }, options) {
  const markerPath = resolve(profileRoot, profileMarkerFileName);

  if (!existsSync(markerPath)) {
    if (options.allowUnmarkedTestProfile) {
      return {
        appOwned: false,
        present: false,
        serviceMatches: null,
      };
    }

    throw new Error(`${service} profile root is missing the PickGauge ownership marker`);
  }

  const markerStat = lstatSync(markerPath);

  if (markerStat.isSymbolicLink()) {
    throw new Error(`${service} profile ownership marker must not be a symlink`);
  }

  if (!markerStat.isFile()) {
    throw new Error(`${service} profile ownership marker must be a file`);
  }

  const marker = JSON.parse(readFileSync(markerPath, "utf8"));
  const serviceMatches = marker.service === service;
  const appOwned =
    marker.schemaVersion === profileMarkerSchemaVersion &&
    marker.appIdentifier === appIdentifier &&
    serviceMatches;

  if (!appOwned) {
    throw new Error(`${service} profile ownership marker does not match PickGauge`);
  }

  return {
    appOwned: true,
    present: true,
    serviceMatches,
  };
}

async function runSidecarRefresh({ service, url, profileLabel, profileRoot }) {
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
        args: launchArgs,
      })}\n`,
    );
    child.stdin.end();

    const response = await waitForSidecarResponse(() => stdout);
    verifySanitizedSidecarOutput({ profileRoot, service, stderr, stdout, url });

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

    await delay(100);
  }

  throw new Error("Timed out waiting for Playwright authenticated-profile refresh response");
}

function inspectChromiumProfileStorage(profileRoot) {
  const inspection = {
    autofillStoreFiles: 0,
    cookieStoreFiles: 0,
    credentialStoreFiles: 0,
    entryLimitReached: false,
    inspectedEntries: 0,
    siteStorageEntries: 0,
    symlinkEntries: 0,
  };

  const pending = [profileRoot];

  while (pending.length > 0) {
    if (inspection.inspectedEntries >= profileInspectionEntryLimit) {
      inspection.entryLimitReached = true;
      return inspection;
    }

    const current = pending.pop();

    for (const name of readdirSync(current)) {
      if (inspection.inspectedEntries >= profileInspectionEntryLimit) {
        inspection.entryLimitReached = true;
        return inspection;
      }

      const entryPath = resolve(current, name);
      const stat = lstatSync(entryPath);
      inspection.inspectedEntries += 1;

      if (stat.isSymbolicLink()) {
        inspection.symlinkEntries += 1;
        continue;
      }

      if (isChromiumLoginDataFile(name)) {
        inspection.credentialStoreFiles += 1;
      }

      if (isChromiumAutofillDataFile(name)) {
        inspection.autofillStoreFiles += 1;
      }

      if (isChromiumCookieDataFile(name)) {
        inspection.cookieStoreFiles += 1;
      }

      if (isChromiumSiteStorageEntry(name)) {
        inspection.siteStorageEntries += 1;
      }

      if (stat.isDirectory()) {
        pending.push(entryPath);
      }
    }
  }

  return inspection;
}

function inspectDisabledStoragePreferences(profileRoot) {
  const preferencesPath = resolve(profileRoot, "Default", "Preferences");

  if (!existsSync(preferencesPath)) {
    return {
      allDisabled: false,
      autofillDisabled: null,
      passwordSavingDisabled: null,
      preferencesPresent: false,
    };
  }

  const preferences = JSON.parse(readFileSync(preferencesPath, "utf8"));
  const passwordSavingDisabled =
    preferenceAtPath(preferences, ["credentials_enable_service"]) === false &&
    preferenceAtPath(preferences, ["credentials_enable_autosignin"]) === false &&
    preferenceAtPath(preferences, ["profile", "password_manager_enabled"]) === false &&
    preferenceAtPath(preferences, ["profile", "password_manager_allow_show_passwords"]) === false;
  const autofillDisabled =
    preferenceAtPath(preferences, ["autofill", "profile_enabled"]) === false &&
    preferenceAtPath(preferences, ["autofill", "credit_card_enabled"]) === false &&
    preferenceAtPath(preferences, ["autofill", "enabled"]) !== true;

  return {
    allDisabled: passwordSavingDisabled && autofillDisabled,
    autofillDisabled,
    passwordSavingDisabled,
    preferencesPresent: true,
  };
}

function inspectDefaultProfileReferences(profileRoot) {
  const preferencesPath = resolve(profileRoot, "Default", "Preferences");

  if (!existsSync(preferencesPath)) {
    return {
      absent: true,
      preferencesPresent: false,
    };
  }

  const rawPreferences = readFileSync(preferencesPath, "utf8");
  const absent = defaultBrowserProfileRoots().every(
    (defaultRoot) => !rawPreferences.includes(defaultRoot),
  );

  return {
    absent,
    preferencesPresent: true,
  };
}

function inspectSanitizedLogFile(options, requests) {
  if (!options.logPath) {
    if (options.requireSanitizedLogFile) {
      throw new Error("normal app log file is required for authenticated smoke");
    }

    return {
      provided: false,
      inspected: false,
      required: options.requireSanitizedLogFile,
      sensitiveContentAbsent: null,
      sizeBytes: null,
      sizeLimitBytes: logInspectionByteLimit,
    };
  }

  if (!isAbsolute(options.logPath)) {
    throw new Error("normal app log file path must be absolute");
  }

  const logPath = resolve(options.logPath);

  if (!existsSync(logPath)) {
    throw new Error("normal app log file does not exist");
  }

  let stat;

  try {
    stat = lstatSync(logPath);
  } catch {
    throw new Error("normal app log file could not be inspected");
  }

  if (stat.isSymbolicLink()) {
    throw new Error("normal app log file must not be a symlink");
  }

  if (!stat.isFile()) {
    throw new Error("normal app log file must be a file");
  }

  if (stat.size > logInspectionByteLimit) {
    throw new Error("normal app log file is too large to inspect safely");
  }

  let rawLog;

  try {
    rawLog = readFileSync(logPath, "utf8");
  } catch {
    throw new Error("normal app log file could not be inspected");
  }

  assertNoSensitiveFragments(rawLog, requests);

  if (sensitiveOutputPatterns.some((pattern) => pattern.test(rawLog))) {
    throw new Error("normal app log file leaked sensitive auth or page material");
  }

  return {
    provided: true,
    inspected: true,
    required: options.requireSanitizedLogFile,
    sensitiveContentAbsent: true,
    sizeBytes: stat.size,
    sizeLimitBytes: logInspectionByteLimit,
  };
}

function preferenceAtPath(preferences, path) {
  return path.reduce((value, key) => value?.[key], preferences);
}

function isChromiumLoginDataFile(name) {
  return name === "Login Data" || name.startsWith("Login Data-");
}

function isChromiumAutofillDataFile(name) {
  return name === "Web Data" || name.startsWith("Web Data-");
}

function isChromiumCookieDataFile(name) {
  return name === "Cookies" || name.startsWith("Cookies-");
}

function isChromiumSiteStorageEntry(name) {
  return ["IndexedDB", "Local Storage", "Session Storage", "Service Worker"].includes(name);
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

function signalProcessGroup(pid, signal) {
  if (!pid) {
    return;
  }

  try {
    process.kill(-pid, signal);
  } catch {
  }
}

function sidecarLaunchEnvironment() {
  const env = { ...process.env };

  if (!env.PLAYWRIGHT_BROWSERS_PATH) {
    env.PLAYWRIGHT_BROWSERS_PATH = resolve(homedir(), ".cache/ms-playwright");
  }

  return env;
}

function verifySanitizedSidecarOutput({ profileRoot, service, stderr, stdout, url }) {
  const output = `${stdout}\n${stderr}`;

  assertNoSensitiveFragments(output, [{ profileRoot, url }]);

  for (const fragment of launchArgs) {
    if (output.includes(fragment)) {
      throw new Error(`${service} authenticated-profile refresh output leaked launch args`);
    }
  }

  if (sensitiveOutputPatterns.some((pattern) => pattern.test(output))) {
    throw new Error(`${service} authenticated-profile refresh output leaked auth material`);
  }

  if (/<!doctype|<html|<body|<script/iu.test(output)) {
    throw new Error(`${service} authenticated-profile refresh output leaked page markup`);
  }
}

function printSanitizedFailure(error) {
  const message = typeof error?.message === "string" ? error.message : "";
  const code =
    [
      ["missing_profile_marker", /missing the PickGauge ownership marker/u],
      ["invalid_profile_marker", /ownership marker/u],
      ["default_browser_profile", /must not be a default browser profile/u],
      ["invalid_profile_root", /profile root/u],
      ["usage_not_reached", /did not reach usage state/u],
      ["storage_preferences_not_disabled", /disabled storage preferences/u],
      ["credential_store_detected", /credential store files/u],
      ["autofill_store_detected", /autofill store files/u],
      ["default_profile_reference_detected", /preferences reference a default browser profile/u],
      ["session_artifacts_missing", /session state or storage artifacts are missing/u],
      ["log_file_missing", /normal app log file (is required|does not exist)/u],
      ["invalid_log_file", /normal app log file (path must be absolute|could not be inspected|must not be a symlink|must be a file)/u],
      ["log_file_too_large", /normal app log file is too large/u],
      ["sensitive_log_detected", /normal app log file leaked|output leaked sensitive launch data|output leaked the home directory path/u],
      ["profile_inspection_failed", /profile inspection/u],
      ["unsupported_page_state", /unsupported page state/u],
      ["sidecar_timeout", /Timed out/u],
    ].find(([, pattern]) => pattern.test(message))?.[0] ?? "auth_profile_smoke_failed";

  console.error(
    JSON.stringify({
      ok: false,
      code,
      message: "Authenticated profile smoke failed",
    }),
  );
}

function assertNoSensitiveFragments(output, requests) {
  const fragments = requests
    .flatMap((request) => [request.profileRoot, request.url])
    .filter(Boolean);

  for (const fragment of fragments) {
    if (output.includes(fragment)) {
      throw new Error("Authenticated-profile smoke output leaked sensitive launch data");
    }
  }

  if (process.env.HOME && output.includes(process.env.HOME)) {
    throw new Error("Authenticated-profile smoke output leaked the home directory path");
  }
}

function verifySanitizedReport(output, requests) {
  assertNoSensitiveFragments(output, requests);

  for (const fragment of launchArgs) {
    if (output.includes(fragment)) {
      throw new Error("Authenticated-profile smoke output leaked launch args");
    }
  }

  if (sensitiveOutputPatterns.some((pattern) => pattern.test(output))) {
    throw new Error("Authenticated-profile smoke output leaked auth or page material");
  }
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
    resolve(xdgConfigHome, "BraveSoftware"),
    resolve(xdgConfigHome, "microsoft-edge"),
    resolve(xdgConfigHome, "vivaldi"),
    resolve(xdgConfigHome, "opera"),
    resolve(home, ".mozilla/firefox"),
    resolve(home, ".var/app/com.google.Chrome"),
    resolve(home, ".var/app/com.brave.Browser"),
    resolve(home, ".var/app/org.chromium.Chromium"),
    resolve(home, ".var/app/org.mozilla.firefox"),
  ];
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

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}
