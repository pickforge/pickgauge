import type {
  AppConfig,
  ProviderProfileInspection,
  Service,
  UsageConfidence,
  UsageSnapshot,
  UsageSource,
} from "./usage";

export const serviceLabels: Record<Service, string> = {
  codex: "Codex",
  claude: "Claude Code",
  grok: "Grok",
  ollama: "Ollama",
};

export const sourceLabels: Record<UsageSource, string> = {
  local: "Local estimate",
  web: "Official web",
  merged: "Official + local",
  fake: "Preview",
};

export function snapshotSourceLabel(snapshot: UsageSnapshot) {
  return snapshot.details.providerId === "ollama.local"
    ? "Local daemon"
    : sourceLabels[snapshot.source];
}

export const confidenceLabels: Record<UsageConfidence, string> = {
  high: "High",
  medium: "Medium",
  low: "Low",
  unknown: "Unknown",
};

export type WebProviderControlState = {
  webRefreshDisabled: boolean;
  webCooldownDisabled: boolean;
  profilePathInputsDisabled: boolean;
  officialRefreshDisabled: boolean;
  startLoginDisabled: boolean;
};

export function formatPercent(value: number | null) {
  return value === null ? "Unknown" : `${Math.round(value)}%`;
}

export function formatCount(value: number, locales?: Intl.LocalesArgument) {
  return new Intl.NumberFormat(locales).format(value);
}

