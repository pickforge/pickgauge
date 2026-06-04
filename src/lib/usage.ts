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

type ProviderStatusCode =
  | "parsed"
  | "placeholder"
  | "not_configured"
  | "disabled"
  | "missing_data"
  | "unavailable"
  | "permission_denied"
  | "parse_failed"
  | "login_required"
  | "mfa_required"
  | "captcha_or_bot_check"
  | "network_unavailable"
  | "timed_out"
  | "unexpected_ui"
  | "unsafe_path"
  | "internal";

const providerStatusMessages: Partial<Record<ProviderStatusCode, string>> = {
  not_configured: "Provider not configured",
  disabled: "Provider disabled",
  missing_data: "No usage data found",
  unavailable: "Provider unavailable",
  permission_denied: "Usage data is not readable",
  parse_failed: "Usage data could not be parsed",
  login_required: "Login required",
  mfa_required: "MFA required",
  captcha_or_bot_check: "Additional verification required",
  network_unavailable: "Network unavailable",
  timed_out: "Usage refresh timed out",
  unexpected_ui: "Unexpected usage page",
  unsafe_path: "Profile path blocked",
  internal: "Provider unavailable",
};

function isProviderStatusCode(value: unknown): value is ProviderStatusCode {
  return typeof value === "string" && value in providerStatusMessages;
}

function statusMessage(value: unknown) {
  if (!isProviderStatusCode(value)) {
    return null;
  }

  return providerStatusMessages[value] ?? null;
}

export function providerStatusMessage(snapshot: UsageSnapshot) {
  return statusMessage(snapshot.details.status) ?? statusMessage(snapshot.details.webStatus);
}

export function redactedUserPath(path: string) {
  return path
    .replace(/^\/home\/[^/]+(?=\/)/, "~")
    .replace(/^\/Users\/[^/]+(?=\/)/, "~");
}

export type UsageDisplayState = {
  snapshots: UsageSnapshot[];
  updatedAt: string;
};

export type CommandError = {
  code: string;
  message: string;
};

export type OfficialUsagePage = {
  service: Service;
  url: string;
  openedAt: string;
};

export type ProviderLoginStart = {
  service: Service;
  url: string;
  status: "login_required" | "launched";
  backend: "playwright-headed-chromium-sidecar";
  profileLabel: string;
  profilePrepared: boolean;
  startedAt: string;
};

export type LoginRequiredEvent = {
  service: Service;
  url: string;
  reason: "managed_login_not_available" | "sidecar_unavailable";
  emittedAt: string;
};

export type ClearedProviderProfile = {
  service: Service;
  cleared: boolean;
  clearedAt: string;
};

export type ProviderProfileInspection = {
  service: Service;
  profileLabel: string;
  profilePrepared: boolean;
  credentialStoreFiles: number;
  autofillStoreFiles: number;
  symlinkEntries: number;
  passwordSavingEnabled: boolean;
  autofillEnabled: boolean;
  inspectedEntries: number;
  entryLimitReached: boolean;
  inspectedAt: string;
};

export type LogLocation = {
  path: string;
  exists: boolean;
  redactionPolicy: string;
  updatedAt: string;
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
  autostart: {
    enabled: boolean;
  };
};

export const defaultConfig: AppConfig = {
  version: 4,
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
  autostart: {
    enabled: false,
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

export type BrowserPreviewState = "default" | "missing-local-data" | "network-unavailable" | "expired-login";

const browserPreviewStates = new Set<BrowserPreviewState>([
  "default",
  "missing-local-data",
  "network-unavailable",
  "expired-login",
]);

export function browserPreviewStateFromSearch(search: string): BrowserPreviewState {
  const state = new URLSearchParams(search).get("previewState");

  return state !== null && browserPreviewStates.has(state as BrowserPreviewState)
    ? (state as BrowserPreviewState)
    : "default";
}

export function browserPreviewSnapshots(state: BrowserPreviewState): UsageSnapshot[] {
  switch (state) {
    case "missing-local-data":
      return previewSnapshots({
        confidence: "unknown",
        details: { status: "missing_data" },
        lastUpdated: "Preview missing local data",
        remainingPercent: null,
        source: "local",
        usedPercent: null,
      });
    case "network-unavailable":
      return previewSnapshots({
        details: { status: "parsed", webStatus: "network_unavailable" },
        lastUpdated: "2026-06-04T12:00:00Z",
        source: "local",
      });
    case "expired-login":
      return previewSnapshots({
        details: { status: "parsed", webStatus: "login_required" },
        lastUpdated: "2026-06-04T12:00:00Z",
        source: "local",
      });
    case "default":
      return fallbackSnapshots;
  }
}

function previewSnapshots(partial: Partial<UsageSnapshot>): UsageSnapshot[] {
  return fallbackSnapshots.map((snapshot) => ({
    ...snapshot,
    ...partial,
    details: {
      ...partial.details,
    },
  }));
}
