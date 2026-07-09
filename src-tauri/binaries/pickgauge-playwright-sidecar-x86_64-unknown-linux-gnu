#!/usr/bin/env node

import { chmodSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const BACKEND_ID = "playwright-headed-chromium-sidecar";
export const PROTOCOL_VERSION = 1;

const allowedServices = new Set(["codex", "claude", "grok", "ollama"]);
const allowedActions = new Set(["launchLogin", "refreshUsage", "httpRefreshUsage"]);
const navigationTimeoutMs = 30_000;
const ollamaHttpUserAgent =
  "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36";
const grokProducts = new Set([
  "PRODUCT_GROK_CHAT",
  "PRODUCT_GROK_BUILD",
  "PRODUCT_API",
  "PRODUCT_GROK_IMAGINE",
  "PRODUCT_GROK_VOICE",
  "PRODUCT_GROK_PLUGINS",
]);

export function validateLaunchRequest(input) {
  if (!input || typeof input !== "object" || Array.isArray(input)) {
    return rejected("invalid_request");
  }

  if (input.protocolVersion !== PROTOCOL_VERSION) {
    return rejected("unsupported_protocol_version");
  }

  if (!allowedActions.has(input.action)) {
    return rejected("unsupported_action");
  }

  if (input.backend !== BACKEND_ID) {
    return rejected("unsupported_backend");
  }

  if (!allowedServices.has(input.service)) {
    return rejected("unsupported_service");
  }

  if (typeof input.profileLabel !== "string" || input.profileLabel.length === 0) {
    return rejected("invalid_profile_label");
  }

  if (typeof input.userDataDir !== "string" || input.userDataDir.length === 0) {
    return rejected("invalid_user_data_dir");
  }

  if (typeof input.url !== "string" || !isHttpsUrl(input.url)) {
    return rejected("invalid_url");
  }

  if (
    input.service === "grok" &&
    input.url !==
      (input.action === "httpRefreshUsage"
        ? "https://grok.com/rest/grok/credits"
        : "https://grok.com/")
  ) {
    return rejected("invalid_url");
  }

  if (input.action === "launchLogin" && input.headless !== false) {
    return rejected("headed_mode_required");
  }

  if (
    (input.action === "refreshUsage" || input.action === "httpRefreshUsage") &&
    input.headless !== true
  ) {
    return rejected("headless_mode_required");
  }

  if (!Array.isArray(input.args) || !input.args.every((arg) => typeof arg === "string")) {
    return rejected("invalid_args");
  }

  if (input.args.some((arg) => arg.startsWith("--user-data-dir="))) {
    return rejected("user_data_dir_arg_forbidden");
  }

  return {
    ok: true,
    request: {
      action: input.action,
      args: input.args,
      backend: input.backend,
      headless: input.headless,
      profileLabel: input.profileLabel,
      service: input.service,
      url: input.url,
      userDataDir: input.userDataDir,
    },
  };
}

export function sanitizedAcceptedResponse(request) {
  return {
    ok: true,
    status: "accepted",
    protocolVersion: PROTOCOL_VERSION,
    action: request.action,
    backend: request.backend,
    service: request.service,
    profileLabel: request.profileLabel,
    headless: request.headless,
    argCount: request.args.length,
  };
}

export function sanitizedRejectedResponse(code) {
  return {
    ok: false,
    status: "rejected",
    protocolVersion: PROTOCOL_VERSION,
    code,
  };
}

export async function runLaunchRequest(input, { dryRun = false } = {}) {
  const validation = validateLaunchRequest(input);

  if (!validation.ok) {
    return sanitizedRejectedResponse(validation.code);
  }

  const { request } = validation;

  if (dryRun) {
    return sanitizedAcceptedResponse(request);
  }

  let context;

  try {
    if (request.action === "httpRefreshUsage") {
      return await runHttpRefreshUsageRequest(request);
    }

    if (request.action === "refreshUsage") {
      return await runRefreshUsageRequest(request);
    }

    const { chromium } = await import("playwright");
    context = await chromium.launchPersistentContext(request.userDataDir, {
      args: request.args,
      headless: request.headless,
      timeout: navigationTimeoutMs,
    });
    const page = context.pages()[0] ?? (await context.newPage());
    await page.goto(request.url, {
      waitUntil: "domcontentloaded",
      timeout: navigationTimeoutMs,
    });

    return {
      ...sanitizedAcceptedResponse(request),
      status: "launched",
    };
  } catch {
    if (context) {
      await context.close().catch(() => {});
    }

    return sanitizedRejectedResponse("sidecar_launch_failed");
  }
}

async function runRefreshUsageRequest(request) {
  let context;

  try {
    const { chromium } = await import("playwright");
    context = await chromium.launchPersistentContext(request.userDataDir, {
      args: request.args,
      headless: true,
      timeout: navigationTimeoutMs,
    });
    const page = context.pages()[0] ?? (await context.newPage());
    const existingCookieCount = await serviceCookieCount(context, request.url);
    await page.goto(request.url, {
      waitUntil: "domcontentloaded",
      timeout: navigationTimeoutMs,
    });
    await page.waitForLoadState("networkidle", { timeout: 5_000 }).catch(() => {});

    const visibleUsage = await extractVisibleUsage(page, request.service);
    const pageState =
      request.service === "grok"
        ? await detectGrokPageState(page, existingCookieCount)
        : await detectPageState(page, visibleUsage, existingCookieCount);

    if ((request.service === "grok" || request.service === "ollama") && pageState === "usage") {
      // Harvest the authenticated session so later refreshes can skip Chromium
      // and fetch the page over plain HTTP. The cookie never leaves the sidecar.
      await persistSession(context, request);
    }

    return {
      ...sanitizedAcceptedResponse(request),
      status: "checked",
      pageState,
      remainingPercent: pageState === "usage" ? visibleUsage.remainingPercent : null,
      resetAt: pageState === "usage" ? visibleUsage.resetAt : null,
      usedPercent: pageState === "usage" ? visibleUsage.usedPercent : null,
      visibleFields: pageState === "usage" ? visibleUsage.visibleFields : [],
      weekly: pageState === "usage" ? visibleUsage.weekly : null,
      products: pageState === "usage" ? (visibleUsage.products ?? []) : [],
    };
  } catch (error) {
    if (context) {
      return sanitizedCheckedResponse(request, refreshFailurePageState(error));
    }

    return sanitizedRejectedResponse("sidecar_refresh_failed");
  } finally {
    if (context) {
      await context.close().catch(() => {});
    }
  }
}

function sanitizedCheckedResponse(request, pageState, visibleUsage = emptyVisibleUsage()) {
  return {
    ...sanitizedAcceptedResponse(request),
    status: "checked",
    pageState,
    remainingPercent: pageState === "usage" ? visibleUsage.remainingPercent : null,
    resetAt: pageState === "usage" ? visibleUsage.resetAt : null,
    usedPercent: pageState === "usage" ? visibleUsage.usedPercent : null,
    visibleFields: pageState === "usage" ? visibleUsage.visibleFields : [],
    weekly: pageState === "usage" ? visibleUsage.weekly : null,
    products: pageState === "usage" ? (visibleUsage.products ?? []) : [],
  };
}

function emptyVisibleUsage() {
  return {
    remainingPercent: null,
    resetAt: null,
    usedPercent: null,
    visibleFields: [],
    weekly: null,
    products: [],
  };
}

// Lightweight refresh: replay the harvested session cookie over plain HTTP,
// with no Chromium launched. A missing or expired session reports `logged_out`
// so the caller falls back to the browser.
async function runHttpRefreshUsageRequest(request) {
  if (request.service !== "grok" && request.service !== "ollama") {
    return sanitizedCheckedResponse(request, "unexpected_ui");
  }

  const cookies = readSession(request);

  if (cookies === null) {
    return sanitizedCheckedResponse(request, "logged_out");
  }

  let response;

  try {
    response = await fetch(request.url, {
      headers:
        request.service === "grok"
          ? {
              accept: "application/json",
              cookie: cookies.map((cookie) => `${cookie.name}=${cookie.value}`).join("; "),
            }
          : {
              accept: "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
              "accept-language": "en-US,en;q=0.9",
              cookie: cookies.map((cookie) => `${cookie.name}=${cookie.value}`).join("; "),
              "user-agent": ollamaHttpUserAgent,
            },
      redirect: "manual",
    });
  } catch (error) {
    return sanitizedCheckedResponse(request, refreshFailurePageState(error));
  }

  if (response.status >= 300 && response.status < 400) {
    // Redirected to sign-in: the harvested session expired.
    return sanitizedCheckedResponse(request, "logged_out");
  }

  if (request.service === "grok" && (response.status === 401 || response.status === 403)) {
    return sanitizedCheckedResponse(request, "logged_out");
  }

  if (!response.ok) {
    return sanitizedCheckedResponse(request, "unexpected_ui");
  }

  const body = await response.text().catch(() => "");

  if (request.service === "grok") {
    const parsed = parseGrokCreditsBody(body);
    return sanitizedCheckedResponse(request, parsed.pageState, parsed.usage);
  }

  const usage = extractOllamaUsageFromHtml(body);
  const pageState = usage.visibleFields.length > 0 ? "usage" : "unexpected_ui";

  return sanitizedCheckedResponse(request, pageState, usage);
}

function sessionStorePath(request) {
  return `${request.userDataDir}.session.json`;
}

async function persistSession(context, request) {
  try {
    const cookies = await context.cookies(request.url);
    const pairs = cookies
      .filter((cookie) => typeof cookie.name === "string" && typeof cookie.value === "string")
      .map((cookie) => ({ name: cookie.name, value: cookie.value }));

    if (pairs.length === 0) {
      return;
    }

    const path = sessionStorePath(request);
    writeFileSync(path, JSON.stringify(pairs), { mode: 0o600 });
    chmodSync(path, 0o600);
  } catch {
    // A failed harvest just means the next refresh falls back to the browser.
  }
}

function readSession(request) {
  try {
    const path = sessionStorePath(request);

    if (!existsSync(path)) {
      return null;
    }

    const parsed = JSON.parse(readFileSync(path, "utf8"));

    if (!Array.isArray(parsed)) {
      return null;
    }

    const pairs = parsed.filter(
      (entry) => entry && typeof entry.name === "string" && typeof entry.value === "string",
    );

    return pairs.length > 0 ? pairs : null;
  } catch {
    return null;
  }
}

function refreshFailurePageState(error) {
  const message = typeof error?.message === "string" ? error.message : "";

  if (error?.name === "TimeoutError" || /\btimeout\b/iu.test(message)) {
    return "timed_out";
  }

  return "network_unavailable";
}

export async function detectPageState(page, visibleUsage, cookieCount) {
  if (await urlLooksLoggedOut(page)) {
    return "logged_out";
  }

  if (await hasAnyVisibleLocator(captchaLocators(page))) {
    return "captcha_or_bot_check";
  }

  if (await hasAnyVisibleLocator(mfaLocators(page))) {
    return "mfa_required";
  }

  if (await hasAnyVisibleLocator(authGateLocators(page))) {
    return "logged_out";
  }

  if (visibleUsage.visibleFields.length > 0) {
    return "usage";
  }

  if (cookieCount === 0) {
    return "logged_out";
  }

  return "unexpected_ui";
}

async function detectGrokPageState(page, cookieCount) {
  if (await urlLooksLoggedOut(page)) {
    return "logged_out";
  }

  if (await hasAnyVisibleLocator(captchaLocators(page))) {
    return "captcha_or_bot_check";
  }

  if (await hasAnyVisibleLocator(mfaLocators(page))) {
    return "mfa_required";
  }

  if (await hasAnyVisibleLocator(authGateLocators(page))) {
    return "logged_out";
  }

  try {
    const host = new URL(page.url()).hostname.toLowerCase();

    if (host === "grok.com" || host.endsWith(".grok.com")) {
      return "usage";
    }
  } catch {
    return "unexpected_ui";
  }

  return cookieCount === 0 ? "logged_out" : "unexpected_ui";
}

async function serviceCookieCount(context, url) {
  return (await context.cookies(url).catch(() => [])).length;
}

async function urlLooksLoggedOut(page) {
  try {
    const url = new URL(page.url());
    const host = url.hostname.toLowerCase();
    const path = url.pathname.toLowerCase();

    return (
      host === "accounts.x.ai" ||
      host.includes("auth") ||
      host.includes("login") ||
      host.includes("signin") ||
      path.includes("login") ||
      path.includes("signin") ||
      path.includes("sign-in") ||
      path.includes("oauth") ||
      path.includes("authorize")
    );
  } catch {
    return false;
  }
}

function authGateLocators(page) {
  return [
    page.getByRole("button", { name: /\b(log in|sign in|sign up)\b/iu }),
    page.getByRole("link", { name: /\b(log in|sign in|sign up)\b/iu }),
    page.getByRole("button", { name: /\bcontinue with\b/iu }),
    page.getByRole("textbox", { name: /\b(email|phone)\b/iu }),
    page.getByText(/\b(log in|sign in|sign up|continue with|welcome back)\b/iu),
  ];
}

function captchaLocators(page) {
  return [
    page.getByText(/\b(captcha|verify you are human|security check|checking your browser)\b/iu),
  ];
}

function mfaLocators(page) {
  return [
    page.getByText(/\b(two-factor|multi-factor|verification code|authentication code)\b/iu),
  ];
}

async function hasAnyVisibleLocator(locators) {
  for (const locator of locators) {
    const count = await locator.count().catch(() => 0);

    for (let index = 0; index < Math.min(count, 25); index += 1) {
      if (await locator.nth(index).isVisible({ timeout: 500 }).catch(() => false)) {
        return true;
      }
    }
  }

  return false;
}

export async function extractVisibleUsage(page, service) {
  if (service === "ollama") {
    const html = await page.content().catch(() => "");
    return { ...extractOllamaUsageFromHtml(html), service };
  }

  const text = await page.locator("body").innerText({ timeout: 2_000 }).catch(() => "");
  const percentages = visiblePercentages(text);
  const resetAt = visibleResetAt(text);
  const visibleFields = [];

  if (percentages.remainingPercent !== null) {
    visibleFields.push("remaining_percent");
  }

  if (percentages.usedPercent !== null) {
    visibleFields.push("used_percent");
  }

  if (resetAt !== null) {
    visibleFields.push("reset_at");
  }

  if (/\b(plan|pro|team|max|plus)\b/iu.test(text)) {
    visibleFields.push("plan_label");
  }

  if (/\b(day|week|month|hour|reset|renews|window)\b/iu.test(text)) {
    visibleFields.push("quota_window");
  }

  return {
    remainingPercent: percentages.remainingPercent,
    resetAt,
    service,
    usedPercent: percentages.usedPercent,
    visibleFields: [...new Set(visibleFields)],
    weekly: null,
  };
}

export function parseGrokCreditsBody(body) {
  if (typeof body !== "string" || /^\s*</u.test(body)) {
    return { pageState: "logged_out", usage: emptyVisibleUsage() };
  }

  let value;
  try {
    value = JSON.parse(body);
  } catch {
    return { pageState: "unexpected_ui", usage: emptyVisibleUsage() };
  }

  const usage = extractGrokCreditsUsage(value);
  return usage === null
    ? { pageState: "unexpected_ui", usage: emptyVisibleUsage() }
    : { pageState: "usage", usage };
}

export function extractGrokCreditsUsage(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }

  const config = value.config;
  if (!config || typeof config !== "object" || Array.isArray(config)) {
    return null;
  }

  const hasUsage = Object.prototype.hasOwnProperty.call(config, "creditUsagePercent");
  const usedPercent = hasUsage ? config.creditUsagePercent : 0;
  if (typeof usedPercent !== "number" || !validPercent(usedPercent)) {
    return null;
  }

  const period = config.currentPeriod;
  const resetAt =
    period && typeof period === "object" && !Array.isArray(period)
      ? normalizeRfc3339(period.billingPeriodEnd)
      : null;
  const products = Array.isArray(config.productUsage)
    ? config.productUsage.flatMap((entry) => {
        if (
          !entry ||
          typeof entry !== "object" ||
          Array.isArray(entry) ||
          !grokProducts.has(entry.product) ||
          !validPercent(entry.usagePercent)
        ) {
          return [];
        }

        return [{ product: entry.product, usagePercent: entry.usagePercent }];
      })
    : [];

  const visibleFields = ["used_percent", "remaining_percent", "quota_window"];
  if (resetAt !== null) {
    visibleFields.push("reset_at");
  }

  return {
    remainingPercent: 100 - usedPercent,
    resetAt,
    usedPercent,
    visibleFields,
    weekly: null,
    products,
  };
}

