#!/usr/bin/env node

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const BACKEND_ID = "playwright-headed-chromium-sidecar";
export const PROTOCOL_VERSION = 1;

const allowedServices = new Set(["codex", "claude"]);
const allowedActions = new Set(["launchLogin"]);

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

  if (input.headless !== false) {
    return rejected("headed_mode_required");
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
    const { chromium } = await import("playwright");
    context = await chromium.launchPersistentContext(request.userDataDir, {
      args: request.args,
      headless: request.headless,
    });
    const page = context.pages()[0] ?? (await context.newPage());
    await page.goto(request.url, { waitUntil: "domcontentloaded" });

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
