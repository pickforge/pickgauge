#!/usr/bin/env node

import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
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
  `forgegauge-playwright-sidecar-${targetTriple}`,
);
const launchArgs = [
  "--disable-save-password-bubble",
  "--disable-password-manager-reauthentication",
  "--disable-features=AutofillServerCommunication",
  "--no-first-run",
];
const validationRoot = mkdtempSync(resolve(tmpdir(), "forgegauge-sidecar-profiles-"));

try {
  const profileRoots = new Map();

  for (const request of [
    {
      service: "codex",
      url: "https://chatgpt.com/codex/cloud/settings/analytics",
      profileLabel: "codex-profile",
      profileRoot: resolve(validationRoot, "codex"),
    },
    {
      service: "claude",
      url: "https://claude.ai/new#settings/usage",
      profileLabel: "claude-profile",
      profileRoot: resolve(validationRoot, "claude"),
    },
  ]) {
    profileRoots.set(request.service, request.profileRoot);
    await validateLaunch(request);
  }

  if (profileRoots.get("codex") === profileRoots.get("claude")) {
    throw new Error("Codex and Claude sidecar validation profiles must be distinct");
  }

  console.log("Playwright sidecar kept Codex and Claude validation profiles isolated");
} finally {
  rmSync(validationRoot, { force: true, recursive: true });
}

async function validateLaunch({ service, url, profileLabel, profileRoot }) {
  assertNonDefaultProfile(profileRoot);
  await runLaunch({ service, url, profileLabel, profileRoot });

  const sentinelPath = resolve(profileRoot, "forgegauge-profile-sentinel.txt");
  writeFileSync(sentinelPath, `${service}\n`);

  await runLaunch({ service, url, profileLabel, profileRoot });

  if (!existsSync(sentinelPath)) {
    throw new Error(`${service} sidecar profile did not persist across relaunch`);
  }

  console.log(`Playwright sidecar persisted ${service} isolated profile across relaunch`);
}

async function runLaunch({ service, url, profileLabel, profileRoot }) {
  const child = spawn(sidecarPath, [], {
    detached: true,
    stdio: ["pipe", "pipe", "pipe"],
  });
  const timeout = setTimeout(() => {
    stopProcessGroup(child.pid);
  }, 30_000);
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
        action: "launchLogin",
        backend: "playwright-headed-chromium-sidecar",
        service,
        url,
        profileLabel,
        userDataDir: profileRoot,
        headless: false,
        args: launchArgs,
      })}\n`,
    );
    child.stdin.end();

    const response = await waitForLaunchResponse(() => stdout);

    if (
      response.ok !== true ||
      response.status !== "launched" ||
      response.service !== service ||
      response.profileLabel !== profileLabel ||
      response.argCount !== launchArgs.length
    ) {
      throw new Error(`Unexpected ${service} launch response: ${JSON.stringify(response)}`);
    }

    if (stdout.includes(profileRoot) || stderr.includes(profileRoot)) {
      throw new Error(`${service} sidecar output leaked the raw profile path`);
    }

    if (!existsSync(profileRoot)) {
      throw new Error(`${service} sidecar did not create the requested profile directory`);
    }

    console.log(`Playwright sidecar launched ${service} with an isolated temporary profile`);
  } finally {
    clearTimeout(timeout);
    stopProcessGroup(child.pid);
    await waitForExit(child);
  }
}

async function waitForLaunchResponse(readStdout) {
  const started = Date.now();

  while (Date.now() - started < 30_000) {
    const line = readStdout()
      .split(/\r?\n/u)
      .find((candidate) => candidate.trim().length > 0);

    if (line) {
      return JSON.parse(line);
    }

    await new Promise((resolveDelay) => setTimeout(resolveDelay, 100));
  }

  throw new Error("Timed out waiting for Playwright sidecar launch response");
}

async function waitForExit(child) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return;
  }

  await new Promise((resolveExit) => {
    const timeout = setTimeout(resolveExit, 3_000);
    child.once("exit", () => {
      clearTimeout(timeout);
      resolveExit();
    });
  });
}

function stopProcessGroup(pid) {
  if (!pid) {
    return;
  }

  try {
    process.kill(-pid, "SIGTERM");
  } catch {
  }
}

function assertNonDefaultProfile(profileRoot) {
  const home = homedir();
  const defaultProfileRoots = [
    resolve(home, ".config/google-chrome"),
    resolve(home, ".config/chromium"),
    resolve(home, ".cache/ms-playwright"),
  ];

  for (const defaultRoot of defaultProfileRoots) {
    if (profileRoot === defaultRoot || profileRoot.startsWith(`${defaultRoot}/`)) {
      throw new Error("Sidecar validation profile must not use a default browser profile");
    }
  }
}
