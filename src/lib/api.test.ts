import { invoke } from "@tauri-apps/api/core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { api, desktopApiAvailable } from "./api";
import { defaultConfig } from "./usage";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const invokeMock = vi.mocked(invoke);

describe("desktop api availability", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("is false outside Tauri", () => {
    expect(desktopApiAvailable()).toBe(false);
  });

  it("is true when Tauri internals are present", () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });

    expect(desktopApiAvailable()).toBe(true);
  });
});

describe("Tauri invoke wrappers", () => {
  afterEach(() => {
    invokeMock.mockReset();
  });

  it("uses stable command names and payloads", () => {
    const config = {
      ...defaultConfig,
      lowUsageThreshold: 15,
    };

    for (const [call, command, args] of [
      [() => api.getAppConfig(), "get_app_config", undefined],
      [() => api.updateAppConfig(config), "update_app_config", { config }],
      [() => api.getDisplayState(), "get_display_state", undefined],
      [() => api.refreshUsage(), "refresh_usage", undefined],
      [
        () => api.refreshProvider("codex", "web"),
        "refresh_provider",
        { service: "codex", source: "web" },
      ],
      [() => api.clearCachedSnapshots(), "clear_cached_snapshots", undefined],
      [
        () => api.openOfficialUsagePage("claude"),
        "open_official_usage_page",
        { service: "claude" },
      ],
      [() => api.startProviderLogin("codex"), "start_provider_login", { service: "codex" }],
      [() => api.resetProviderSession("claude"), "reset_provider_session", { service: "claude" }],
      [
        () => api.inspectProviderProfile("codex"),
        "inspect_provider_profile",
        { service: "codex" },
      ],
      [() => api.getLogLocation(), "get_log_location", undefined],
      [() => api.getSystemTheme(), "get_system_theme", undefined],
      [() => api.hideMainWindow(), "hide_main_window", undefined],
      [() => api.showMainWindow(), "show_main_window", undefined],
      [() => api.toggleFloatButton(), "toggle_float_button", undefined],
    ] as const) {
      call();
      if (args === undefined) {
        expect(invokeMock).toHaveBeenLastCalledWith(command);
      } else {
        expect(invokeMock).toHaveBeenLastCalledWith(command, args);
      }
    }
  });

  it("passes local timezone offset to history queries", () => {
    const utcOffsetSeconds = -new Date().getTimezoneOffset() * 60;

    api.getUsageHistory(14);
    api.getLocalDailyUsage(30);

    expect(invokeMock).toHaveBeenNthCalledWith(1, "get_usage_history", {
      days: 14,
      utcOffsetSeconds,
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, "get_local_daily_usage", {
      days: 30,
      utcOffsetSeconds,
    });
  });
});