function visiblePercentages(text) {
  const remaining = percentNearLabel(text, /\b(remaining|left|available)\b/iu);
  const used = percentNearLabel(text, /\b(used|usage|consumed)\b/iu);
  const firstPercent = firstPercentValue(text);

  return {
    remainingPercent: remaining ?? (used === null ? firstPercent : null),
    usedPercent: used,
  };
}

function percentNearLabel(text, labelPattern) {
  const normalized = text.replace(/\s+/gu, " ");
  const matches = normalized.matchAll(/([0-9]{1,3}(?:\.[0-9]{1,2})?)\s*%/gu);
  let bestMatch = null;

  for (const match of matches) {
    const value = Number.parseFloat(match[1]);
    const start = Math.max(0, match.index - 80);
    const end = Math.min(normalized.length, match.index + match[0].length + 80);
    const distance = closestLabelDistance(normalized.slice(start, end), labelPattern, match.index - start);

    if (
      validPercent(value) &&
      distance !== null &&
      (bestMatch === null || distance < bestMatch.distance)
    ) {
      bestMatch = { distance, value };
    }
  }

  return bestMatch?.value ?? null;
}

function closestLabelDistance(context, labelPattern, percentIndex) {
  const flags = labelPattern.flags.includes("g") ? labelPattern.flags : `${labelPattern.flags}g`;
  const pattern = new RegExp(labelPattern.source, flags);
  let shortest = null;

  for (const match of context.matchAll(pattern)) {
    const distance = Math.abs(match.index - percentIndex);

    if (shortest === null || distance < shortest) {
      shortest = distance;
    }
  }

  return shortest;
}

