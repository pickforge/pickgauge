import assert from "node:assert/strict";
import test from "node:test";
import {
  BACKEND_ID,
  PROTOCOL_VERSION,
  runLaunchRequest,
  sanitizedAcceptedResponse,
  validateLaunchRequest,
} from "./forgegauge-playwright-sidecar.mjs";

function request(overrides = {}) {
  return {
    action: "launchLogin",
    args: [
      "--disable-save-password-bubble",
      "--disable-password-manager-reauthentication",
      "--disable-features=AutofillServerCommunication",
      "--no-first-run",
    ],
    backend: BACKEND_ID,
    headless: false,
    profileLabel: "codex-profile",
    protocolVersion: PROTOCOL_VERSION,
    service: "codex",
    url: "https://chatgpt.com/codex/cloud/settings/analytics",
    userDataDir: "/home/dev/.local/share/com.pickforge.forgegauge/browser-profiles/codex",
    ...overrides,
  };
}

test("accepts sanitized headed Playwright login launch requests", () => {
  const validation = validateLaunchRequest(request());

  assert.equal(validation.ok, true);
  assert.equal(validation.request.profileLabel, "codex-profile");
  assert.equal(validation.request.headless, false);
});

test("accepts sanitized headless Playwright usage refresh requests", () => {
  const validation = validateLaunchRequest(
    request({
      action: "refreshUsage",
      headless: true,
    }),
  );

  assert.equal(validation.ok, true);
  assert.equal(validation.request.action, "refreshUsage");
  assert.equal(validation.request.headless, true);
});

test("dry-run response omits raw user data directory and launch args", async () => {
  const rawPath = "/home/dev/.local/share/com.pickforge.forgegauge/browser-profiles/claude";
  const result = await runLaunchRequest(
    request({
      profileLabel: "claude-profile",
      service: "claude",
      url: "https://claude.ai/usage",
      userDataDir: rawPath,
    }),
    { dryRun: true },
  );
  const serialized = JSON.stringify(result);

  assert.deepEqual(
    result,
    sanitizedAcceptedResponse({
      action: "launchLogin",
      args: request().args,
      backend: BACKEND_ID,
      headless: false,
      profileLabel: "claude-profile",
      service: "claude",
    }),
  );
  assert.equal(serialized.includes(rawPath), false);
  assert.equal(serialized.includes("/home/dev"), false);
  assert.equal(serialized.includes("--disable-save-password-bubble"), false);
});

test("rejects visible browser usage refresh requests", () => {
  const validation = validateLaunchRequest(
    request({
      action: "refreshUsage",
      headless: false,
    }),
  );

  assert.deepEqual(validation, {
    ok: false,
    code: "headless_mode_required",
  });
});

test("rejects headless login launch requests", () => {
  const validation = validateLaunchRequest(request({ headless: true }));

  assert.deepEqual(validation, {
    ok: false,
    code: "headed_mode_required",
  });
});

test("rejects user-data-dir launch args because Playwright receives it separately", () => {
  const validation = validateLaunchRequest(
    request({
      args: ["--user-data-dir=/home/dev/.config/chromium"],
    }),
  );

  assert.deepEqual(validation, {
    ok: false,
    code: "user_data_dir_arg_forbidden",
  });
});

test("rejects invalid requests without echoing sensitive input", async () => {
  const rawPath = "/home/dev/secret-profile";
  const result = await runLaunchRequest(
    request({
      headless: true,
      userDataDir: rawPath,
    }),
    { dryRun: true },
  );
  const serialized = JSON.stringify(result);

  assert.equal(result.ok, false);
  assert.equal(result.code, "headed_mode_required");
  assert.equal(serialized.includes(rawPath), false);
  assert.equal(serialized.includes("/home/dev"), false);
});
