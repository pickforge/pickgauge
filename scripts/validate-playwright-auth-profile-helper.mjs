#!/usr/bin/env node

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const helperPath = resolve(repoRoot, "scripts/validate-playwright-authenticated-profile.mjs");
const validationRoot = mkdtempSync(resolve(tmpdir(), "forgegauge-auth-profile-helper-"));
const appIdentifier = "com.pickforge.forgegauge";
const markerFileName = ".forgegauge-profile.json";
const officialUrls = [
  "https://chatgpt.com/codex/cloud/settings/analytics",
  "https://claude.ai/new#settings/usage",
];
const failClosedStates = new Set([
  "logged_out",
  "mfa_required",
  "captcha_or_bot_check",
  "network_unavailable",
  "timed_out",
  "unexpected_ui",
]);

try {
  validateHelpOutput();
  validateStrictBlankProfileRefresh();
  validateSharedProfileRootInput();
  validateEnvironmentSharedProfileRootInput();
  validateServiceProfileOverridePrecedence();
  validateRelativeSharedProfileRootFailure();
  validateRelativeEnvironmentSharedProfileRootFailure();
  validateSymlinkSharedProfileRootFailure();
  validateEnvironmentProfileInputs();
  validatePreflightFailure({
    code: "credential_store_detected",
    fileName: "Login Data",
    flag: "--require-no-credential-store-files",
    name: "credential-store",
  });
  validatePreflightFailure({
    code: "autofill_store_detected",
    fileName: "Web Data",
    flag: "--require-no-autofill-store-files",
    name: "autofill-store",
  });
  validateProfileMarkerMismatchFailure();
  validateDefaultProfileReferenceFailure();
  validateSessionArtifactRequirementFailure();
  validateSensitiveLogFailure();

  console.log("Authenticated profile helper validation passed");
} finally {
  rmSync(validationRoot, { force: true, recursive: true });
}

if (existsSync(validationRoot)) {
  throw new Error("Temporary authenticated-profile helper validation root must be removed");
}

function validateHelpOutput() {
  const result = runHelper(["--help"]);

  assert.equal(result.status, 0);
  assert.match(result.stdout, /smoke:auth-profile/u);
  assertSanitized(result);
}

function validateStrictBlankProfileRefresh() {
  const codexProfileRoot = createProfile("codex", "strict-blank-codex");
  const claudeProfileRoot = createProfile("claude", "strict-blank-claude");
  const logPath = resolve(validationRoot, "strict-blank.log");

  writeFileSync(logPath, "ForgeGauge startup completed\n", { mode: 0o600 });

  const result = runHelper([
    "--codex-profile",
    codexProfileRoot,
    "--claude-profile",
    claudeProfileRoot,
    "--log-file",
    logPath,
    "--require-sanitized-log-file",
    "--require-disabled-storage-preferences",
    "--require-no-credential-store-files",
    "--require-no-autofill-store-files",
    "--require-no-default-profile-references",
  ]);
  const output = JSON.parse(result.stdout);
  const services = new Map(output.services.map((service) => [service.service, service]));

  assert.equal(result.status, 0);
  assert.equal(output.logInspection.inspected, true);
  assert.equal(output.logInspection.sensitiveContentAbsent, true);
  assert.equal(output.services.length, 2);
  assertServiceResult(services.get("codex"), "codex");
  assertServiceResult(services.get("claude"), "claude");
  assertSanitized(result, [codexProfileRoot, claudeProfileRoot, logPath]);
}

function validateEnvironmentProfileInputs() {
  const codexProfileRoot = createProfile("codex", "env-codex");
  const claudeProfileRoot = createProfile("claude", "env-claude");
  const logPath = resolve(validationRoot, "env.log");

  writeFileSync(logPath, "ForgeGauge env smoke completed\n", { mode: 0o600 });

  const result = runHelper(
    [
      "--require-sanitized-log-file",
      "--require-disabled-storage-preferences",
      "--require-no-credential-store-files",
      "--require-no-autofill-store-files",
      "--require-no-default-profile-references",
    ],
    {
      FORGEGAUGE_AUTH_CLAUDE_PROFILE_ROOT: claudeProfileRoot,
      FORGEGAUGE_AUTH_CODEX_PROFILE_ROOT: codexProfileRoot,
      FORGEGAUGE_AUTH_LOG_PATH: logPath,
    },
  );
  const output = JSON.parse(result.stdout);
  const services = new Map(output.services.map((service) => [service.service, service]));

  assert.equal(result.status, 0);
  assert.equal(output.logInspection.inspected, true);
  assert.equal(output.logInspection.sensitiveContentAbsent, true);
  assert.equal(output.services.length, 2);
  assertServiceResult(services.get("codex"), "codex");
  assertServiceResult(services.get("claude"), "claude");
  assertSanitized(result, [codexProfileRoot, claudeProfileRoot, logPath]);
}

