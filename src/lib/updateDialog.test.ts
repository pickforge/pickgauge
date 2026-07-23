import { createUpdateController, type UpdateAdapter } from "@pickforge/tauri-updater";
import { describe, expect, it, vi } from "vitest";
import {
  createMainWindowEligibility,
  MAIN_WINDOW_LABEL,
  type EligibilityWindow,
} from "./updateDialog";

/** Stateful fake so tests can flip visibility/focus and emit focus-changed
 * events the way a real Tauri window would, instead of a fresh mock per
 * assertion. */
class FakeEligibilityWindow implements EligibilityWindow {
  label: string;
  visible: boolean;
  focused: boolean;
  listeners: ((event: { payload: boolean }) => void)[] = [];
  /** Fires synchronously inside `onFocusChanged`, before it resolves — lets
   * tests simulate the window's state changing during listener registration. */
  onRegister?: () => void;

  constructor(label: string, opts: { visible?: boolean; focused?: boolean } = {}) {
    this.label = label;
    this.visible = opts.visible ?? false;
    this.focused = opts.focused ?? false;
  }

  isVisible = vi.fn(async () => this.visible);
  isFocused = vi.fn(async () => this.focused);
  onFocusChanged = vi.fn(async (listener: (event: { payload: boolean }) => void) => {
    this.listeners.push(listener);
    this.onRegister?.();
    return () => {
      this.listeners = this.listeners.filter((l) => l !== listener);
    };
  });

  emitFocus(focused: boolean) {
    this.focused = focused;
    for (const listener of this.listeners) {
      listener({ payload: focused });
    }
  }
}

/** Flushes pending microtasks (the `currentlyEligible()` `Promise.all`
 * round-trip inside `createMainWindowEligibility` takes a couple of ticks),
 * more reliably than a fixed number of `await Promise.resolve()` calls. */
async function flush() {
  await new Promise((resolve) => setTimeout(resolve, 0));
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
    const win = new FakeEligibilityWindow("float", { visible: true, focused: true });
    const eligibility = createMainWindowEligibility(win, true);

    await expect(eligibility.whenEligible()).resolves.toBe(false);
    expect(win.isVisible).not.toHaveBeenCalled();
    expect(win.onFocusChanged).not.toHaveBeenCalled();
  });

  it("never becomes eligible outside a packaged build", async () => {
    const win = new FakeEligibilityWindow(MAIN_WINDOW_LABEL, { visible: true, focused: true });
    const eligibility = createMainWindowEligibility(win, false);

    await expect(eligibility.whenEligible()).resolves.toBe(false);
    expect(win.isVisible).not.toHaveBeenCalled();
  });

  it("resolves immediately for an already-visible and focused main window", async () => {
    const win = new FakeEligibilityWindow(MAIN_WINDOW_LABEL, { visible: true, focused: true });
    const eligibility = createMainWindowEligibility(win, true);

    await expect(eligibility.whenEligible()).resolves.toBe(true);
    expect(win.onFocusChanged).not.toHaveBeenCalled();
  });

  it("defers a visible-but-unfocused main window until it gains focus", async () => {
    const win = new FakeEligibilityWindow(MAIN_WINDOW_LABEL, { visible: true, focused: false });
    const eligibility = createMainWindowEligibility(win, true);

    const pending = eligibility.whenEligible();
    await flush();
    expect(win.onFocusChanged).toHaveBeenCalledOnce();

    win.emitFocus(true);
    await expect(pending).resolves.toBe(true);
  });

  it("stays ineligible when a focus event fires without the window becoming visible", async () => {
    const win = new FakeEligibilityWindow(MAIN_WINDOW_LABEL, { visible: false, focused: false });
    const eligibility = createMainWindowEligibility(win, true);

    const pending = eligibility.whenEligible();
    await flush();

    // Focused, but still hidden (e.g. a tray/login-start window regaining
    // OS focus while its main window stays hidden) must not resolve.
    win.emitFocus(true);
    await flush();
    let resolved = false;
    void pending.then(() => {
      resolved = true;
    });
    await flush();
    expect(resolved).toBe(false);

    // Once it is actually shown and re-focused, it resolves.
    win.visible = true;
    win.emitFocus(true);
    await expect(pending).resolves.toBe(true);
  });

  it("defers a hidden main window until it is visible and focused, then unlistens", async () => {
    const win = new FakeEligibilityWindow(MAIN_WINDOW_LABEL, { visible: false, focused: false });
    const eligibility = createMainWindowEligibility(win, true);
    const pending = eligibility.whenEligible();
    await flush();

    // A blur (or other unfocus) event before the window is shown must not
    // resolve the prompt.
    win.emitFocus(false);
    await flush();

    win.visible = true;
    win.emitFocus(true);
    await expect(pending).resolves.toBe(true);
    expect(win.listeners).toHaveLength(0);

    // A later, redundant focus event is ignored (single-flight).
    const callsBefore = win.onFocusChanged.mock.calls.length;
    win.emitFocus(true);
    expect(win.onFocusChanged.mock.calls.length).toBe(callsBefore);
  });

  it("never resolves for a hidden main window that stays unfocused", async () => {
    const win = new FakeEligibilityWindow(MAIN_WINDOW_LABEL, { visible: false, focused: false });
    const eligibility = createMainWindowEligibility(win, true);
    let resolved = false;
    void eligibility.whenEligible().then(() => {
      resolved = true;
    });

    await flush();
    expect(resolved).toBe(false);
  });

  it("does not miss eligibility reached while the focus listener registers", async () => {
    // The window becomes visible and focused synchronously as a side effect
    // of registering the listener (simulating a real focus/show event
    // landing in the gap between the initial check and the listener being
    // wired up) — whenEligible() must still resolve without waiting for a
    // subsequent focus event that will never come.
    const win = new FakeEligibilityWindow(MAIN_WINDOW_LABEL, { visible: false, focused: false });
    win.onRegister = () => {
      win.visible = true;
      win.focused = true;
    };
    const eligibility = createMainWindowEligibility(win, true);

    await expect(eligibility.whenEligible()).resolves.toBe(true);
  });
});

describe("capsule and hidden-window exclusion through the shared controller", () => {
  it("never checks for updates when mounted for the floating capsule window", async () => {
    const adapter = fakeAdapter();
    const win = new FakeEligibilityWindow("float", { visible: true, focused: true });
    const controller = createUpdateController({
      adapter,
      eligibility: createMainWindowEligibility(win, true),
    });

    await controller.start();

    expect(adapter.check).not.toHaveBeenCalled();
    expect(controller.getState()).toEqual({ status: "idle" });
  });

  it("defers the update check for a hidden main window until it is shown and focused", async () => {
    const adapter = fakeAdapter();
    const win = new FakeEligibilityWindow(MAIN_WINDOW_LABEL, { visible: false, focused: false });
    const controller = createUpdateController({
      adapter,
      eligibility: createMainWindowEligibility(win, true),
    });

    const started = controller.start();
    await flush();
    expect(adapter.check).not.toHaveBeenCalled();

    win.visible = true;
    win.emitFocus(true);
    await started;
    expect(adapter.check).toHaveBeenCalledOnce();
  });
});
