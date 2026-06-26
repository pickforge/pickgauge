// Auto-update over the Tauri updater plugin. The plugin checks the GitHub
// `latest.json` endpoint configured in tauri.conf.json and verifies the bundle
// signature against the embedded public key. No-ops outside Tauri (browser
// preview) and in dev, where there is no signed bundle to replace.
import { desktopApiAvailable } from "./api";

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
