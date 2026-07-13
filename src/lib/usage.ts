export type Service = "codex" | "claude" | "grok" | "ollama";

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

export type UsageWindow = {
  remainingPercent: number | null;
  usedPercent: number | null;
  resetAt: string | null;
};

/**
 * Pull the 5-hour, weekly, and Claude Fable windows out of a snapshot's details, when the
 * provider supplies them (the CLI-credentials provider does). Falls back to
 * the headline `remainingPercent` for the 5-hour window so providers without
 * window detail still render.
 */
export function usageWindows(snapshot: UsageSnapshot): {
  fiveHour: UsageWindow | null;
  week: UsageWindow | null;
  fable: UsageWindow | null;
} {
  const readWindow = (value: unknown): UsageWindow | null => {
    if (!value || typeof value !== "object") return null;
    const w = value as Record<string, unknown>;
    const num = (v: unknown): number | null => (typeof v === "number" ? v : null);
    return {
      remainingPercent: num(w.remainingPercent),
      usedPercent: num(w.usedPercent),
      resetAt: typeof w.resetAt === "string" ? w.resetAt : null,
    };
  };

  const windows = snapshot.details?.windows as Record<string, unknown> | undefined;
  const fiveHour = readWindow(windows?.fiveHour);
  const week = readWindow(windows?.week);
  const fable = readWindow(windows?.fable);

  return {
    fiveHour:
      fiveHour ??
      (windows === undefined && snapshot.remainingPercent !== null
        ? {
            remainingPercent: snapshot.remainingPercent,
            usedPercent: snapshot.usedPercent,
            resetAt: snapshot.resetAt,
          }
        : null),
    week,
    fable,
  };
}

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
  const status = isProviderStatusCode(snapshot.details.status)
    ? snapshot.details.status
    : fallbackWebStatus(snapshot);


  if (snapshot.details.providerId === "ollama.local" && status === "login_required") {
    return "Ollama daemon requires sign-in outside PickGauge";
  }

  if (snapshot.details.providerId === "ollama.local" && status === "not_configured") {
    return "Ollama isn't running";
  }

  return statusMessage(status);
}

export function providerStatusKind(snapshot: UsageSnapshot): "ok" | "warn" | "bad" | "idle" {
  const status = snapshot.details.webStatus ?? snapshot.details.status;

  if (
    snapshot.details.providerId === "ollama.local" &&
    (status === "not_configured" || status === "login_required")
  ) {
    return "warn";
  }

  if (
    status === "login_required" ||
    status === "mfa_required" ||
    status === "captcha_or_bot_check"
  ) {
    return "warn";
  }

  if (
    status === "permission_denied" ||
    status === "parse_failed" ||
    status === "unavailable" ||
    status === "network_unavailable" ||
    status === "timed_out" ||
    status === "unexpected_ui" ||
    status === "unsafe_path" ||
    status === "internal"
  ) {
    return "bad";
  }

  if (snapshot.remainingPercent !== null || status === "parsed") {
    return "ok";
  }

  return "idle";
}

function fallbackWebStatus(snapshot: UsageSnapshot) {
  return snapshot.source === "local" || snapshot.source === "fake"
    ? snapshot.details.webStatus
    : null;
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

export function floatDisplaySnapshots(snapshots: UsageSnapshot[]) {
  return snapshots.filter(
    (snapshot) =>
      (snapshot.service === "codex" || snapshot.service === "claude") &&
      !(snapshot.remainingPercent === null && typeof snapshot.details.plan === "string"),
  );
}

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
  status: "already_authenticated" | "login_required" | "launched" | "preflight_unavailable";
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
  cookieStoreFiles: number;
  siteStorageEntries: number;
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
    grok: boolean;
    ollama: boolean;
  };
  providers: {
    localEnabled: boolean;
    webEnabled: boolean;
    cliEnabled: boolean;
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
    grokPath: string | null;
    ollamaPath: string | null;
  };
  localQuotas: {
    codex: LocalServiceQuotaSettings;
    claude: LocalServiceQuotaSettings;
  };
  autostart: {
    enabled: boolean;
  };
  crashReports: boolean;
  ui: {
    sounds: boolean;
    floatButton: boolean;
    theme: "system" | "dark" | "light";
  };
};

