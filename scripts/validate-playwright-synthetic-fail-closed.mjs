#!/usr/bin/env node

import { execFileSync, spawn } from "node:child_process";
import { createServer } from "node:https";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const targetTriple = execFileSync("rustc", ["--print", "host-tuple"], {
  encoding: "utf8",
}).trim();
const sidecarPath = resolve(
  repoRoot,
  "src-tauri/binaries",
  `pickgauge-playwright-sidecar-${targetTriple}`,
);
const launchTimeoutMs = 30_000;
const stopTimeoutMs = 3_000;
const launchArgs = [
  "--disable-save-password-bubble",
  "--disable-password-manager-reauthentication",
  "--disable-features=AutofillServerCommunication",
  "--no-first-run",
  "--ignore-certificate-errors",
  "--allow-insecure-localhost",
];
const disabledStoragePreferences = {
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
};
const validationRoot = mkdtempSync(resolve(tmpdir(), "pickgauge-synthetic-fail-closed-"));
const syntheticCookieName = "pickgauge_synthetic_session";
const sensitiveOutputPattern =
  /\b(set-cookie|cookie:|authorization:|bearer\s+[A-Za-z0-9._~+/-]+=*|session[_-]?token)\b/iu;
let server = null;
let validationRootCleanupFailed = false;

try {
  const certPaths = generateCertificate(validationRoot);
  server = await startSyntheticServer(certPaths);
  const { port } = server.address();
  const baseUrl = `https://127.0.0.1:${port}`;
  const services = [];

  for (const service of ["codex", "claude"]) {
    services.push(...(await validateService(baseUrl, service)));
  }

  const report = JSON.stringify(
    {
      generatedAt: new Date().toISOString(),
      backend: "playwright-headed-chromium-sidecar",
      targetTriple,
      syntheticServer: {
        protocol: "https",
        host: "127.0.0.1",
      },
      services,
    },
    null,
    2,
  );

  verifySanitizedReport(report, baseUrl);
  console.log(report);
} finally {
  if (server) {
    await closeServer(server);
  }

  rmSync(validationRoot, { force: true, recursive: true });
  validationRootCleanupFailed = existsSync(validationRoot);
}

if (validationRootCleanupFailed) {
  throw new Error("Temporary synthetic fail-closed validation root must be removed");
}

async function validateService(baseUrl, service) {
  const results = [];
  const profileLabel = `${service}-profile`;

  for (const scenario of [
    { expectedPageState: "usage", name: "usage", path: "/usage" },
    { expectedPageState: "logged_out", name: "logged_out", path: "/logged-out" },
    { expectedPageState: "mfa_required", name: "mfa_required", path: "/mfa" },
    {
      expectedPageState: "captcha_or_bot_check",
      name: "captcha_or_bot_check",
      path: "/captcha",
    },
  ]) {
    const profileRoot = resolve(validationRoot, service, scenario.name);
    prepareDisabledStoragePreferences(profileRoot);
    results.push(
      await validateHeadlessRefresh({
        ...scenario,
        profileLabel,
        profileRoot,
        service,
        url: `${baseUrl}${scenario.path}`,
      }),
    );
  }

  const unexpectedProfileRoot = resolve(validationRoot, service, "unexpected_ui");
  prepareDisabledStoragePreferences(unexpectedProfileRoot);
  await seedSyntheticCookie(unexpectedProfileRoot, baseUrl);
  results.push(
    await validateHeadlessRefresh({
      expectedPageState: "unexpected_ui",
      name: "unexpected_ui",
      profileLabel,
      profileRoot: unexpectedProfileRoot,
      service,
      url: `${baseUrl}/unexpected`,
    }),
  );

  return results;
}