function firstPercentValue(text) {
  const match = text.match(/\b([0-9]{1,3}(?:\.[0-9]{1,2})?)\s*%/u);

  if (!match) {
    return null;
  }

  const value = Number.parseFloat(match[1]);
  return validPercent(value) ? value : null;
}

function validPercent(value) {
  return Number.isFinite(value) && value >= 0 && value <= 100;
}

function visibleResetAt(text) {
  const iso = text.match(/\b20[0-9]{2}-[01][0-9]-[0-3][0-9]T[0-2][0-9]:[0-5][0-9]/u);

  if (!iso) {
    return null;
  }

  const value = iso[0].endsWith("Z") ? iso[0] : `${iso[0]}:00Z`;
  return Number.isNaN(Date.parse(value)) ? null : value;
}

// Parse the two Ollama usage meters straight from the server-rendered HTML.
// Both the browser refresh (via page.content()) and the lightweight HTTP refresh
// feed this single parser, so there is one extraction code path to maintain.
export function parseOllamaUsageHtml(html) {
  if (typeof html !== "string" || html.length === 0) {
    return null;
  }

  return {
    session: readOllamaMeter(html, "Session usage"),
    weekly: readOllamaMeter(html, "Weekly usage"),
  };
}

function readOllamaMeter(html, prefix) {
  const label = new RegExp(
    `aria-label="${prefix}\\s+([0-9]{1,3}(?:\\.[0-9]+)?)\\s*%\\s*used"`,
    "i",
  ).exec(html);

  if (label === null) {
    return null;
  }

  // The reset timestamp is the first data-time attribute after this meter's
  // label; the next window's block (and its own data-time) appears later.
  const reset = /data-time="([^"]+)"/i.exec(html.slice(label.index + label[0].length));

  return {
    usedPercent: Number.parseFloat(label[1]),
    resetAt: reset === null ? null : reset[1],
  };
}

