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
} from "./pickgauge-playwright-sidecar.mjs";


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
    userDataDir: "/home/dev/.local/share/com.pickforge.pickgauge/browser-profiles/codex",
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

test("rejects managed login and refresh for unsupported providers", () => {
  for (const service of ["grok", "ollama"]) {
    const validation = validateLaunchRequest(
      request({
        service,
        profileLabel: `${service}-profile`,
        url: service === "grok" ? "https://grok.com/" : "https://ollama.com/settings",
      }),
    );

    assert.deepEqual(validation, { ok: false, code: "unsupported_service" });
  }
});

test("rejects harvested-session HTTP refresh actions", () => {
  const validation = validateLaunchRequest(
    request({ action: "httpRefreshUsage", headless: true }),
  );

  assert.deepEqual(validation, { ok: false, code: "unsupported_action" });
});



test("dry-run response omits raw user data directory and launch args", async () => {
  const rawPath = "/home/dev/.local/share/com.pickforge.pickgauge/browser-profiles/claude";
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
    weekly: null,
    fable: null,
  });
});

test("extracts Claude session, weekly, and Fable allowances from labeled text", async () => {
  const usage = await extractVisibleUsage(
    fakePage({
      bodyText:
        "Plan usage limits\nCurrent session\n18% used\nWeekly limits\nAll models\n43% used\nFable 5 only\n12% used",
    }),
    "claude",
  );

  assert.deepEqual(usage, {
    remainingPercent: 82,
    resetAt: null,
    service: "claude",
    usedPercent: 18,
    visibleFields: ["remaining_percent", "used_percent", "quota_window", "plan_label"],
    weekly: { remainingPercent: 57, resetAt: null, usedPercent: 43 },
    fable: { remainingPercent: 88, resetAt: null, usedPercent: 12 },
  });
});

test("does not infer a Claude Fable allowance without its label", async () => {
  const usage = await extractVisibleUsage(
    fakePage({ bodyText: "Current session\n18% used\nAll models\n43% used" }),
    "claude",
  );

  assert.equal(usage.fable, null);
});

test("preserves a remaining-only Claude fallback without a session label", async () => {
  const usage = await extractVisibleUsage(
    fakePage({ bodyText: "Max plan usage. 42% remaining." }),
    "claude",
  );

  assert.equal(usage.remainingPercent, 42);
  assert.equal(usage.usedPercent, null);
  assert.deepEqual(usage.visibleFields, ["remaining_percent", "plan_label"]);
});

test("treats a legacy Claude percent-usage label as used", async () => {
  const usage = await extractVisibleUsage(
    fakePage({ bodyText: "Max plan limits. 42% usage." }),
    "claude",
  );

  assert.equal(usage.remainingPercent, null);
  assert.equal(usage.usedPercent, 42);
  assert.deepEqual(usage.visibleFields, ["used_percent", "plan_label"]);
});

test("does not borrow a percentage from the next Claude quota row", async () => {
  const usage = await extractVisibleUsage(
    fakePage({
      bodyText:
        "Current session\nTemporarily unavailable\nWeekly limits\nAll models\n43% used\nFable 5 only\n12% used",
    }),
    "claude",
  );

  assert.equal(usage.remainingPercent, null);
  assert.equal(usage.usedPercent, null);
  assert.deepEqual(usage.weekly, { remainingPercent: 57, resetAt: null, usedPercent: 43 });
  assert.deepEqual(usage.fable, { remainingPercent: 88, resetAt: null, usedPercent: 12 });
});

test("does not promote secondary Claude rows without a session label", async () => {
  const usage = await extractVisibleUsage(
    fakePage({ bodyText: "All models\n43% used\nFable 5 only\n12% used" }),
    "claude",
  );

  assert.equal(usage.remainingPercent, null);
  assert.equal(usage.usedPercent, null);
  assert.deepEqual(usage.weekly, { remainingPercent: 57, resetAt: null, usedPercent: 43 });
  assert.deepEqual(usage.fable, { remainingPercent: 88, resetAt: null, usedPercent: 12 });
});

test("keeps a missing Claude weekly percentage separate from Fable", async () => {
  const usage = await extractVisibleUsage(
    fakePage({
      bodyText:
        "Current session\n18% used\nAll models\nTemporarily unavailable\nFable only\n12% used",
    }),
    "claude",
  );

  assert.equal(usage.weekly, null);
  assert.deepEqual(usage.fable, { remainingPercent: 88, resetAt: null, usedPercent: 12 });
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

function fakePage({
  bodyText = "",
  url = "https://example.com/usage",
  visibleTexts = [],
  evaluateResult = null,
} = {}) {
  const locatorForText = (pattern) => fakeLocator(visibleTexts.some((text) => pattern.test(text)));

  return {
    getByRole: (_role, options = {}) => locatorForText(options.name ?? /$a/u),
    getByText: (pattern) => locatorForText(pattern),
    locator: (selector) => ({
      innerText: async () => (selector === "body" ? bodyText : ""),
    }),
    evaluate: async () => evaluateResult,
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
