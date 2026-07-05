#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { arch, platform, release } from "node:os";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const packageJson = readJson(resolve(repoRoot, "package.json"));
const tauriConfig = readJson(resolve(repoRoot, "src-tauri/tauri.conf.json"));
const appVersion = tauriConfig?.version ?? packageJson?.version ?? "unknown";
const appImagePath = resolve(
  repoRoot,
  `src-tauri/target/release/bundle/appimage/PickGauge_${appVersion}_amd64.AppImage`,
);
const sidecarPath = resolve(
  repoRoot,
  "src-tauri/binaries/pickgauge-playwright-sidecar-x86_64-unknown-linux-gnu",
);
const readmePath = resolve(repoRoot, "README.md");
const releaseWorkflowPath = resolve(repoRoot, ".github/workflows/release.yml");
const appSveltePath = resolve(repoRoot, "src/App.svelte");
const displayHelperPath = resolve(repoRoot, "src/lib/display.ts");
const displayTestPath = resolve(repoRoot, "src/lib/display.test.ts");
const browserPreviewValidationPath = resolve(repoRoot, "scripts/validate-browser-preview.mjs");
const rustAppPath = resolve(repoRoot, "src-tauri/src/lib.rs");
const officialUsageUrls = [
  "https://chatgpt.com/codex/cloud/settings/analytics",
  "https://claude.ai/new#settings/usage",
];
const sensitiveOutputPattern =
  /\b(set-cookie|cookie:|authorization:|bearer\s+[A-Za-z0-9._~+/-]+=*|access[_-]?token|refresh[_-]?token|id[_-]?token|session[_-]?(id|token)|csrf)\b/iu;
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
    statusNotifierHostRegistered: statusNotifierHostRegistered(),
  },
  smokeDependencies: {
    kdeTray: commandAvailability(["qdbus", "gdbus", "xdotool", "xprop", "xmessage"]),
  },
  artifacts: {
    appImage: fileMetadata(appImagePath),
    playwrightSidecar: fileMetadata(sidecarPath),
  },
  releaseReadiness: {
    platformArtifactsConfigured: {
      linuxAppImage: fileContainsAll(releaseWorkflowPath, ["linux-appimage", "*.AppImage"]),
      windows: fileContainsAll(releaseWorkflowPath, ["windows", "*.msi", "*.exe"]),
      macosIntel: fileContainsAll(releaseWorkflowPath, ["macos-intel", "*.dmg"]),
      macosAppleSilicon: fileContainsAll(releaseWorkflowPath, ["macos-apple-silicon", "*.dmg"]),
    },
    windowsMacosUntestedCaveat: {
      readme: fileContainsAll(readmePath, ["Windows and macOS", "untested"]),
      releaseWorkflowNotes: fileContainsAll(releaseWorkflowPath, [
        "Windows and macOS",
        "untested",
      ]),
    },
    platformRuntimeSmokeStillRequired: true,
  },
  loginVisibilityAutomation: {
    refreshOfficialRemainsHeadless: fileContainsAll(rustAppPath, [
      "refreshUsage",
      "headless",
      "web_usage_refresh_sidecar_request",
    ]),
    startLoginHasHeadlessPreflight: fileContainsAll(rustAppPath, [
      "headless_web_usage_response",
      "login_start_preflight_outcome",
    ]),
    authenticatedPreflightSkipsHeadedLaunch: fileContainsAll(rustAppPath, [
      "LOGIN_STATUS_ALREADY_AUTHENTICATED",
      "launch_headed_browser: false",
    ]),
    successfulPreflightUpdatesSnapshots: fileContainsAll(rustAppPath, [
      "refresh_web_provider_preflight_response",
      "SNAPSHOTS_UPDATED_EVENT",
    ]),
    loginPreflightSnapshotRecordingIsBestEffort: fileContainsAll(rustAppPath, [
      "login_start_preflight_outcome_from_response",
      "let _ = refresh_web_provider_preflight_response",
    ]),
    loginPreflightOutcomeBeforeSnapshotParse: fileContainsAll(rustAppPath, [
      "login_start_preflight_outcome_from_response_uses_page_state_before_snapshot_parse_result",
      "invalid_visible_percentage",
    ]),
    frontendHidesStartLoginForOfficialUsage: fileContainsAll(browserPreviewValidationPath, [
      "official-usage",
      "should keep Start login hidden after opt-in",
    ]),
    frontendClearsStaleLoginStatus: fileContainsAll(appSveltePath, [
      "loginStatusClearedBySnapshots",
      "loginStatusService",
    ]),
    frontendRequiresWebEvidenceToClearLoginStatus: fileContainsAll(displayHelperPath, [
      "loginStatusClearedBySnapshots",
      "snapshot.details.webStatus !== undefined",
    ]),
    frontendRegressionCoveragePresent: fileContainsAll(displayTestPath, [
      "ignores fallback login prompts",
      "keeps login status messages when matching snapshots have no web evidence",
    ]),
  },
  manualEvidence: {
    recordObservedKdeTrayBehavior: true,
    recordAuthenticatedLoginOutcomeSeparately: true,
    recordWindowsMacosSmokeSeparately: true,
    templates: {
      kdeTray: manualEvidenceTemplate(
        [
          "date",
          "desktopSession",
          "osSessionType",
          "artifactUsed",
          "trayVisible",
          "trayClickOpensPopup",
          "popupCloseOrFallback",
          "settingsPersistAfterRestart",
          "quitExitsApp",
          "automatedSmokeDependenciesAvailable",
        ],
        [
          "pending_user_visible_tray_observation",
          "pending_user_visible_popup_observation",
          "pending_user_visible_settings_observation",
        ],
      ),
      authenticatedWeb: manualEvidenceTemplate(
        [
          "date",
          "service",
          "profileLabel",
          "loginPromptShownOnlyWhenNeeded",
          "headedLoginOpenedOnlyAfterUserActionState",
          "silentPreflightClearsStaleLoginPrompt",
          "localOnlyRefreshDoesNotClearNeededLoginPrompt",
          "headlessRefreshReachedUsage",
          "visibleFieldsObserved",
          "savedCredentialArtifactsAbsent",
          "sensitiveLogsAbsent",
        ],
        [
          "pending_authenticated_codex_profile",
          "pending_authenticated_claude_profile",
          "pending_saved_credential_absence_check",
        ],
      ),
      platformSmoke: manualEvidenceTemplate(
        [
          "date",
          "platform",
          "artifactUsed",
          "installedOrLaunched",
          "trayOrMenuVisible",
          "settingsPersistAfterRestart",
          "quitExitsApp",
        ],
        [
          "pending_windows_runtime",
          "pending_macos_intel_runtime",
          "pending_macos_apple_silicon_runtime",
        ],
      ),
    },
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

verifySanitizedPreflight(serialized);

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

function commandAvailability(commands) {
  return commands.reduce((availability, command) => {
    availability[command] = commandAvailable(command);
    return availability;
  }, {});
}

function commandAvailable(command) {
  return (process.env.PATH ?? "").split(":").some((directory) => {
    try {
      const stats = statSync(resolve(directory, command));

      return stats.isFile() && (stats.mode & 0o111) !== 0;
    } catch {
      return false;
    }
  });
}

function statusNotifierHostRegistered() {
  if (!commandAvailable("qdbus")) {
    return null;
  }

  try {
    return (
      execFileSync(
        "qdbus",
        [
          "org.kde.StatusNotifierWatcher",
          "/StatusNotifierWatcher",
          "org.freedesktop.DBus.Properties.Get",
          "org.kde.StatusNotifierWatcher",
          "IsStatusNotifierHostRegistered",
        ],
        {
          encoding: "utf8",
          stdio: ["ignore", "pipe", "ignore"],
        },
      ).trim() === "true"
    );
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

function manualEvidenceTemplate(requiredFields, pendingObservations) {
  return {
    status: "pending_manual_observation",
    requiredFields,
    pendingObservations,
    keepOnlySanitizedNotes: true,
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

function fileContainsAll(path, fragments) {
  try {
    const content = readFileSync(path, "utf8");

    return fragments.every((fragment) => content.includes(fragment));
  } catch {
    return false;
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

function verifySanitizedPreflight(output) {
  for (const fragment of [
    process.env.HOME,
    repoRoot,
    appImagePath,
    sidecarPath,
    readmePath,
    releaseWorkflowPath,
    appSveltePath,
    displayHelperPath,
    displayTestPath,
    browserPreviewValidationPath,
    rustAppPath,
    ...officialUsageUrls,
  ].filter(Boolean)) {
    if (output.includes(fragment)) {
      throw new Error("Preflight output leaked sensitive local or provider data");
    }
  }

  if (sensitiveOutputPattern.test(output)) {
    throw new Error("Preflight output leaked auth material");
  }

  if (/<!doctype|<html|<body|<script/iu.test(output)) {
    throw new Error("Preflight output leaked page markup");
  }
}
