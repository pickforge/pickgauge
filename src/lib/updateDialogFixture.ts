// Dev-only visual fixture for the shared update dialog. PickGauge has no
// visual-regression/screenshot-baseline harness (see scripts/
// validate-browser-preview.mjs — functional Playwright assertions, not
// pixel baselines), so this route stands in for one: `bun run dev` then
// open `?updateDialogFixture=available` or `?updateDialogFixture=downloading`
// to inspect the dialog with deterministic, injected state. Resize the
// window to PickGauge's normal (1000x700) and minimum (820x580) sizes to
// check both. Never reachable in a production build — `import.meta.env.DEV`
// is statically eliminated, so this module and its @pickforge/tauri-updater
// imports are tree-shaken out entirely.
import {
  createEligibility,
  createProcessCheckGate,
  createUpdateController,
  definePickforgeUpdaterElement,
  type UpdateAdapter,
  type UpdateDialogMetadata,
} from "@pickforge/tauri-updater";
import type { UpdateDialogHost } from "./updateDialog";

const FIXTURE_UPDATE = {
  version: "0.3.0",
  notes: "Faster refresh polling\nFixed a tray icon flicker on Linux\nSmaller install size",
};

function fixtureAdapter(scenario: string): UpdateAdapter {
  return {
    async check() {
      return FIXTURE_UPDATE;
    },
    async downloadAndInstall(onEvent) {
      onEvent({ type: "started", contentLength: 100 });
      if (scenario !== "downloading") {
        onEvent({ type: "progress", chunkLength: 100 });
        onEvent({ type: "finished" });
        return;
      }
      // Report some progress, then hang forever so "downloading" is a
      // stable, screenshot-able state instead of racing to "restarting".
      onEvent({ type: "progress", chunkLength: 42 });
      await new Promise<void>(() => {});
    },
    async relaunch() {},
  };
}

const FIXTURE_METADATA: UpdateDialogMetadata = {
  productName: "PickGauge",
  currentVersion: "0.2.0",
  productMark: "PG",
};

export async function mountUpdateDialogFixture(
  target: HTMLElement,
  scenario: string,
): Promise<void> {
  target.replaceChildren();
  definePickforgeUpdaterElement();
  const element = document.createElement("pickforge-update-dialog") as unknown as UpdateDialogHost &
    HTMLElement;
  target.append(element);

  const controller = createUpdateController({
    adapter: fixtureAdapter(scenario),
    eligibility: createEligibility({ packaged: true, mainWindow: true, visible: true, focused: true }),
    gate: createProcessCheckGate(),
  });
  element.metadata = FIXTURE_METADATA;
  element.controller = controller;
  await controller.start();

  if (scenario === "downloading") {
    void controller.install();
  }
}
