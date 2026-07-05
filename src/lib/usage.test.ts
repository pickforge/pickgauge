import { describe, expect, it } from "vitest";
import {
  browserPreviewSnapshots,
  defaultConfig,
  fallbackSnapshots,
  providerStatusMessage,
  redactedUserPath,
  usageWindows,
  type UsageSnapshot,
} from "./usage";

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

describe("usage windows", () => {
  it("reads provider-supplied window details", () => {
    expect(
      usageWindows(
        snapshot({
          details: {
            windows: {
              fiveHour: {
                remainingPercent: 64,
                usedPercent: 36,
                resetAt: "2026-06-03T17:00:00Z",
              },
              week: {
                remainingPercent: 80,
                usedPercent: 20,
                resetAt: "2026-06-10T00:00:00Z",
              },
            },
          },
        }),
      ),
    ).toEqual({
      fiveHour: {
        remainingPercent: 64,
        usedPercent: 36,
        resetAt: "2026-06-03T17:00:00Z",
      },
      week: {
        remainingPercent: 80,
        usedPercent: 20,
        resetAt: "2026-06-10T00:00:00Z",
      },
    });
  });

  it("falls back to headline usage only for the five-hour window", () => {
    expect(
      usageWindows(
        snapshot({
          remainingPercent: 72,
          usedPercent: 28,
          resetAt: "2026-06-03T17:00:00Z",
        }),
      ),
    ).toEqual({
      fiveHour: {
        remainingPercent: 72,
        usedPercent: 28,
        resetAt: "2026-06-03T17:00:00Z",
      },
      week: null,
    });
  });

  it("drops malformed window fields without inventing values", () => {
    expect(
      usageWindows(
        snapshot({
          details: {
            windows: {
              fiveHour: {
                remainingPercent: "64",
                usedPercent: 36,
                resetAt: 123,
              },
            },
          },
        }),
      ),
    ).toEqual({
      fiveHour: {
        remainingPercent: null,
        usedPercent: 36,
        resetAt: null,
      },
      week: null,
    });
  });
});

describe("usage fixtures and redaction", () => {
  it("keeps default privacy-sensitive settings opt-in", () => {
    expect(defaultConfig.providers.webEnabled).toBe(false);
    expect(defaultConfig.browserProfiles).toEqual({
      rootPath: null,
      codexPath: null,
      claudePath: null,
      ollamaPath: null,
    });
  });

  it("redacts common user home directories", () => {
    expect(redactedUserPath("/home/alice/.codex/state.sqlite")).toBe("~/.codex/state.sqlite");
    expect(redactedUserPath("/Users/alice/Library/Application Support/PickGauge")).toBe(
      "~/Library/Application Support/PickGauge",
    );
    expect(redactedUserPath("/var/lib/pickgauge")).toBe("/var/lib/pickgauge");
  });

  it("keeps fallback snapshots as non-authoritative placeholders", () => {
    expect(fallbackSnapshots).toEqual([
      expect.objectContaining({
        service: "codex",
        source: "fake",
        confidence: "unknown",
        details: { status: "placeholder" },
      }),
      expect.objectContaining({
        service: "claude",
        source: "fake",
        confidence: "unknown",
        details: { status: "placeholder" },
      }),
    ]);
    expect(fallbackSnapshots.map(providerStatusMessage)).toEqual([null, null]);
  });

  it("creates preview snapshots with independent detail objects", () => {
    const snapshots = browserPreviewSnapshots("network-unavailable");

    expect(snapshots.map(providerStatusMessage)).toEqual(["Network unavailable", "Network unavailable"]);
    expect(snapshots[0].details).not.toBe(snapshots[1].details);
  });
});