function validateSharedProfileRootInput() {
  const profileRoot = resolve(validationRoot, "shared-root");
  const codexProfileRoot = createProfile("codex", "shared-root/codex");
  const claudeProfileRoot = createProfile("claude", "shared-root/claude");
  const logPath = resolve(validationRoot, "shared-root.log");

  writeFileSync(logPath, "ForgeGauge shared root smoke completed\n", { mode: 0o600 });

  const result = runHelper([
    "--profile-root",
    profileRoot,
    "--log-file",
    logPath,
    "--require-sanitized-log-file",
    "--require-disabled-storage-preferences",
    "--require-no-credential-store-files",
    "--require-no-autofill-store-files",
    "--require-no-default-profile-references",
  ]);
  const output = JSON.parse(result.stdout);
  const services = new Map(output.services.map((service) => [service.service, service]));

  assert.equal(result.status, 0);
  assert.equal(output.logInspection.inspected, true);
  assert.equal(output.logInspection.sensitiveContentAbsent, true);
  assert.equal(output.services.length, 2);
  assertServiceResult(services.get("codex"), "codex");
  assertServiceResult(services.get("claude"), "claude");
  assertSanitized(result, [profileRoot, codexProfileRoot, claudeProfileRoot, logPath]);
}

function validateRelativeSharedProfileRootFailure() {
  const result = runHelper(["--profile-root", "relative-browser-profiles"]);

  assertFailureCode(result, "invalid_profile_root");
  assertSanitized(result, ["relative-browser-profiles"]);
}

function validateEnvironmentSharedProfileRootInput() {
  const profileRoot = resolve(validationRoot, "env-shared-root");
  const codexProfileRoot = createProfile("codex", "env-shared-root/codex");
  const claudeProfileRoot = createProfile("claude", "env-shared-root/claude");
  const logPath = resolve(validationRoot, "env-shared-root.log");

  writeFileSync(logPath, "ForgeGauge environment shared root smoke completed\n", {
    mode: 0o600,
  });

  const result = runHelper(
    [
      "--require-sanitized-log-file",
      "--require-disabled-storage-preferences",
      "--require-no-credential-store-files",
      "--require-no-autofill-store-files",
      "--require-no-default-profile-references",
    ],
    {
      FORGEGAUGE_AUTH_LOG_PATH: logPath,
      FORGEGAUGE_AUTH_PROFILE_ROOT: profileRoot,
    },
  );
  const output = JSON.parse(result.stdout);
  const services = new Map(output.services.map((service) => [service.service, service]));

  assert.equal(result.status, 0);
  assert.equal(output.logInspection.inspected, true);
  assert.equal(output.logInspection.sensitiveContentAbsent, true);
  assert.equal(output.services.length, 2);
  assertServiceResult(services.get("codex"), "codex");
  assertServiceResult(services.get("claude"), "claude");
  assertSanitized(result, [profileRoot, codexProfileRoot, claudeProfileRoot, logPath]);
}

function validateServiceProfileOverridePrecedence() {
  const profileRoot = resolve(validationRoot, "override-shared-root");
  const unusedCodexProfileRoot = createProfile("codex", "override-shared-root/codex");
  const claudeProfileRoot = createProfile("claude", "override-shared-root/claude");
  const codexProfileRoot = createProfile("codex", "override-codex");
  const logPath = resolve(validationRoot, "override-precedence.log");

  writeFileSync(logPath, "ForgeGauge override precedence smoke completed\n", { mode: 0o600 });

  const result = runHelper([
    "--profile-root",
    profileRoot,
    "--codex-profile",
    codexProfileRoot,
    "--log-file",
    logPath,
    "--require-sanitized-log-file",
    "--require-disabled-storage-preferences",
    "--require-no-credential-store-files",
    "--require-no-autofill-store-files",
    "--require-no-default-profile-references",
  ]);
  const output = JSON.parse(result.stdout);
  const services = new Map(output.services.map((service) => [service.service, service]));

  assert.equal(result.status, 0);
  assert.equal(output.logInspection.inspected, true);
  assert.equal(output.logInspection.sensitiveContentAbsent, true);
  assert.equal(output.services.length, 2);
  assertServiceResult(services.get("codex"), "codex");
  assertServiceResult(services.get("claude"), "claude");
  assertSanitized(result, [
    profileRoot,
    codexProfileRoot,
    claudeProfileRoot,
    unusedCodexProfileRoot,
    logPath,
  ]);
}

function validateRelativeEnvironmentSharedProfileRootFailure() {
  const result = runHelper([], {
    FORGEGAUGE_AUTH_PROFILE_ROOT: "relative-browser-profiles",
  });

  assertFailureCode(result, "invalid_profile_root");
  assertSanitized(result, ["relative-browser-profiles"]);
}

function validateSymlinkSharedProfileRootFailure() {
  const realProfileRoot = resolve(validationRoot, "symlink-target");
  const symlinkProfileRoot = resolve(validationRoot, "symlink-shared-root");

  createProfile("codex", "symlink-target/codex");
  createProfile("claude", "symlink-target/claude");
  symlinkSync(realProfileRoot, symlinkProfileRoot, "dir");

  const result = runHelper(["--profile-root", symlinkProfileRoot]);

  assertFailureCode(result, "invalid_profile_root");
  assertSanitized(result, [realProfileRoot, symlinkProfileRoot]);
}

