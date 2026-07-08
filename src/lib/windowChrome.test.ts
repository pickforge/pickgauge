import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const win = {
  minimize: vi.fn(() => Promise.resolve()),
  close: vi.fn(() => Promise.resolve()),
  toggleMaximize: vi.fn(() => Promise.resolve()),
  isMaximized: vi.fn(() => Promise.resolve(false)),
  startResizeDragging: vi.fn(() => Promise.resolve()),
};

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => win,
}));

import {
  closeWindow,
  minimizeWindow,
  readMaximized,
  startResize,
  toggleMaximizeWindow,
} from "./windowChrome";

beforeEach(() => {
  Object.values(win).forEach((fn) => fn.mockClear());
  win.isMaximized.mockResolvedValue(false);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("minimizeWindow", () => {
  it("minimizes the current window", async () => {
    await minimizeWindow();
    expect(win.minimize).toHaveBeenCalledOnce();
  });

  it("swallows rejections", async () => {
    win.minimize.mockRejectedValueOnce(new Error("no window"));
    await expect(minimizeWindow()).resolves.toBeUndefined();
  });
});

describe("closeWindow", () => {
  it("closes the current window", async () => {
    await closeWindow();
    expect(win.close).toHaveBeenCalledOnce();
  });

  it("swallows rejections", async () => {
    win.close.mockRejectedValueOnce(new Error("no window"));
    await expect(closeWindow()).resolves.toBeUndefined();
  });
});

describe("toggleMaximizeWindow", () => {
  it("toggles then reports the new maximized state", async () => {
    win.isMaximized.mockResolvedValueOnce(true);
    await expect(toggleMaximizeWindow()).resolves.toBe(true);
    expect(win.toggleMaximize).toHaveBeenCalledOnce();
  });

  it("returns false when the toggle fails", async () => {
    win.toggleMaximize.mockRejectedValueOnce(new Error("closing"));
    await expect(toggleMaximizeWindow()).resolves.toBe(false);
  });
});

describe("readMaximized", () => {
  it("reads the maximized state", async () => {
    win.isMaximized.mockResolvedValueOnce(true);
    await expect(readMaximized()).resolves.toBe(true);
  });

  it("returns false when the read fails", async () => {
    win.isMaximized.mockRejectedValueOnce(new Error("closing"));
    await expect(readMaximized()).resolves.toBe(false);
  });
});

describe("startResize", () => {
  it("starts a native resize drag in the given direction", async () => {
    await startResize("SouthEast");
    expect(win.startResizeDragging).toHaveBeenCalledWith("SouthEast");
  });

  it("swallows rejections", async () => {
    win.startResizeDragging.mockRejectedValueOnce(new Error("denied"));
    await expect(startResize("North")).resolves.toBeUndefined();
  });
});
