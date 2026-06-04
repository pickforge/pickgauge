import { describe, expect, it } from "vitest";
import {
  confidenceLabels,
  formatPercent,
  lastOfficialCheck,
  localActivitySummary,
  profileInspectionSummary,
  profilePathFromInput,
  profilePathValue,
  serviceLabels,
  snapshotIsStale,
  sourceLabels,
  webProviderControlState,
} from "./display";
import {
  browserPreviewSnapshots,
  browserPreviewStateFromSearch,
  defaultConfig,
  providerStatusMessage,
  type AppConfig,
  type ProviderProfileInspection,
  type UsageSnapshot,
} from "./usage";

function configWithWebEnabled(webEnabled: boolean): AppConfig {
  return {
    ...defaultConfig,
    providers: {
      ...defaultConfig.providers,
      webEnabled,
    },
  };
}

function snapshot(partial: Partial<UsageSnapshot>): UsageSnapshot {
  return {
    service: "codex",
    remainingPercent: null,
    usedPercent: null,
    resetAt: null,
    source: "local",
    confidence: "low",
    lastUpdated: "2026-06-03T12:00:00Z",
    details: {},
    ...partial,
  };
}

function profileInspection(
  partial: Partial<ProviderProfileInspection>,
): ProviderProfileInspection {
  return {
    service: "codex",
    profileLabel: "codex-profile",
    profilePrepared: true,
    credentialStoreFiles: 0,
    autofillStoreFiles: 0,
    symlinkEntries: 0,
    passwordSavingEnabled: false,
    autofillEnabled: false,
    inspectedEntries: 4,
    entryLimitReached: false,
    inspectedAt: "2026-06-04T00:00:00Z",
    ...partial,
  };
}

describe("frontend display formatting", () => {
  it("formats unknown and rounded percentage values", () => {
    expect(formatPercent(null)).toBe("Unknown");
    expect(formatPercent(72.49)).toBe("72%");
    expect(formatPercent(72.5)).toBe("73%");
  });

  it("summarizes local token activity without account-wide precision", () => {
    expect(
      localActivitySummary(
        snapshot({
          details: {
            totalTokens: 1234,
            sessionCount: 2,
            serverToolUseCount: 3,
            modelCount: 1,
          },
        }),
        "en-US",
      ),
    ).toBe("Local activity: 1,234 tokens | 2 sessions | 3 server tool uses | 1 model");
  });

  it("does not summarize web snapshots or calibrated local percentages", () => {
    expect(localActivitySummary(snapshot({ source: "web" }))).toBeNull();
    expect(localActivitySummary(snapshot({ remainingPercent: 82 }))).toBeNull();
  });

  it("formats invalid official check timestamps without rewriting them", () => {
    expect(
      lastOfficialCheck(
        snapshot({
          details: {
            lastOfficialCheckAt: "not-a-timestamp",
          },
        }),
      ),
    ).toBe("not-a-timestamp");
  });
});

describe("frontend confidence and source labels", () => {
  it("keeps service labels stable", () => {
    expect(serviceLabels).toEqual({
      codex: "Codex",
      claude: "Claude Code",
    });
  });

  it("keeps source labels stable", () => {
    expect(sourceLabels).toEqual({
      local: "Local estimate",
      web: "Official web",
      merged: "Official + local",
      fake: "Preview",
    });
  });

  it("keeps confidence labels stable", () => {
    expect(confidenceLabels).toEqual({
      high: "High",
      medium: "Medium",
      low: "Low",
      unknown: "Unknown",
    });
  });
});

describe("frontend provider status notes", () => {
  it("maps missing local data and web outage statuses to user-facing notes", () => {
    expect(providerStatusMessage(snapshot({ details: { status: "missing_data" } }))).toBe(
      "No usage data found",
    );
    expect(providerStatusMessage(snapshot({ details: { status: "unavailable" } }))).toBe(
      "Provider unavailable",
    );
    expect(providerStatusMessage(snapshot({ details: { status: "network_unavailable" } }))).toBe(
      "Network unavailable",
    );
    expect(providerStatusMessage(snapshot({ details: { status: "timed_out" } }))).toBe(
      "Usage refresh timed out",
    );
  });

  it("maps login/interruption statuses without exposing raw provider details", () => {
    expect(providerStatusMessage(snapshot({ details: { status: "login_required" } }))).toBe(
      "Login required",
    );
    expect(providerStatusMessage(snapshot({ details: { status: "captcha_or_bot_check" } }))).toBe(
      "Additional verification required",
    );
    expect(providerStatusMessage(snapshot({ details: { status: "unexpected_ui" } }))).toBe(
      "Unexpected usage page",
    );
  });

  it("hides parsed, placeholder, and unknown status values", () => {
    expect(providerStatusMessage(snapshot({ details: { status: "parsed" } }))).toBeNull();
    expect(providerStatusMessage(snapshot({ details: { status: "placeholder" } }))).toBeNull();
    expect(providerStatusMessage(snapshot({ details: { status: "raw error text" } }))).toBeNull();
  });

  it("uses fallback web status notes when local data stays visible", () => {
    expect(
      providerStatusMessage(
        snapshot({
          details: {
            status: "parsed",
            webStatus: "login_required",
          },
        }),
      ),
    ).toBe("Login required");
  });
});

