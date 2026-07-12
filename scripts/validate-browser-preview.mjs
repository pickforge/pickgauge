#!/usr/bin/env node

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const port = Number(process.env.PICKGAUGE_BROWSER_PREVIEW_PORT ?? 1420);
const baseUrl = `http://127.0.0.1:${port}/`;
const viewports = [
  { label: "desktop", width: 1000, height: 700 },
  { label: "minimum", width: 820, height: 600 },
  { label: "compact", width: 680, height: 600 },
  { label: "narrow", width: 390, height: 900 },
];
const previewStates = [
  { state: "default", notes: [] },
  { state: "official-usage", notes: [], startLoginVisibleAfterOptIn: false },
  { state: "missing-local-data", notes: ["No usage data found"] },
  { state: "network-unavailable", notes: ["Network unavailable"] },
  { state: "expired-login", notes: ["Login required"] },
  { state: "mfa-required", notes: ["MFA required"] },
  { state: "captcha-or-bot-check", notes: ["Additional verification required"] },
  { state: "unexpected-ui", notes: ["Unexpected usage page"] },
  { state: "timed-out", notes: ["Usage refresh timed out"] },
  { state: "parse-failed", notes: ["Usage data could not be parsed"] },
  { state: "stale-data", notes: ["Stale data"] },
  { state: "provider-unavailable", notes: ["Provider unavailable"] },
  { state: "permission-denied", notes: ["Usage data is not readable"] },
  { state: "unsafe-profile-path", notes: ["Profile path blocked"] },
  { state: "provider-disabled", notes: ["Provider disabled"] },
];

const server = startViteServer();

try {
  await waitForServer(baseUrl, server);
  const browser = await chromium.launch();

  try {
    for (const viewport of viewports) {
      for (const previewState of previewStates) {
        await validatePreviewState(browser, viewport, previewState);
      }
    }

    await validateDesktopOnlyControlFallbacks(browser);
  } finally {
    await browser.close();
  }

  console.log("Browser preview validation passed for four-provider responsive checks");
} finally {
  await stopServer(server);
}

function startViteServer() {
  const child = spawn("bun", ["run", "dev", "--strictPort"], {
    cwd: repoRoot,
    detached: process.platform !== "win32",
    env: {
      ...process.env,
      BROWSER: "none",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  const output = [];

  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => output.push(chunk));
  child.stderr.on("data", (chunk) => output.push(chunk));

  return { child, output };
}

async function waitForServer(url, server) {
  const started = Date.now();

  while (Date.now() - started < 20_000) {
    if (server.child.exitCode !== null) {
      throw new Error(`Vite dev server exited before validation:\n${server.output.join("")}`);
    }

    if (await serverResponds(url)) {
      return;
    }

    await delay(250);
  }

  throw new Error(`Timed out waiting for Vite dev server:\n${server.output.join("")}`);
}

async function serverResponds(url) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 500);

  try {
    const response = await fetch(url, { signal: controller.signal });
    return response.ok;
  } catch {
    return false;
  } finally {
    clearTimeout(timeout);
  }
}

async function validatePreviewState(browser, viewport, previewState) {
  const context = await browser.newContext({
    viewport: { width: viewport.width, height: viewport.height },
  });
  const page = await context.newPage();
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  const url =
    previewState.state === "default"
      ? baseUrl
      : `${baseUrl}?previewState=${previewState.state}`;

  try {
    await page.goto(url, { waitUntil: "domcontentloaded" });
    await page
      .locator("article.usage-card")
      .first()
      .waitFor({ timeout: 10_000 })
      .catch((error) => {
        throw new Error(
          `${viewport.label} ${previewState.state} did not render a usage card: ${error.message}; page errors: ${pageErrors.join(" | ") || "none"}`,
        );
      });

    assert.equal(await page.title(), "PickGauge");
    assert.equal(
      await page.locator("article.usage-card").count(),
      4,
      `${viewport.label} ${previewState.state} should render all usage cards`,
    );
    await assertVisibleText(page, "Codex");
    await assertVisibleText(page, "Claude Code");
    await assertVisibleText(page, "Grok");
    await assertVisibleText(page, "Ollama");
    await assertVisibleText(page, "Remaining usage");

    for (const note of previewState.notes) {
      assert.equal(
        await page.getByText(note, { exact: true }).count(),
        4,
        `${viewport.label} ${previewState.state} should render ${note} for all services`,
      );
    }

    if (previewState.startLoginVisibleAfterOptIn === false) {
      assert.equal(
        await page.getByRole("button", { name: "Start Codex login" }).count(),
        0,
        `${viewport.label} ${previewState.state} should not show Start login`,
      );
    }

    await assertNoHorizontalOverflow(page, viewport, previewState.state);
  } finally {
    await context.close();
  }
}

