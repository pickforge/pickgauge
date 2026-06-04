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
    expect(browserPreviewStateFromSearch("?previewState=<script>")).toBe("default");
  });

  it("renders status-note snapshots for browser-preview smoke states", () => {
    expect(browserPreviewSnapshots("missing-local-data").map(providerStatusMessage)).toEqual([
      "No usage data found",
      "No usage data found",
    ]);
    expect(browserPreviewSnapshots("network-unavailable").map(providerStatusMessage)).toEqual([
      "Network unavailable",
      "Network unavailable",
    ]);
    expect(browserPreviewSnapshots("expired-login").map(providerStatusMessage)).toEqual([
      "Login required",
      "Login required",
    ]);
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
