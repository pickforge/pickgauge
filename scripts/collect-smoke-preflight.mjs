#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { arch, platform, release } from "node:os";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const appImagePath = resolve(
  repoRoot,
  "src-tauri/target/release/bundle/appimage/ForgeGauge_0.1.0_amd64.AppImage",
);
const sidecarPath = resolve(
  repoRoot,
  "src-tauri/binaries/forgegauge-playwright-sidecar-x86_64-unknown-linux-gnu",
);
const packageJson = readJson(resolve(repoRoot, "package.json"));
const tauriConfig = readJson(resolve(repoRoot, "src-tauri/tauri.conf.json"));
const preflight = {
  generatedAt: new Date().toISOString(),
  git: gitMetadata(),
  app: {
    packageName: packageJson?.name ?? null,
    packageVersion: packageJson?.version ?? null,
    tauriProductName: tauriConfig?.productName ?? null,
    tauriIdentifier: tauriConfig?.identifier ?? null,
  },
  runtime: {
    node: process.version,
    platform: platform(),
    arch: arch(),
    kernelRelease: release(),
    osRelease: osReleaseMetadata(),
    playwrightPackageVersion: packageVersion("playwright"),
  },
  desktopSession: {
    xdgSessionType: safeEnv("XDG_SESSION_TYPE"),
    currentDesktop: safeEnv("XDG_CURRENT_DESKTOP"),
    desktopSession: safeEnv("DESKTOP_SESSION"),
    waylandDisplaySet: Boolean(process.env.WAYLAND_DISPLAY),
    x11DisplaySet: Boolean(process.env.DISPLAY),
  },
  artifacts: {
    appImage: fileMetadata(appImagePath),
    playwrightSidecar: fileMetadata(sidecarPath),
  },
  manualEvidence: {
    recordObservedKdeTrayBehavior: true,
    recordAuthenticatedLoginOutcomeSeparately: true,
    recordWindowsMacosSmokeSeparately: true,
    excludedFromThisPreflight: [
      "cookies",
      "tokens",
      "authHeaders",
      "browserProfileContents",
      "accountIdentifiers",
      "authenticatedPageContent",
      "fullLocalPaths",
    ],
  },
};
const serialized = `${JSON.stringify(preflight, null, 2)}\n`;
const homePath = process.env.HOME;

if (homePath && serialized.includes(homePath)) {
  throw new Error("Preflight output contains the home directory path");
}

process.stdout.write(serialized);

function gitMetadata() {
  const status = git(["status", "--porcelain"]);

  return {
    branch: git(["branch", "--show-current"]),
    commit: git(["rev-parse", "--short", "HEAD"]),
    dirty: status !== null && status.length > 0,
  };
}

function git(args) {
  try {
    return execFileSync("git", args, {
      cwd: repoRoot,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
  } catch {
    return null;
  }
}

function fileMetadata(path) {
  if (!existsSync(path)) {
    return {
      exists: false,
      executable: false,
      path: repoRelative(path),
      sizeBytes: null,
    };
  }

  const stats = statSync(path);

  return {
    exists: true,
    executable: process.platform === "win32" || (stats.mode & 0o111) !== 0,
    path: repoRelative(path),
    sizeBytes: stats.size,
  };
}

function osReleaseMetadata() {
  const values = parseOsRelease();

  return {
    id: values.ID ?? null,
    name: values.NAME ?? null,
    prettyName: values.PRETTY_NAME ?? null,
    versionId: values.VERSION_ID ?? null,
  };
}

function parseOsRelease() {
  if (!existsSync("/etc/os-release")) {
    return {};
  }

  return readFileSync("/etc/os-release", "utf8")
    .split(/\r?\n/u)
    .reduce((values, line) => {
      const match = /^([A-Z0-9_]+)=(.*)$/u.exec(line);

      if (!match) {
        return values;
      }

      values[match[1]] = match[2].replace(/^"|"$/gu, "");
      return values;
    }, {});
}

function packageVersion(name) {
  const packagePath = resolve(repoRoot, "node_modules", name, "package.json");
  const metadata = readJson(packagePath);

  return metadata?.version ?? packageJson?.dependencies?.[name] ?? null;
}

function readJson(path) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch {
    return null;
  }
}

function repoRelative(path) {
  return relative(repoRoot, path).replaceAll("\\", "/");
}

function safeEnv(name) {
  const value = process.env[name];

  if (!value) {
    return null;
  }

  if (!/^[A-Za-z0-9_ .:+-]{1,80}$/u.test(value)) {
    return "<redacted>";
  }

  return value;
}
