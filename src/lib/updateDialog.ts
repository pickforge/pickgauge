// Wires the shared @pickforge/tauri-updater controller into PickGauge's
// main window, behind the `studioUpdateDialog` flag (see ./flags.ts). Only
// App.svelte (the main-window component) imports this module — Float.svelte
// (the floating capsule) never does, and `createMainWindowEligibility` below
// additionally checks the real Tauri window label as a second, independent
// guard against the capsule or a hidden main window ever surfacing the
// prompt. This replaces the legacy `window.confirm` flow in ./updater.ts
// once the flag is on; that file is untouched and keeps working while the
// flag is off.
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  createProcessCheckGate,
  createTauriUpdaterAdapter,
  createUpdateController,
  definePickforgeUpdaterElement,
  type UpdateAdapter,
  type UpdateController,
  type UpdateDialogMetadata,
  type UpdateEligibility,
} from "@pickforge/tauri-updater";
import { desktopApiAvailable } from "./api";

export const MAIN_WINDOW_LABEL = "main";

/** The subset of the Tauri `Window` API eligibility needs, so tests can
 * inject a fake instead of a real webview window. */
export interface EligibilityWindow {
  label: string;
  isVisible(): Promise<boolean>;
  onFocusChanged(handler: (event: { payload: boolean }) => void): Promise<() => void>;
}

/** The DOM surface `mountUpdateDialog` needs from a `<pickforge-update-dialog>`
 * element, so callers don't have to import the package's element class. */
export interface UpdateDialogHost {
  metadata: UpdateDialogMetadata;
  controller: UpdateController | undefined;
}

/**
 * Resolves eligible only for the packaged app's main window once it is
 * visible. The floating capsule (`label !== "main"`) and a hidden main
 * window (tray/login-start) never resolve true from here; a hidden main
 * window waits for its first focus before resolving, exactly mirroring the
 * pre-existing `checkForUpdatesWhenVisible` deferral in ./updater.ts.
 */
export function createMainWindowEligibility(
  win: EligibilityWindow,
  packaged: boolean,
): UpdateEligibility {
  return {
    async whenEligible() {
      if (!packaged || win.label !== MAIN_WINDOW_LABEL) {
        return false;
      }

      if (await win.isVisible().catch(() => false)) {
        return true;
      }

      return new Promise<boolean>((resolve) => {
        let started = false;
        let unlisten: (() => void) | undefined;

        void win
          .onFocusChanged(({ payload: focused }) => {
            if (!focused || started) {
              return;
            }
            started = true;
            unlisten?.();
            resolve(true);
          })
          .then((fn) => {
            unlisten = fn;
            if (started) {
              fn();
            }
          });
      });
    },
  };
}

async function createAdapter(): Promise<UpdateAdapter> {
  const [{ check }, { relaunch }] = await Promise.all([
    import("@tauri-apps/plugin-updater"),
    import("@tauri-apps/plugin-process"),
  ]);
  return createTauriUpdaterAdapter({ check, relaunch });
}

let controllerPromise: Promise<UpdateController> | undefined;

/** One controller for the process; its process-check gate enforces a
 * single update check per process even if this is somehow called twice. */
function getUpdateController(packaged: boolean): Promise<UpdateController> {
  controllerPromise ??= (async () => {
    const adapter = await createAdapter();
    const eligibility = createMainWindowEligibility(getCurrentWindow(), packaged);
    return createUpdateController({ adapter, eligibility, gate: createProcessCheckGate() });
  })();
  return controllerPromise;
}

/**
 * Mounts the shared update dialog on `host` and starts the single
 * per-process check. No-ops in dev builds and outside Tauri (browser
 * preview), matching the legacy flow's packaged-build-only behavior.
 * Startup check failures are swallowed by the controller (silent,
 * non-blocking); anything else that goes wrong while wiring up is logged
 * and otherwise ignored so it can never block app startup.
 */
export function mountUpdateDialog(host: UpdateDialogHost): void {
  const packaged = !import.meta.env.DEV && desktopApiAvailable();
  if (!packaged) {
    return;
  }

  definePickforgeUpdaterElement();

  void (async () => {
    try {
      const controller = await getUpdateController(packaged);
      host.metadata = {
        productName: "PickGauge",
        currentVersion: await getVersion().catch(() => "—"),
        productMark: "PG",
      };
      host.controller = controller;
      await controller.start();
    } catch (error) {
      console.error("PickGauge update dialog setup failed", error);
    }
  })();
}