describe("browser preview state fixtures", () => {
  it("parses known preview state query parameters with a safe default", () => {
    expect(browserPreviewStateFromSearch("?previewState=missing-local-data")).toBe(
      "missing-local-data",
    );
    expect(browserPreviewStateFromSearch("?previewState=network-unavailable")).toBe(
      "network-unavailable",
    );
    expect(browserPreviewStateFromSearch("?previewState=expired-login")).toBe("expired-login");
    expect(browserPreviewStateFromSearch("?previewState=mfa-required")).toBe("mfa-required");
    expect(browserPreviewStateFromSearch("?previewState=captcha-or-bot-check")).toBe(
      "captcha-or-bot-check",
    );
    expect(browserPreviewStateFromSearch("?previewState=unexpected-ui")).toBe("unexpected-ui");
    expect(browserPreviewStateFromSearch("?previewState=timed-out")).toBe("timed-out");
    expect(browserPreviewStateFromSearch("?previewState=parse-failed")).toBe("parse-failed");
    expect(browserPreviewStateFromSearch("?previewState=stale-data")).toBe("stale-data");
    expect(browserPreviewStateFromSearch("?previewState=provider-unavailable")).toBe(
      "provider-unavailable",
    );
    expect(browserPreviewStateFromSearch("?previewState=permission-denied")).toBe(
      "permission-denied",
    );
    expect(browserPreviewStateFromSearch("?previewState=unsafe-profile-path")).toBe(
      "unsafe-profile-path",
    );
    expect(browserPreviewStateFromSearch("?previewState=provider-disabled")).toBe(
      "provider-disabled",
    );
    expect(browserPreviewStateFromSearch("?previewState=<script>")).toBe("default");
  });

  it("renders status-note snapshots for browser-preview smoke states", () => {
    for (const [state, note] of [
      ["missing-local-data", "No usage data found"],
      ["network-unavailable", "Network unavailable"],
      ["expired-login", "Login required"],
      ["mfa-required", "MFA required"],
      ["captcha-or-bot-check", "Additional verification required"],
      ["unexpected-ui", "Unexpected usage page"],
      ["timed-out", "Usage refresh timed out"],
      ["parse-failed", "Usage data could not be parsed"],
      ["provider-unavailable", "Provider unavailable"],
      ["permission-denied", "Usage data is not readable"],
      ["unsafe-profile-path", "Profile path blocked"],
      ["provider-disabled", "Provider disabled"],
    ] as const) {
      expect(browserPreviewSnapshots(state).map(providerStatusMessage)).toEqual([note, note]);
    }
  });

  it("renders stale browser-preview snapshots without inventing provider errors", () => {
    const snapshots = browserPreviewSnapshots("stale-data");

    expect(snapshots.map(snapshotIsStale)).toEqual([true, true]);
    expect(snapshots.map(providerStatusMessage)).toEqual([null, null]);
  });
});

describe("frontend settings form behavior", () => {
  it("shows empty profile path inputs for default paths", () => {
    expect(profilePathValue(null)).toBe("");
    expect(profilePathValue("/tmp/forgegauge/codex")).toBe("/tmp/forgegauge/codex");
  });

  it("normalizes blank profile path input back to default null", () => {
    expect(profilePathFromInput("  ")).toBeNull();
    expect(profilePathFromInput(" /tmp/forgegauge/profile ")).toBe("/tmp/forgegauge/profile");
  });
});

describe("frontend web-provider opt-in disabled states", () => {
  it("disables web-only controls while experimental web providers are off", () => {
    expect(webProviderControlState(configWithWebEnabled(false))).toEqual({
      webRefreshDisabled: true,
      webCooldownDisabled: true,
      profilePathInputsDisabled: true,
      officialRefreshDisabled: true,
      startLoginDisabled: true,
    });
  });

  it("enables web-only controls after experimental web provider opt-in", () => {
    expect(webProviderControlState(configWithWebEnabled(true))).toEqual({
      webRefreshDisabled: false,
      webCooldownDisabled: false,
      profilePathInputsDisabled: false,
      officialRefreshDisabled: false,
      startLoginDisabled: false,
    });
  });
});

describe("frontend profile inspection summaries", () => {
  it("summarizes clean, unprepared, and suspicious profile states", () => {
    expect(profileInspectionSummary(profileInspection({}))).toBe("Codex profile inspection clean");
    expect(profileInspectionSummary(profileInspection({ profilePrepared: false }))).toBe(
      "Codex profile is not prepared",
    );
    expect(
      profileInspectionSummary(
        profileInspection({
          credentialStoreFiles: 2,
          autofillStoreFiles: 3,
          symlinkEntries: 1,
          passwordSavingEnabled: true,
          autofillEnabled: true,
          entryLimitReached: true,
        }),
      ),
    ).toBe(
      "Codex profile inspection found 2 credential files, 3 autofill store files, 1 symlink entry, password saving enabled, autofill enabled, inspection limit reached",
    );
  });
});
