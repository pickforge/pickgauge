import { describe, expect, it } from "vitest";
import {
  browserPreviewSnapshots,
  defaultConfig,
  fallbackSnapshots,
  floatDisplaySnapshots,
  providerStatusMessage,
  providerStatusKind,
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
              fable: {
                remainingPercent: 88,
                usedPercent: 12,
                resetAt: null,
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
      fable: {
        remainingPercent: 88,
        usedPercent: 12,
        resetAt: null,
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
      fable: null,
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
      fable: null,
    });
  });

  it("does not relabel a secondary-only headline as five-hour usage", () => {
    expect(
      usageWindows(
        snapshot({
          remainingPercent: 57,
          usedPercent: 43,
          details: {
            windows: {
              week: { remainingPercent: 57, usedPercent: 43, resetAt: null },
              fable: { remainingPercent: 88, usedPercent: 12, resetAt: null },
            },
          },
        }),
      ),
    ).toEqual({
      fiveHour: null,
      week: { remainingPercent: 57, usedPercent: 43, resetAt: null },
      fable: { remainingPercent: 88, usedPercent: 12, resetAt: null },
    });
  });
});

describe("provider status kind", () => {
  it("prioritizes a failed web refresh over fallback usage", () => {
    expect(
      providerStatusKind(
        snapshot({
          remainingPercent: 72,
          details: { status: "parsed", webStatus: "network_unavailable" },
        }),
      ),
    ).toBe("bad");
  });

  it("marks an official verification gate as a warning", () => {
    expect(
      providerStatusKind(
        snapshot({
          remainingPercent: 72,
          details: { status: "parsed", webStatus: "captcha_or_bot_check" },
        }),
      ),
    ).toBe("warn");
  });

  it("marks an unavailable provider as bad", () => {
    expect(providerStatusKind(snapshot({ details: { status: "unavailable" } }))).toBe("bad");
  });
});

describe("usage fixtures and redaction", () => {
  it("keeps deferred services off while preserving legacy config fields", () => {
    expect(defaultConfig.enabledServices.grok).toBe(false);
    expect(defaultConfig.enabledServices.ollama).toBe(false);
    expect(defaultConfig.providers.webEnabled).toBe(false);
    expect(defaultConfig.browserProfiles).toEqual({
      rootPath: null,
      codexPath: null,
      claudePath: null,
      grokPath: null,
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

    expect(snapshots.map(providerStatusMessage)).toEqual([
      "Network unavailable",
      "Network unavailable",
    ]);
    expect(snapshots[0].details).not.toBe(snapshots[1].details);
  });


  it("keeps deferred services out of the floating capsule", () => {
    const codex = snapshot({ service: "codex", remainingPercent: 72 });
    const claude = snapshot({ service: "claude", remainingPercent: 61 });
    const grok = snapshot({ service: "grok", remainingPercent: 50 });
    const ollama = snapshot({ service: "ollama", remainingPercent: 40 });

    expect(floatDisplaySnapshots([codex, claude, grok, ollama])).toEqual([codex, claude]);
  });
});
