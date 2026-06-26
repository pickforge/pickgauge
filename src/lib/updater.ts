// Auto-update over the Tauri updater plugin. The plugin checks the GitHub
// `latest.json` endpoint configured in tauri.conf.json and verifies the bundle
// signature against the embedded public key. No-ops outside Tauri (browser
// preview) and in dev, where there is no signed bundle to replace.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { desktopApiAvailable } from "./api";

// The main window is created hidden and the floating capsule shares this
// entrypoint, so an unguarded startup check could prompt from an invisible or
// floating webview (and twice). Run the check from the main window only, and
// defer it until that window is actually visible.
export async function checkForUpdatesWhenVisible(): Promise<void> {
  if (import.meta.env.DEV || !desktopApiAvailable()) {
    return;
  }

  const appWindow = getCurrentWindow();

  if (await appWindow.isVisible().catch(() => false)) {
    await checkForUpdatesOnStartup();
    return;
  }

  let started = false;
  const unlisten = await appWindow.onFocusChanged(({ payload: focused }) => {
    if (!focused || started) {
      return;
    }
    started = true;
    unlisten();
    void checkForUpdatesOnStartup();
  });
}

/** Check once on startup; if an update is found, prompt, install, relaunch. */
export async function checkForUpdatesOnStartup(): Promise<void> {
  if (import.meta.env.DEV || !desktopApiAvailable()) {
    return;
  }

  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    if (!update) {
      return;
    }

    const accepted = window.confirm(
      `PickGauge ${update.version} is available. Download and restart to update now?`,
    );
    if (!accepted) {
      return;
    }

    await update.downloadAndInstall();

    const { relaunch } = await import("@tauri-apps/plugin-process");
    await relaunch();
  } catch (error) {
    console.error("PickGauge update check failed", error);
  }
}
