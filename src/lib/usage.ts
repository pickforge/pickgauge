export type Service = "codex" | "claude";

export type UsageSource = "local" | "web" | "merged" | "fake";

export type UsageConfidence = "high" | "medium" | "low" | "unknown";

export type UsageSnapshot = {
  service: Service;
  remainingPercent: number | null;
  usedPercent: number | null;
  resetAt: string | null;
  source: UsageSource;
  confidence: UsageConfidence;
  lastUpdated: string;
  details: Record<string, unknown>;
};

export const fallbackSnapshots: UsageSnapshot[] = [
  {
    service: "codex",
    remainingPercent: 72,
    usedPercent: 28,
    resetAt: null,
    source: "fake",
    confidence: "unknown",
    lastUpdated: "Waiting for local provider",
    details: { status: "placeholder" },
  },
  {
    service: "claude",
    remainingPercent: 41,
    usedPercent: 59,
    resetAt: null,
    source: "fake",
    confidence: "unknown",
    lastUpdated: "Waiting for local provider",
    details: { status: "placeholder" },
  },
];
