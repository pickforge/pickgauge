import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  ClearedProviderProfile,
  LogLocation,
  OfficialUsagePage,
  ProviderLoginStart,
  ProviderProfileInspection,
  Service,
  UsageDisplayState,
  UsageSource,
} from "./usage";

export type DailyGaugeStat = {
  day: string;
  avgRemainingPercent: number | null;
  minRemainingPercent: number | null;
  lastRemainingPercent: number | null;
  samples: number;
};

export type UsageHistoryReport = {
  codex: DailyGaugeStat[];
  claude: DailyGaugeStat[];
  days: number;
  generatedAt: string;
};

export type DailyTokenUsage = {
  day: string;
  tokens: number;
  activity: number;
};

export type LocalDailyUsageReport = {
  codex: DailyTokenUsage[];
  claude: DailyTokenUsage[];
  codexStatus: string | null;
  claudeStatus: string | null;
  days: number;
  generatedAt: string;
};

export type WindowVisibility = {
  status: string;
  updatedAt: string;
};

export function desktopApiAvailable() {
  return (
    typeof window !== "undefined" &&
    Boolean((window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__)
  );
}

function utcOffsetSeconds() {
  return -new Date().getTimezoneOffset() * 60;
}

export const api = {
  getAppConfig: () => invoke<AppConfig>("get_app_config"),
  updateAppConfig: (config: AppConfig) => invoke<AppConfig>("update_app_config", { config }),
  getDisplayState: () => invoke<UsageDisplayState>("get_display_state"),
  refreshUsage: () => invoke<UsageDisplayState>("refresh_usage"),
  refreshProvider: (service: Service, source: UsageSource) =>
    invoke<UsageDisplayState>("refresh_provider", { service, source }),
  clearCachedSnapshots: () => invoke<UsageDisplayState>("clear_cached_snapshots"),
  openOfficialUsagePage: (service: Service) =>
    invoke<OfficialUsagePage>("open_official_usage_page", { service }),
  startProviderLogin: (service: Service) =>
    invoke<ProviderLoginStart>("start_provider_login", { service }),
  resetProviderSession: (service: Service) =>
    invoke<ClearedProviderProfile>("reset_provider_session", { service }),
  inspectProviderProfile: (service: Service) =>
    invoke<ProviderProfileInspection>("inspect_provider_profile", { service }),
  getLogLocation: () => invoke<LogLocation>("get_log_location"),
  hideMainWindow: () => invoke<WindowVisibility>("hide_main_window"),
  showMainWindow: () => invoke<WindowVisibility>("show_main_window"),
  getUsageHistory: (days: number) =>
    invoke<UsageHistoryReport>("get_usage_history", {
      days,
      utcOffsetSeconds: utcOffsetSeconds(),
    }),
  getLocalDailyUsage: (days: number) =>
    invoke<LocalDailyUsageReport>("get_local_daily_usage", {
      days,
      utcOffsetSeconds: utcOffsetSeconds(),
    }),
};
