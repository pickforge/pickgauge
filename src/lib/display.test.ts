import { describe, expect, it } from "vitest";
import {
  confidenceLabels,
  formatPercent,
  lastOfficialCheck,
  localActivitySummary,
  profilePathFromInput,
  profilePathValue,
  serviceLabels,
  sourceLabels,
  webProviderControlState,
} from "./display";
import { defaultConfig, type AppConfig, type UsageSnapshot } from "./usage";

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
            modelCount: 1,
          },
        }),
        "en-US",
      ),
    ).toBe("Local activity: 1,234 tokens | 2 sessions | 1 model");
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