async function seedSyntheticCookie(profileRoot, baseUrl) {
  const { chromium } = await import("playwright");
  const context = await chromium.launchPersistentContext(profileRoot, {
    args: launchArgs,
    headless: true,
    timeout: launchTimeoutMs,
  });

  try {
    await context.addCookies([
      {
        expires: Math.floor(Date.now() / 1000) + 3_600,
        name: syntheticCookieName,
        sameSite: "Lax",
        url: baseUrl,
        value: "present",
      },
    ]);

    const cookies = await context.cookies(baseUrl);
    if (!cookies.some((cookie) => cookie.name === syntheticCookieName)) {
      throw new Error("Synthetic authenticated cookie could not be seeded");
    }
  } finally {
    await context.close().catch(() => {});
  }
}

async function validateHeadlessRefresh({
  expectedPageState,
  name,
  profileLabel,
  profileRoot,
  service,
  url,
}) {
  const response = await runSidecarRefresh({ profileLabel, profileRoot, service, url });

  if (
    response.ok !== true ||
    response.status !== "checked" ||
    response.action !== "refreshUsage" ||
    response.service !== service ||
    response.profileLabel !== profileLabel ||
    response.headless !== true ||
    response.argCount !== launchArgs.length
  ) {
    throw new Error(`${service} ${name} returned an unexpected sidecar response`);
  }

  if (response.pageState !== expectedPageState) {
    throw new Error(`${service} ${name} returned ${response.pageState}`);
  }

  return {
    expectedPageState,
    headlessRefresh: true,
    profileLabel,
    sanitizedOutput: true,
    scenario: name,
    service,
    visibleBrowserRequired: false,
  };
}

async function runSidecarRefresh({ profileLabel, profileRoot, service, url }) {
  const child = spawn(sidecarPath, [], {
    detached: true,
    env: sidecarLaunchEnvironment(),
    stdio: ["pipe", "pipe", "pipe"],
  });
  const timeout = setTimeout(() => {
    signalProcessGroup(child.pid, "SIGTERM");
  }, launchTimeoutMs);
  let stdout = "";
  let stderr = "";

  try {
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.stdin.write(
      `${JSON.stringify({
        protocolVersion: 1,
        action: "refreshUsage",
        backend: "playwright-headed-chromium-sidecar",
        service,
        url,
        profileLabel,
        userDataDir: profileRoot,
        headless: true,
        args: launchArgs,
      })}\n`,
    );
    child.stdin.end();

    const response = await waitForSidecarResponse(() => stdout);
    verifySanitizedOutput({ profileRoot, service, stderr, stdout, url });

    return response;
  } finally {
    clearTimeout(timeout);
    await stopProcessGroup(child);
  }
}

async function waitForSidecarResponse(readStdout) {
  const started = Date.now();

  while (Date.now() - started < launchTimeoutMs) {
    const line = readStdout()
      .split(/\r?\n/u)
      .find((candidate) => candidate.trim().length > 0);

    if (line) {
      return JSON.parse(line);
    }

    await new Promise((resolveDelay) => setTimeout(resolveDelay, 100));
  }

  throw new Error("Timed out waiting for Playwright sidecar refresh response");
}

async function waitForExit(child) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return true;
  }

  return new Promise((resolveExit) => {
    const timeout = setTimeout(() => resolveExit(false), stopTimeoutMs);
    child.once("exit", () => {
      clearTimeout(timeout);
      resolveExit(true);
    });
  });
}

async function stopProcessGroup(child) {
  const pid = child.pid;

  if (!pid) {
    return;
  }

  signalProcessGroup(pid, "SIGTERM");

  if (await waitForExit(child)) {
    return;
  }

  signalProcessGroup(pid, "SIGKILL");
  await waitForExit(child);
}

function signalProcessGroup(pid, signal) {
  if (!pid) {
    return;
  }

  try {
    process.kill(-pid, signal);
  } catch {}
}