function assertServiceResult(service, serviceName) {
  assert.ok(service, `${serviceName} service result should be present`);
  assert.equal(service.service, serviceName);
  assert.equal(service.headlessRefresh, true);
  assert.equal(service.visibleBrowserRequired, false);
  assert.equal(service.profileMarker.appOwned, true);
  assert.equal(service.credentialStoreFilesAbsent, true);
  assert.equal(service.autofillStoreFilesAbsent, true);
  assert.equal(service.disabledStoragePreferences.allDisabled, true);
  assert.equal(service.defaultProfileReferences.absent, true);
  assert.ok(
    service.failClosedState === null || failClosedStates.has(service.failClosedState),
    `Unexpected fail-closed state: ${service.failClosedState}`,
  );
}

function validateProfileMarkerMismatchFailure() {
  const profileRoot = createProfile("claude", "marker-mismatch");
  const result = runHelper(["--codex-profile", profileRoot]);

  assertFailureCode(result, "invalid_profile_marker");
  assertSanitized(result, [profileRoot]);
}

function validatePreflightFailure({ code, fileName, flag, name }) {
  const profileRoot = createProfile("codex", name);
  const filePath = resolve(profileRoot, "Default", fileName);

  writeFileSync(filePath, "", { mode: 0o600 });

  const result = runHelper(["--codex-profile", profileRoot, flag]);

  assertFailureCode(result, code);
  assertSanitized(result, [profileRoot, filePath]);
}

function validateDefaultProfileReferenceFailure() {
  const profileRoot = createProfile("codex", "default-profile-reference", {
    extraPreferences: {
      forgegaugeTestReference: resolve(homedir(), ".config/chromium"),
    },
  });
  const result = runHelper([
    "--codex-profile",
    profileRoot,
    "--require-no-default-profile-references",
  ]);

  assertFailureCode(result, "default_profile_reference_detected");
  assertSanitized(result, [profileRoot]);
}

function validateSessionArtifactRequirementFailure() {
  const profileRoot = createProfile("codex", "session-artifacts");
  const result = runHelper([
    "--codex-profile",
    profileRoot,
    "--require-session-storage-artifacts",
  ]);

  assertFailureCode(result, "session_artifacts_missing");
  assertSanitized(result, [profileRoot]);
}

function validateSensitiveLogFailure() {
  const profileRoot = createProfile("codex", "sensitive-log");
  const logPath = resolve(validationRoot, "sensitive.log");

  writeFileSync(logPath, "Authorization: Bearer abcdefghijklmnopqrstuvwxyz012345\n", {
    mode: 0o600,
  });

  const result = runHelper([
    "--codex-profile",
    profileRoot,
    "--log-file",
    logPath,
    "--require-sanitized-log-file",
  ]);

  assertFailureCode(result, "sensitive_log_detected");
  assertSanitized(result, [profileRoot, logPath]);
}

function createProfile(service, name, { extraPreferences = {} } = {}) {
  const profileRoot = resolve(validationRoot, name);
  const defaultRoot = resolve(profileRoot, "Default");

  mkdirSync(defaultRoot, { recursive: true, mode: 0o700 });
  writeJson(resolve(profileRoot, markerFileName), {
    appIdentifier,
    createdAt: "2026-06-04T12:00:00Z",
    schemaVersion: 1,
    service,
  });
  writeJson(resolve(defaultRoot, "Preferences"), {
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
    ...extraPreferences,
  });

  return profileRoot;
}

function writeJson(path, value) {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600 });
}

function runHelper(args, envOverrides = {}) {
  const result = spawnSync(process.execPath, [helperPath, ...args], {
    cwd: repoRoot,
    encoding: "utf8",
    env: {
      ...process.env,
      ...envOverrides,
      npm_config_loglevel: "silent",
    },
    timeout: 90_000,
  });

  if (result.error) {
    throw result.error;
  }

  return result;
}

function assertFailureCode(result, code) {
  assert.notEqual(result.status, 0);

  const failureLine = result.stderr
    .split(/\r?\n/u)
    .find((line) => line.trim().startsWith("{"));

  assert.ok(failureLine, `Expected sanitized failure JSON, got: ${result.stderr}`);

  const failure = JSON.parse(failureLine);

  assert.deepEqual(failure, {
    ok: false,
    code,
    message: "Authenticated profile smoke failed",
  });
}

function assertSanitized(result, extraFragments = []) {
  const output = `${result.stdout}\n${result.stderr}`;
  const fragments = [
    validationRoot,
    helperPath,
    ...officialUrls,
    ...extraFragments,
  ];

  for (const fragment of fragments) {
    assert.equal(
      output.includes(fragment),
      false,
      `Helper output leaked sensitive fragment: ${fragment}`,
    );
  }

  if (process.env.HOME) {
    assert.equal(output.includes(process.env.HOME), false, "Helper output leaked HOME");
  }
}