export function extractOllamaUsageFromHtml(html) {
  const { primary, secondary } = pickOllamaWindows(parseOllamaUsageHtml(html));

  if (primary === null) {
    return emptyVisibleUsage();
  }

  const usedPercent = validPercent(primary.usedPercent) ? primary.usedPercent : null;
  const resetAt = normalizeIsoReset(primary.resetAt);
  const weekly = ollamaWindow(secondary);
  const visibleFields = [];

  if (usedPercent !== null) {
    visibleFields.push("used_percent", "remaining_percent");
  }

  if (resetAt !== null) {
    visibleFields.push("reset_at");
  }

  if (weekly !== null) {
    visibleFields.push("quota_window");
  }

  return {
    remainingPercent: usedPercent === null ? null : 100 - usedPercent,
    resetAt,
    usedPercent,
    visibleFields,
    weekly,
  };
}

// The session window is the headline gauge (it resets in hours, like the other
// services' 5-hour window) and the weekly window is the secondary bar. When the
// session meter is absent the weekly window becomes the headline with no
// secondary, mirroring the previous single-window fallback.
function pickOllamaWindows(meters) {
  if (!meters) {
    return { primary: null, secondary: null };
  }

  const session = validOllamaMeter(meters.session) ? meters.session : null;
  const weekly = validOllamaMeter(meters.weekly) ? meters.weekly : null;

  if (session !== null) {
    return { primary: session, secondary: weekly };
  }

  return { primary: weekly, secondary: null };
}