async function validateDesktopOnlyControlFallbacks(browser) {
  const context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
  });
  const page = await context.newPage();

  try {
    await page.goto(baseUrl, { waitUntil: "domcontentloaded" });
    await page.locator("article.usage-card").first().waitFor();

    const officialRefresh = page.getByRole("button", {
      name: "Refresh official Codex usage",
    });
    const defaultStartLogin = page.getByRole("button", {
      name: "Start Codex login",
    });
    const codexProfile = page.getByLabel("Codex profile");

    assert.equal(await officialRefresh.isDisabled(), true);
    assert.equal(await defaultStartLogin.count(), 0);

    await openSettingsView(page);
    assert.equal(await codexProfile.isDisabled(), true);

    const expiredLoginStart = await validateStartLoginPrompt(page, "expired-login", {
      expectedVisible: true,
    });
    assert.equal(await expiredLoginStart?.isDisabled(), true);

    await validateStartLoginPrompt(page, "mfa-required", { expectedVisible: true });
    await validateStartLoginPrompt(page, "captcha-or-bot-check", { expectedVisible: true });
    await validateStartLoginPrompt(page, "network-unavailable", { expectedVisible: false });
    await validateStartLoginPrompt(page, "unexpected-ui", { expectedVisible: false });

    await assertNoHorizontalOverflow(page, { label: "desktop", width: 1280 }, "controls");
  } finally {
    await context.close();
  }
}

async function openSettingsView(page) {
  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByLabel("Official web readings").waitFor({ state: "attached" });
}

async function validateStartLoginPrompt(page, state, { expectedVisible }) {
  await page.goto(`${baseUrl}?previewState=${state}`, { waitUntil: "domcontentloaded" });
  await page.locator("article.usage-card").first().waitFor();

  const startLogin = page.getByRole("button", {
    name: "Start Codex login",
  });

  if (!expectedVisible) {
    assert.equal(await startLogin.count(), 0, `${state} should not show Start login`);
    return null;
  }

  assert.equal(await startLogin.isDisabled(), true, `${state} login prompt should start disabled`);
  return startLogin;
}

async function assertVisibleText(page, text) {
  const locator = page.getByText(text, { exact: true }).first();
  await locator.waitFor();
  assert.equal(await locator.isVisible(), true, `${text} should be visible`);
}

async function assertNoHorizontalOverflow(page, viewport, state) {
  const overflow = await page.evaluate(() => {
    const documentWidth = Math.max(
      document.documentElement.scrollWidth,
      document.body.scrollWidth,
    );
    const offenders = Array.from(document.body.querySelectorAll("*"))
      .map((element) => {
        const rect = element.getBoundingClientRect();
        return {
          tag: element.tagName.toLowerCase(),
          className:
            typeof element.className === "string" ? element.className.slice(0, 80) : "",
          left: Math.floor(rect.left),
          right: Math.ceil(rect.right),
          width: Math.ceil(rect.width),
        };
      })
      .filter(
        (rect) =>
          rect.width > 0 &&
          (rect.left < -1 || rect.right > document.documentElement.clientWidth + 1),
      )
      .slice(0, 5);

    return {
      documentWidth,
      viewportWidth: document.documentElement.clientWidth,
      offenders,
    };
  });

  assert.ok(
    overflow.documentWidth <= overflow.viewportWidth + 1,
    `${viewport.label} ${state} document overflows horizontally: ${JSON.stringify(overflow)}`,
  );
  assert.deepEqual(
    overflow.offenders,
    [],
    `${viewport.label} ${state} has horizontally overflowing elements`,
  );
}

async function stopServer(server) {
  if (server.child.exitCode !== null || server.child.signalCode !== null) {
    return;
  }

  const pid = server.child.pid;

  if (pid) {
    try {
      process.kill(process.platform === "win32" ? pid : -pid, "SIGTERM");
    } catch {
    }
  }

  if (await waitForProcessExit(server.child, 3_000)) {
    return;
  }

  if (pid) {
    try {
      process.kill(process.platform === "win32" ? pid : -pid, "SIGKILL");
    } catch {
    }
  }

  await waitForProcessExit(server.child, 1_000);
}

function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}

function waitForProcessExit(child, milliseconds) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return Promise.resolve(true);
  }

  return new Promise((resolveExit) => {
    const timeout = setTimeout(() => resolveExit(false), milliseconds);
    child.once("exit", () => {
      clearTimeout(timeout);
      resolveExit(true);
    });
  });
}