function prepareDisabledStoragePreferences(profileRoot) {
  const defaultProfileDir = resolve(profileRoot, "Default");
  mkdirSync(defaultProfileDir, { recursive: true, mode: 0o700 });
  writeFileSync(
    resolve(defaultProfileDir, "Preferences"),
    `${JSON.stringify(disabledStoragePreferences, null, 2)}\n`,
    { mode: 0o600 },
  );
}

function sidecarLaunchEnvironment() {
  const env = {
    ...process.env,
  };

  if (!env.PLAYWRIGHT_BROWSERS_PATH) {
    env.PLAYWRIGHT_BROWSERS_PATH = resolve(homedir(), ".cache/ms-playwright");
  }

  return env;
}

function verifySanitizedOutput({ profileRoot, service, stderr, stdout, url }) {
  const output = `${stdout}\n${stderr}`;

  for (const fragment of [profileRoot, url, ...launchArgs]) {
    if (output.includes(fragment)) {
      throw new Error(`${service} synthetic refresh output leaked sensitive launch data`);
    }
  }

  if (sensitiveOutputPattern.test(output)) {
    throw new Error(`${service} synthetic refresh output leaked auth material`);
  }

  if (/<!doctype|<html|<body|<script/iu.test(output)) {
    throw new Error(`${service} synthetic refresh output leaked page markup`);
  }
}

function verifySanitizedReport(report, baseUrl) {
  for (const fragment of [
    validationRoot,
    baseUrl,
    syntheticCookieName,
    ...launchArgs,
    process.env.HOME,
  ].filter(Boolean)) {
    if (report.includes(fragment)) {
      throw new Error("Synthetic fail-closed report leaked sensitive launch data");
    }
  }

  if (sensitiveOutputPattern.test(report)) {
    throw new Error("Synthetic fail-closed report leaked auth material");
  }

  if (/<!doctype|<html|<body|<script/iu.test(report)) {
    throw new Error("Synthetic fail-closed report leaked page markup");
  }
}

function generateCertificate(root) {
  const keyPath = resolve(root, "server.key");
  const certPath = resolve(root, "server.crt");

  execFileSync(
    "openssl",
    [
      "req",
      "-x509",
      "-newkey",
      "rsa:2048",
      "-nodes",
      "-days",
      "1",
      "-subj",
      "/CN=127.0.0.1",
      "-keyout",
      keyPath,
      "-out",
      certPath,
    ],
    { stdio: "ignore" },
  );

  return { certPath, keyPath };
}

async function startSyntheticServer({ certPath, keyPath }) {
  const serverOptions = {
    cert: readFileSync(certPath),
    key: readFileSync(keyPath),
  };
  const localServer = createServer(serverOptions, (request, response) => {
    const page = syntheticPage(request.url ?? "/");
    response.writeHead(200, page.headers);
    response.end(page.body);
  });

  await new Promise((resolveListen) => {
    localServer.listen(0, "127.0.0.1", resolveListen);
  });

  return localServer;
}

async function closeServer(localServer) {
  await new Promise((resolveClose) => {
    localServer.close(resolveClose);
  });
}

function syntheticPage(path) {
  switch (path) {
    case "/usage":
      return htmlPage(
        "Pro plan monthly window. 42% remaining. 58% used. Resets 2026-06-04T18:30.",
        {
          "set-cookie": `${syntheticCookieName}=present; Path=/; SameSite=Lax`,
        },
      );
    case "/logged-out":
      return htmlPage("Welcome back. Sign in to continue.");
    case "/mfa":
      return htmlPage("Enter your verification code to continue.");
    case "/captcha":
      return htmlPage("Verify you are human before continuing.");
    case "/unexpected":
      return htmlPage("Account dashboard loaded, but usage summary is temporarily unavailable.");
    default:
      return htmlPage("Not found");
  }
}

function htmlPage(text, headers = {}) {
  return {
    body: `<!doctype html><html><body><main>${escapeHtml(text)}</main></body></html>`,
    headers: {
      "content-type": "text/html; charset=utf-8",
      ...headers,
    },
  };
}

function escapeHtml(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
