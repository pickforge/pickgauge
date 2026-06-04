import assert from "node:assert/strict";
import test from "node:test";
import {
  BACKEND_ID,
  PROTOCOL_VERSION,
  detectPageState,
  extractVisibleUsage,
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

test("extracts sanitized visible usage fields from page text", async () => {
  const usage = await extractVisibleUsage(
    fakePage({
      bodyText:
        "Team plan monthly window. 42.5% remaining. 57.5% used. Resets 2026-06-04T18:30.",
    }),
    "codex",
  );

  assert.deepEqual(usage, {
    remainingPercent: 42.5,
    resetAt: "2026-06-04T18:30:00Z",
    service: "codex",
    usedPercent: 57.5,
    visibleFields: [
      "remaining_percent",
      "used_percent",
      "reset_at",
      "plan_label",
      "quota_window",
    ],
  });
});

test("classifies synthetic visible page states without authenticated page content", async () => {
  assert.equal(
    await detectPageState(
      fakePage({ url: "https://auth.example.com/login" }),
      emptyVisibleUsage(),
      2,
    ),
    "logged_out",
  );
  assert.equal(
    await detectPageState(fakePage({ visibleTexts: ["Verify you are human"] }), emptyVisibleUsage(), 2),
    "captcha_or_bot_check",
  );
  assert.equal(
    await detectPageState(fakePage({ visibleTexts: ["Enter your verification code"] }), emptyVisibleUsage(), 2),
    "mfa_required",
  );
  assert.equal(
    await detectPageState(fakePage({ visibleTexts: ["Continue with email"] }), emptyVisibleUsage(), 2),
    "logged_out",
  );
  assert.equal(
    await detectPageState(
      fakePage(),
      { ...emptyVisibleUsage(), visibleFields: ["remaining_percent"] },
      2,
    ),
    "usage",
  );
  assert.equal(await detectPageState(fakePage(), emptyVisibleUsage(), 0), "logged_out");
  assert.equal(await detectPageState(fakePage(), emptyVisibleUsage(), 2), "unexpected_ui");
});

function emptyVisibleUsage() {
  return {
    remainingPercent: null,
    resetAt: null,
    usedPercent: null,
    visibleFields: [],
  };
}

function fakePage({ bodyText = "", url = "https://example.com/usage", visibleTexts = [] } = {}) {
  const locatorForText = (pattern) => fakeLocator(visibleTexts.some((text) => pattern.test(text)));

  return {
    getByRole: (_role, options = {}) => locatorForText(options.name ?? /$a/u),
    getByText: (pattern) => locatorForText(pattern),
    locator: (selector) => ({
      innerText: async () => (selector === "body" ? bodyText : ""),
    }),
    url: () => url,
  };
}

function fakeLocator(visible) {
  return {
    count: async () => (visible ? 1 : 0),
    nth: () => ({
      isVisible: async () => visible,
    }),
  };
}
