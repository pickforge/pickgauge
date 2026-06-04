#!/usr/bin/env node

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const BACKEND_ID = "playwright-headed-chromium-sidecar";
export const PROTOCOL_VERSION = 1;

const allowedServices = new Set(["codex", "claude"]);
const allowedActions = new Set(["launchLogin", "refreshUsage"]);
const navigationTimeoutMs = 30_000;

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

  if (input.action === "launchLogin" && input.headless !== false) {
    return rejected("headed_mode_required");
  }

  if (input.action === "refreshUsage" && input.headless !== true) {
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
    const pageState = await detectPageState(page, visibleUsage, existingCookieCount);

    return {
      ...sanitizedAcceptedResponse(request),
      status: "checked",
      pageState,
      remainingPercent: pageState === "usage" ? visibleUsage.remainingPercent : null,
      resetAt: pageState === "usage" ? visibleUsage.resetAt : null,
      usedPercent: pageState === "usage" ? visibleUsage.usedPercent : null,
      visibleFields: pageState === "usage" ? visibleUsage.visibleFields : [],
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
  };
}

function emptyVisibleUsage() {
  return {
    remainingPercent: null,
    resetAt: null,
    usedPercent: null,
    visibleFields: [],
  };
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

async function serviceCookieCount(context, url) {
  return (await context.cookies(url).catch(() => [])).length;
}

async function urlLooksLoggedOut(page) {
  try {
    const url = new URL(page.url());
    const host = url.hostname.toLowerCase();
    const path = url.pathname.toLowerCase();

    return (
      host.includes("auth") ||
      host.includes("login") ||
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
