#!/usr/bin/env node

// Dev helper: capture branded-UI screenshots of every view in browser
// preview mode. Usage: node scripts/capture-preview-screenshots.mjs
// (expects `bun run dev` already listening on port 1420).

import { mkdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = resolve(repoRoot, ".playwright-mcp/preview-shots");
const port = Number(process.env.PICKGAUGE_BROWSER_PREVIEW_PORT ?? 1420);
const baseUrl = `http://127.0.0.1:${port}/`;

await mkdir(outDir, { recursive: true });

const browser = await chromium.launch();
const context = await browser.newContext({
  viewport: { width: 1000, height: 700 },
  deviceScaleFactor: 2,
});
const page = await context.newPage();

async function shoot(name, url, ready) {
  await page.goto(url, { waitUntil: "networkidle" });
  await ready?.();
  await page.waitForTimeout(1100);
  await page.screenshot({ path: resolve(outDir, `${name}.png`) });
  console.log(`captured ${name}.png`);
}

await shoot("dashboard", baseUrl, () =>
  page.locator("article.usage-card").first().waitFor(),
);
await shoot("dashboard-official", `${baseUrl}?previewState=official-usage`, () =>
  page.locator("article.usage-card").first().waitFor(),
);
await shoot("history", baseUrl, async () => {
  await page.locator("article.usage-card").first().waitFor();
  await page.getByRole("button", { name: "History" }).click();
});
await shoot("settings", baseUrl, async () => {
  await page.locator("article.usage-card").first().waitFor();
  await page.getByRole("button", { name: "Settings" }).click();
});

await page.setViewportSize({ width: 184, height: 64 });
await shoot("float", `${baseUrl}?window=float`);

await browser.close();