export const defaultConfig: AppConfig = {
  version: 7,
  enabledServices: {
    codex: true,
    claude: true,
    grok: false,
    ollama: false,
  },
  providers: {
    localEnabled: true,
    webEnabled: false,
    cliEnabled: true,
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
    grokPath: null,
    ollamaPath: null,
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
  crashReports: true,
  ui: {
    sounds: true,
    floatButton: true,
    theme: "system",
  },
};

export const fallbackSnapshots = [
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
] satisfies UsageSnapshot[];

export type BrowserPreviewState =
  | "default"
  | "official-usage"
  | "missing-local-data"
  | "network-unavailable"
  | "expired-login"
  | "mfa-required"
  | "captcha-or-bot-check"
  | "unexpected-ui"
  | "timed-out"
  | "parse-failed"
  | "stale-data"
  | "provider-unavailable"
  | "permission-denied"
  | "unsafe-profile-path"
  | "provider-disabled";

const browserPreviewStates = new Set<BrowserPreviewState>([
  "default",
  "official-usage",
  "missing-local-data",
  "network-unavailable",
  "expired-login",
  "mfa-required",
  "captcha-or-bot-check",
  "unexpected-ui",
  "timed-out",
  "parse-failed",
  "stale-data",
  "provider-unavailable",
  "permission-denied",
  "unsafe-profile-path",
  "provider-disabled",
]);

export function browserPreviewStateFromSearch(search: string): BrowserPreviewState {
  const state = new URLSearchParams(search).get("previewState");

  return state !== null && browserPreviewStates.has(state as BrowserPreviewState)
    ? (state as BrowserPreviewState)
    : "default";
}

export function browserPreviewSnapshots(state: BrowserPreviewState): UsageSnapshot[] {
  switch (state) {
    case "official-usage":
      return officialPreviewSnapshots();
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
    case "mfa-required":
      return previewSnapshots({
        details: { status: "parsed", webStatus: "mfa_required" },
        lastUpdated: "2026-06-04T12:00:00Z",
        source: "local",
      });
    case "captcha-or-bot-check":
      return previewSnapshots({
        details: { status: "parsed", webStatus: "captcha_or_bot_check" },
        lastUpdated: "2026-06-04T12:00:00Z",
        source: "local",
      });
    case "unexpected-ui":
      return previewSnapshots({
        details: { status: "parsed", webStatus: "unexpected_ui" },
        lastUpdated: "2026-06-04T12:00:00Z",
        source: "local",
      });
    case "timed-out":
      return previewSnapshots({
        details: { status: "parsed", webStatus: "timed_out" },
        lastUpdated: "2026-06-04T12:00:00Z",
        source: "local",
      });
    case "parse-failed":
      return previewSnapshots({
        details: { status: "parsed", webStatus: "parse_failed" },
        lastUpdated: "2026-06-04T12:00:00Z",
        source: "local",
      });
    case "stale-data":
      return previewSnapshots({
        details: { stale: true, status: "parsed" },
        lastUpdated: "2026-06-04T12:00:00Z",
        source: "local",
      });
    case "provider-unavailable":
      return previewSnapshots({
        details: { status: "unavailable" },
        lastUpdated: "2026-06-04T12:00:00Z",
        source: "local",
      });
    case "permission-denied":
      return previewSnapshots({
        details: { status: "permission_denied" },
        lastUpdated: "2026-06-04T12:00:00Z",
        source: "local",
      });
    case "unsafe-profile-path":
      return previewSnapshots({
        details: { status: "unsafe_path" },
        lastUpdated: "2026-06-04T12:00:00Z",
        source: "local",
      });
    case "provider-disabled":
      return previewSnapshots({
        confidence: "unknown",
        details: { status: "disabled" },
        lastUpdated: "Preview provider disabled",
        remainingPercent: null,
        source: "local",
        usedPercent: null,
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

function officialPreviewSnapshots(): UsageSnapshot[] {
  const reset = (hours: number) => new Date(Date.now() + hours * 3_600_000).toISOString();
  const readings: Record<
    (typeof fallbackSnapshots)[number]["service"],
    { remaining: number; windows: Record<string, UsageWindow> }
  > = {
    codex: {
      remaining: 67,
      windows: {
        fiveHour: { remainingPercent: 67, usedPercent: 33, resetAt: reset(2) },
        week: { remainingPercent: 76, usedPercent: 24, resetAt: reset(120) },
      },
    },
    claude: {
      remaining: 61,
      windows: {
        fiveHour: { remainingPercent: 61, usedPercent: 39, resetAt: reset(2) },
        week: { remainingPercent: 92, usedPercent: 8, resetAt: reset(144) },
        fable: { remainingPercent: 34, usedPercent: 66, resetAt: reset(96) },
      },
    },
  };

  return fallbackSnapshots.map((snapshot) => {
    const reading = readings[snapshot.service];
    return {
      ...snapshot,
      remainingPercent: reading.remaining,
      usedPercent: 100 - reading.remaining,
      resetAt: reading.windows.fiveHour?.resetAt ?? reading.windows.week?.resetAt ?? null,
      source: "web",
      confidence: "high",
      lastUpdated: new Date().toISOString(),
      details: {
        providerId: "preview.web",
        status: "parsed",
        windows: reading.windows,
      },
    };
  });
}
