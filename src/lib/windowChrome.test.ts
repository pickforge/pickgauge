import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const win = {
  minimize: vi.fn(() => Promise.resolve()),
  close: vi.fn(() => Promise.resolve()),
  toggleMaximize: vi.fn(() => Promise.resolve()),
  isMaximized: vi.fn(() => Promise.resolve(false)),
  startDragging: vi.fn(() => Promise.resolve()),
  startResizeDragging: vi.fn(() => Promise.resolve()),
};

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => win,
}));

import {
  closeWindow,
  handleTitlebarMouseDown,
  minimizeWindow,
  readMaximized,
  startResize,
  toggleMaximizeWindow,
} from "./windowChrome";

function mouseDown(detail = 2, button = 0, interactive = false): MouseEvent {
  return {
    button,
    detail,
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
    target: {
      closest: vi.fn(() => (interactive ? {} : null)),
    },
  } as unknown as MouseEvent;
}

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

describe("handleTitlebarMouseDown", () => {
  it("toggles maximize for a primary-button double click", async () => {
    const event = mouseDown();
    handleTitlebarMouseDown(event);

    await vi.waitFor(() => expect(win.toggleMaximize).toHaveBeenCalledOnce());
    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(event.stopPropagation).toHaveBeenCalledOnce();
  });

  it("ignores interactive titlebar children", async () => {
    handleTitlebarMouseDown(mouseDown(2, 0, true));

    await Promise.resolve();
    expect(win.toggleMaximize).not.toHaveBeenCalled();
  });

  it("ignores non-primary double clicks", async () => {
    handleTitlebarMouseDown(mouseDown(2, 2));

    await Promise.resolve();
    expect(win.toggleMaximize).not.toHaveBeenCalled();
  });

  it("starts window dragging for a single press", async () => {
    const event = mouseDown(1);
    handleTitlebarMouseDown(event);

    await vi.waitFor(() => expect(win.startDragging).toHaveBeenCalledOnce());
    expect(win.toggleMaximize).not.toHaveBeenCalled();
    expect(event.stopPropagation).toHaveBeenCalledOnce();
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