export function detailNumber(snapshot: UsageSnapshot, key: string) {
  const value = snapshot.details[key];
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

export function detailString(snapshot: UsageSnapshot, key: string) {
  const value = snapshot.details[key];
  return typeof value === "string" && value.length > 0 ? value : null;
}

export function plural(value: number, singular: string, pluralValue = `${singular}s`) {
  return value === 1 ? singular : pluralValue;
}

// TODO(#69): split provider-specific activity summaries.
// eslint-disable-next-line complexity -- Legacy display projection exceeds the enforced cap.
export function localActivitySummary(snapshot: UsageSnapshot, locales?: Intl.LocalesArgument) {
  if (snapshot.source !== "local" || snapshot.remainingPercent !== null) {
    return null;
  }

  if (snapshot.details.providerId === "ollama.local") {
    return ollamaAvailabilitySummary(snapshot, locales);
  }

  const totalTokens =
    detailNumber(snapshot, "totalTokens") ??
    (detailNumber(snapshot, "inputTokens") ?? 0) +
      (detailNumber(snapshot, "outputTokens") ?? 0) +
      (detailNumber(snapshot, "cacheCreationInputTokens") ?? 0) +
      (detailNumber(snapshot, "cacheReadInputTokens") ?? 0);

  if (totalTokens <= 0) {
    return null;
  }

  const activityCount =
    detailNumber(snapshot, "sessionCount") ??
    detailNumber(snapshot, "usageThreads") ??
    detailNumber(snapshot, "usageRecords");
  const activityLabel =
    detailNumber(snapshot, "sessionCount") !== null
      ? plural(activityCount ?? 0, "session")
      : detailNumber(snapshot, "usageThreads") !== null
        ? plural(activityCount ?? 0, "thread")
        : plural(activityCount ?? 0, "record");
  const modelCount = detailNumber(snapshot, "modelCount");
  const serverToolUseCount = detailNumber(snapshot, "serverToolUseCount");
  const parts = [`${formatCount(totalTokens, locales)} tokens`];

  if (activityCount !== null && activityCount > 0) {
    parts.push(`${formatCount(activityCount, locales)} ${activityLabel}`);
  }

  if (serverToolUseCount !== null && serverToolUseCount > 0) {
    parts.push(
      `${formatCount(serverToolUseCount, locales)} server tool ${plural(
        serverToolUseCount,
        "use",
        "uses",
      )}`,
    );
  }

  if (modelCount !== null && modelCount > 0) {
    parts.push(`${formatCount(modelCount, locales)} ${plural(modelCount, "model")}`);
  }

  return `Local activity: ${parts.join(" | ")}`;
}

export function ollamaAvailabilitySummary(
  snapshot: UsageSnapshot,
  locales?: Intl.LocalesArgument,
) {
  if (snapshot.details.providerId !== "ollama.local" || snapshot.details.status !== "parsed") {
    return null;
  }

  const modelCount = detailNumber(snapshot, "modelCount") ?? 0;
  const loadedModelCount = detailNumber(snapshot, "loadedModelCount");
  const parts = [
    "Daemon running",
    `${formatCount(modelCount, locales)} installed ${plural(modelCount, "model")}`,
  ];

  if (loadedModelCount !== null) {
    parts.push(
      `${formatCount(loadedModelCount, locales)} loaded ${plural(loadedModelCount, "model")}`,
    );
  }

  parts.push("no account quota");
  return parts.join(" · ");
}

export function formatTimestamp(value: string) {
  const parsed = new Date(value);

  if (Number.isNaN(parsed.getTime())) {
    return value;
  }

  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(parsed);
}

export function snapshotIsStale(snapshot: UsageSnapshot) {
  return snapshot.details.stale === true;
}

export function lastOfficialCheck(snapshot: UsageSnapshot) {
  const checkedAt = detailString(snapshot, "lastOfficialCheckAt");
  return checkedAt === null ? null : formatTimestamp(checkedAt);
}

export function loginPromptVisible(snapshot: UsageSnapshot) {
  if (
    snapshot.details.providerId === "ollama.local" &&
    snapshot.details.status === "login_required"
  ) {
    return false;
  }

  if (loginActionStatus(snapshot.details.status)) {
    return true;
  }

  return (
    (snapshot.source === "local" || snapshot.source === "fake") &&
    loginActionStatus(snapshot.details.webStatus)
  );
}

export function loginStatusClearedBySnapshots(service: Service, snapshots: UsageSnapshot[]) {
  const snapshot = snapshots.find((snapshot) => snapshot.service === service);

  if (snapshot === undefined || loginPromptVisible(snapshot)) {
    return false;
  }

  if (snapshot.source === "web" || snapshot.source === "merged") {
    return true;
  }

  return snapshot.details.webStatus !== undefined;
}

function loginActionStatus(value: unknown) {
  return (
    value === "login_required" ||
    value === "mfa_required" ||
    value === "captcha_or_bot_check"
  );
}

export function profilePathValue(value: string | null) {
  return value ?? "";
}

export function profilePathFromInput(value: string) {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

export type SettingsSaveDisplayState = {
  headerSaveHidden: boolean;
  headerSaveDisabled: boolean;
  overlayVisible: boolean;
};

/**
 * Single source of truth for where the primary Settings save action lives.
 * Clean: the header Save button is visible and disabled. Dirty: the header
 * Save button is hidden (without shifting layout) and a viewport-level
 * overlay owns the save/discard action instead. Exactly one is ever
 * presented at a time.
 */
export function settingsSaveDisplayState(dirty: boolean): SettingsSaveDisplayState {
  return {
    headerSaveHidden: dirty,
    headerSaveDisabled: !dirty,
    overlayVisible: dirty,
  };
}

export function webProviderControlState(config: AppConfig): WebProviderControlState {
  const disabled = !config.providers.webEnabled;

  return {
    webRefreshDisabled: disabled,
    webCooldownDisabled: disabled,
    profilePathInputsDisabled: disabled,
    officialRefreshDisabled: disabled,
    startLoginDisabled: disabled,
  };
}

export function profileInspectionSummary(inspection: ProviderProfileInspection) {
  const serviceLabel = serviceLabels[inspection.service];

  if (!inspection.profilePrepared) {
    return `${serviceLabel} profile is not prepared`;
  }

  const issues = [];

  if (inspection.credentialStoreFiles > 0) {
    issues.push(
      `${formatCount(inspection.credentialStoreFiles)} credential ${plural(
        inspection.credentialStoreFiles,
        "file",
      )}`,
    );
  }

  if (inspection.autofillStoreFiles > 0) {
    issues.push(
      `${formatCount(inspection.autofillStoreFiles)} autofill store ${plural(
        inspection.autofillStoreFiles,
        "file",
      )}`,
    );
  }

  if (inspection.cookieStoreFiles > 0) {
    issues.push(
      `${formatCount(inspection.cookieStoreFiles)} cookie store ${plural(
        inspection.cookieStoreFiles,
        "file",
      )}`,
    );
  }

  if (inspection.siteStorageEntries > 0) {
    issues.push(
      `${formatCount(inspection.siteStorageEntries)} site storage ${plural(
        inspection.siteStorageEntries,
        "entry",
        "entries",
      )}`,
    );
  }

  if (inspection.symlinkEntries > 0) {
    issues.push(
      `${formatCount(inspection.symlinkEntries)} symlink ${plural(
        inspection.symlinkEntries,
        "entry",
        "entries",
      )}`,
    );
  }

  if (inspection.passwordSavingEnabled) {
    issues.push("password saving enabled");
  }

  if (inspection.autofillEnabled) {
    issues.push("autofill enabled");
  }

  if (inspection.entryLimitReached) {
    issues.push("inspection limit reached");
  }

  if (issues.length > 0) {
    return `${serviceLabel} profile inspection found ${issues.join(", ")}`;
  }

  return `${serviceLabel} profile inspection clean`;
}
