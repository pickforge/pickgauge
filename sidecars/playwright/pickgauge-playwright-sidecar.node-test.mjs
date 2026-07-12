import assert from "node:assert/strict";
import test from "node:test";
import {
  BACKEND_ID,
  PROTOCOL_VERSION,
  detectPageState,
  extractGrokCreditsUsage,
  extractOllamaUsageFromHtml,
  extractVisibleUsage,
  parseGrokCreditsBody,
  runLaunchRequest,
  sanitizedAcceptedResponse,
  validateLaunchRequest,
} from "./pickgauge-playwright-sidecar.mjs";

function ollamaUsageHtml({ session, weekly } = {}) {
  const meter = (label, time) =>
    `<div data-usage-meter><div data-usage-track aria-label="${label}"></div></div>` +
    (time ? `<div class="local-time" data-time="${time}">Resets</div>` : "");
  return [
    "<html><body>",
    session ? meter(`Session usage ${session.percent}% used`, session.resetAt) : "",
    weekly ? meter(`Weekly usage ${weekly.percent}% used`, weekly.resetAt) : "",
    "</body></html>",
  ].join("");
}

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

test("accepts headless http usage refresh requests", () => {
  const validation = validateLaunchRequest(
    request({ action: "httpRefreshUsage", service: "ollama", headless: true }),
  );

  assert.equal(validation.ok, true);
  assert.equal(validation.request.action, "httpRefreshUsage");
});

test("accepts Grok HTTP refreshes only for the credits endpoint", () => {
  const validation = validateLaunchRequest(
    request({
      action: "httpRefreshUsage",
      headless: true,
      service: "grok",
      url: "https://grok.com/rest/grok/credits",
    }),
  );

  assert.equal(validation.ok, true);
  assert.equal(validation.request.url, "https://grok.com/rest/grok/credits");
});

test("rejects Grok HTTP refreshes for any endpoint other than credits", () => {
  const validation = validateLaunchRequest(
    request({ action: "httpRefreshUsage", headless: true, service: "grok" }),
  );

  assert.deepEqual(validation, { ok: false, code: "invalid_url" });
});

test("parses sanitized Grok weekly credits without exposing the response body", () => {
  const parsed = parseGrokCreditsBody(
    JSON.stringify({
      config: {
        currentPeriod: { billingPeriodEnd: "2026-07-16T00:00:00Z" },
        creditUsagePercent: 28.5,
        productUsage: [
          { product: "PRODUCT_GROK_BUILD", usagePercent: 42 },
          { product: "PRODUCT_API", usagePercent: 3 },
        ],
      },
    }),
  );

  assert.equal(parsed.pageState, "usage");
  assert.deepEqual(parsed.usage, {
    remainingPercent: 71.5,
    resetAt: "2026-07-16T00:00:00Z",
    usedPercent: 28.5,
    visibleFields: ["used_percent", "remaining_percent", "quota_window", "reset_at"],
    weekly: null,
    fable: null,
    products: [
      { product: "PRODUCT_GROK_BUILD", usagePercent: 42 },
      { product: "PRODUCT_API", usagePercent: 3 },
    ],
  });
  assert.equal(JSON.stringify(parsed).includes("currentPeriod"), false);
});

test("treats an absent Grok credit percentage as zero used", () => {
  const usage = extractGrokCreditsUsage({ config: {} });

  assert.equal(usage.usedPercent, 0);
  assert.equal(usage.remainingPercent, 100);
});

test("classifies a Grok credits payload without config as unexpected UI", () => {
  assert.equal(parseGrokCreditsBody("{}").pageState, "unexpected_ui");
});

test("classifies an HTML Grok credits body as logged out", () => {
  assert.equal(parseGrokCreditsBody("<html><body>Sign in</body></html>").pageState, "logged_out");
});

test("rejects visible http usage refresh requests", () => {
  const validation = validateLaunchRequest(
    request({ action: "httpRefreshUsage", service: "ollama", headless: false }),
  );

  assert.deepEqual(validation, { ok: false, code: "headless_mode_required" });
});

test("http refresh reports logged_out when no session has been harvested", async () => {
  const result = await runLaunchRequest(
    request({
      action: "httpRefreshUsage",
      service: "ollama",
      headless: true,
      url: "https://ollama.com/settings",
      userDataDir: "/tmp/pickgauge-nonexistent-profile-xyz",
    }),
  );

  assert.equal(result.status, "checked");
  assert.equal(result.pageState, "logged_out");
  assert.equal(result.visibleFields.length, 0);
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

test("parses the ollama session window as the headline gauge with weekly secondary", () => {
  const usage = extractOllamaUsageFromHtml(
    ollamaUsageHtml({
      session: { percent: 1, resetAt: "2026-06-19T18:00:00Z" },
      weekly: { percent: 37.5, resetAt: "2026-06-22T00:00:00Z" },
    }),
  );

  assert.deepEqual(usage, {
    remainingPercent: 99,
    resetAt: "2026-06-19T18:00:00Z",
    usedPercent: 1,
    visibleFields: ["used_percent", "remaining_percent", "reset_at", "quota_window"],
    weekly: {
      remainingPercent: 62.5,
      resetAt: "2026-06-22T00:00:00Z",
      usedPercent: 37.5,
    },
    fable: null,
  });
});

test("falls back to the ollama weekly window when the session meter is absent", () => {
  const usage = extractOllamaUsageFromHtml(
    ollamaUsageHtml({ weekly: { percent: 37.5, resetAt: "2026-06-22T00:00:00Z" } }),
  );

  assert.equal(usage.usedPercent, 37.5);
  assert.equal(usage.remainingPercent, 62.5);
  assert.equal(usage.resetAt, "2026-06-22T00:00:00Z");
  assert.equal(usage.weekly, null);
});

test("returns no ollama usage fields when the meters are missing", () => {
  const usage = extractOllamaUsageFromHtml("<html><body>no meters</body></html>");

  assert.deepEqual(usage.visibleFields, []);
  assert.equal(usage.usedPercent, null);
  assert.equal(usage.remainingPercent, null);
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
