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

export type UsageDisplayState = {
  snapshots: UsageSnapshot[];
  updatedAt: string;
};

export type CommandError = {
  code: string;
  message: string;
};

export type LocalServiceQuotaSettings = {
  enabled: boolean;
  planLabel: string;
  limitKind: "rollingWindow";
  windowHours: number;
  usageUnit: "tokens";
  limit: number;
};

export type AppConfig = {
  version: number;
  enabledServices: {
    codex: boolean;
    claude: boolean;
  };
  providers: {
    localEnabled: boolean;
    webEnabled: boolean;
  };
  intervals: {
    localSeconds: number;
    webMinutes: number;
    manualWebRefreshCooldownSeconds: number;
    gaugeSwitchSeconds: number;
  };
  lowUsageThreshold: number;
  browserProfiles: {
    rootPath: string | null;
    codexPath: string | null;
    claudePath: string | null;
  };
  localQuotas: {
    codex: LocalServiceQuotaSettings;
    claude: LocalServiceQuotaSettings;
  };
};

export const defaultConfig: AppConfig = {
  version: 3,
  enabledServices: {
    codex: true,
    claude: true,
  },
  providers: {
    localEnabled: true,
    webEnabled: false,
  },
  intervals: {
    localSeconds: 45,
    webMinutes: 30,
    manualWebRefreshCooldownSeconds: 60,
    gaugeSwitchSeconds: 6,
  },
  lowUsageThreshold: 20,
  browserProfiles: {
    rootPath: null,
    codexPath: null,
    claudePath: null,
  },
  localQuotas: {
    codex: {
      enabled: false,
      planLabel: "",
      limitKind: "rollingWindow",
      windowHours: 5,
      usageUnit: "tokens",
      limit: 0,
    },
    claude: {
      enabled: false,
      planLabel: "",
      limitKind: "rollingWindow",
      windowHours: 5,
      usageUnit: "tokens",
      limit: 0,
    },
  },
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
