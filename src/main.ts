import { listen } from "@tauri-apps/api/event";
import { mount } from "svelte";
import App from "./App.svelte";
import Float from "./Float.svelte";
import { api, desktopApiAvailable, EVENT_SETTINGS } from "./lib/api";
import { flagEnabled } from "./lib/flags";
import { initTheme, setTheme, type ThemeSetting } from "./lib/theme";
import { checkForUpdatesWhenVisible } from "./lib/updater";
import type { AppConfig } from "./lib/usage";
import "./app.css";

const target = document.getElementById("app");

if (!target) {
  throw new Error("App root element was not found");
}

// Dev-only visual fixture for the shared update dialog (see
// ./lib/updateDialogFixture.ts) — `import.meta.env.DEV` is statically
// eliminated, so none of this (including the fixture module's
// @pickforge/tauri-updater imports) reaches a production build.
const fixtureScenario = import.meta.env.DEV
  ? new URLSearchParams(window.location.search).get("updateDialogFixture")
  : null;

let app: ReturnType<typeof mount> | undefined;

if (fixtureScenario) {
  void import("./lib/updateDialogFixture").then(({ mountUpdateDialogFixture }) =>
    mountUpdateDialogFixture(target, fixtureScenario),
  );
} else {
  app = mountApp();
}

function currentWindowLabel() {
  const internals = (
    window as Window & {
      __TAURI_INTERNALS__?: { metadata?: { currentWindow?: { label?: string } } };
    }
  ).__TAURI_INTERNALS__;

  if (internals?.metadata?.currentWindow?.label) {
    return internals.metadata.currentWindow.label;
  }

  // Browser preview only: ?window=float renders the floating capsule.
  return new URLSearchParams(window.location.search).get("window") ?? "main";
}

function mountApp() {
  const windowLabel = currentWindowLabel();
  const component = windowLabel === "float" ? Float : App;

  if (component === Float) {
    document.documentElement.classList.add("is-float");
    document.body.classList.add("float-host");
  }

  if (desktopApiAvailable()) {
    api
      .getAppConfig()
      .then((config) => initTheme(config.ui.theme as ThemeSetting))
      .catch(() => initTheme("system"));
    void listen<AppConfig>(EVENT_SETTINGS, (event) => {
      void setTheme(event.payload.ui.theme as ThemeSetting);
    });
    // App.svelte itself mounts the shared update dialog when
    // `studioUpdateDialog` is on (see its `showUpdateDialog` check); running
    // the legacy `window.confirm` flow here too would double-prompt, so it
    // only runs while the flag is off.
    if (windowLabel === "main" && !flagEnabled("studioUpdateDialog")) {
      void checkForUpdatesWhenVisible();
    }
  } else {
    initTheme("system");
  }

  return mount(component, { target: target! });
}

export default app;