function validOllamaMeter(window) {
  return (
    Boolean(window) && typeof window.usedPercent === "number" && validPercent(window.usedPercent)
  );
}

// Shape a secondary meter into the window payload the web provider expects,
// dropping it when the percentage is missing or out of range.
function ollamaWindow(window) {
  if (window === null) {
    return null;
  }

  const usedPercent = validPercent(window.usedPercent) ? window.usedPercent : null;

  if (usedPercent === null) {
    return null;
  }

  return {
    remainingPercent: 100 - usedPercent,
    resetAt: normalizeIsoReset(window.resetAt),
    usedPercent,
  };
}

function normalizeIsoReset(value) {
  if (typeof value !== "string") {
    return null;
  }

  const match = value.match(/\b20[0-9]{2}-[01][0-9]-[0-3][0-9]T[0-2][0-9]:[0-5][0-9]/u);

  if (!match) {
    return null;
  }

  const iso = match[0].endsWith("Z") ? match[0] : `${match[0]}:00Z`;
  return Number.isNaN(Date.parse(iso)) ? null : iso;
}

function normalizeRfc3339(value) {
  if (
    typeof value !== "string" ||
    value.length > 64 ||
    !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/u.test(value)
  ) {
    return null;
  }

  return Number.isNaN(Date.parse(value)) ? null : value;
}

function rejected(code) {
  return { ok: false, code };
}

function isHttpsUrl(value) {
  try {
    return new URL(value).protocol === "https:";
  } catch {
    return false;
  }
}

function readStdin() {
  return readFileSync(0, "utf8");
}

function printJsonLine(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

async function main() {
  const dryRun = process.argv.includes("--dry-run");
  let input;

  try {
    input = JSON.parse(readStdin());
  } catch {
    printJsonLine(sanitizedRejectedResponse("invalid_json"));
    process.exitCode = 1;
    return;
  }

  const result = await runLaunchRequest(input, { dryRun });
  printJsonLine(result);

  if (!result.ok) {
    process.exitCode = 1;
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  await main();
}
