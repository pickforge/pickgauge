import { createUpdateController, type UpdateAdapter } from "@pickforge/tauri-updater";
import { describe, expect, it, vi } from "vitest";
import {
  createMainWindowEligibility,
  MAIN_WINDOW_LABEL,
  type EligibilityWindow,
} from "./updateDialog";

function fakeWindow(overrides: Partial<EligibilityWindow> & { label: string }): EligibilityWindow {
  return {
    isVisible: vi.fn(async () => false),
    onFocusChanged: vi.fn(async () => () => {}),
    ...overrides,
  };
}

function fakeAdapter(): UpdateAdapter {
  return {
    check: vi.fn(async () => ({ version: "1.2.3" })),
    downloadAndInstall: vi.fn(async () => {}),
    relaunch: vi.fn(async () => {}),
  };
}

describe("createMainWindowEligibility", () => {
  it("never becomes eligible for the floating capsule window", async () => {
    const win = fakeWindow({ label: "float", isVisible: vi.fn(async () => true) });
    const eligibility = createMainWindowEligibility(win, true);

    await expect(eligibility.whenEligible()).resolves.toBe(false);
    expect(win.isVisible).not.toHaveBeenCalled();
    expect(win.onFocusChanged).not.toHaveBeenCalled();
  });

  it("never becomes eligible outside a packaged build", async () => {
    const win = fakeWindow({ label: MAIN_WINDOW_LABEL, isVisible: vi.fn(async () => true) });
    const eligibility = createMainWindowEligibility(win, false);

    await expect(eligibility.whenEligible()).resolves.toBe(false);
    expect(win.isVisible).not.toHaveBeenCalled();
  });

  it("resolves immediately for an already-visible main window", async () => {
    const win = fakeWindow({ label: MAIN_WINDOW_LABEL, isVisible: vi.fn(async () => true) });
    const eligibility = createMainWindowEligibility(win, true);

    await expect(eligibility.whenEligible()).resolves.toBe(true);
    expect(win.onFocusChanged).not.toHaveBeenCalled();
  });

  it("defers a hidden main window until it gains focus, then unlistens", async () => {
    let handler: ((event: { payload: boolean }) => void) | undefined;
    const unlisten = vi.fn();
    const win = fakeWindow({
      label: MAIN_WINDOW_LABEL,
      isVisible: vi.fn(async () => false),
      onFocusChanged: vi.fn(async (h) => {
        handler = h;
        return unlisten;
      }),
    });
    const eligibility = createMainWindowEligibility(win, true);
    const pending = eligibility.whenEligible();
    await Promise.resolve();
    await Promise.resolve();

    // A blur (or other unfocus) event before the window is shown must not
    // resolve the prompt.
    handler?.({ payload: false });
    await Promise.resolve();

    handler?.({ payload: true });
    await expect(pending).resolves.toBe(true);
    expect(unlisten).toHaveBeenCalledOnce();

    // A later, redundant focus event is ignored (single-flight).
    handler?.({ payload: true });
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("never resolves for a hidden main window that stays unfocused", async () => {
    const win = fakeWindow({
      label: MAIN_WINDOW_LABEL,
      isVisible: vi.fn(async () => false),
    });
    const eligibility = createMainWindowEligibility(win, true);
    let resolved = false;
    void eligibility.whenEligible().then(() => {
      resolved = true;
    });

    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
    expect(resolved).toBe(false);
  });
});

describe("capsule and hidden-window exclusion through the shared controller", () => {
  it("never checks for updates when mounted for the floating capsule window", async () => {
    const adapter = fakeAdapter();
    const win = fakeWindow({ label: "float" });
    const controller = createUpdateController({
      adapter,
      eligibility: createMainWindowEligibility(win, true),
    });

    await controller.start();

    expect(adapter.check).not.toHaveBeenCalled();
    expect(controller.getState()).toEqual({ status: "idle" });
  });

  it("defers the update check for a hidden main window until it is shown", async () => {
    const adapter = fakeAdapter();
    let handler: ((event: { payload: boolean }) => void) | undefined;
    const win = fakeWindow({
      label: MAIN_WINDOW_LABEL,
      isVisible: vi.fn(async () => false),
      onFocusChanged: vi.fn(async (h) => {
        handler = h;
        return () => {};
      }),
    });
    const controller = createUpdateController({
      adapter,
      eligibility: createMainWindowEligibility(win, true),
    });

    const started = controller.start();
    await Promise.resolve();
    await Promise.resolve();
    expect(adapter.check).not.toHaveBeenCalled();

    handler?.({ payload: true });
    await started;
    expect(adapter.check).toHaveBeenCalledOnce();
  });
});
