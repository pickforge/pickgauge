import { afterEach, describe, expect, it, vi } from "vitest";
import { controlOrder, controlsSide, hostPlatform, isTauri } from "./platform";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetModules();
});

async function detectPlatform(ua: string) {
  vi.resetModules();
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
  vi.stubGlobal("navigator", { userAgent: ua });
  const mod = await import("./platform");
  return mod.hostPlatform();
}

describe("isTauri", () => {
  it("is false in a plain browser", () => {
    vi.stubGlobal("window", {});
    expect(isTauri()).toBe(false);
  });

  it("is true when the Tauri internals are present", () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    expect(isTauri()).toBe(true);
  });
});

describe("hostPlatform", () => {
  it('reports "web" outside Tauri', () => {
    vi.stubGlobal("window", {});
    expect(hostPlatform()).toBe("web");
  });

  it("detects macOS from the user agent under Tauri", async () => {
    expect(await detectPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15)")).toBe("macos");
  });

  it("detects Windows from the user agent under Tauri", async () => {
    expect(await detectPlatform("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe("windows");
  });

  it("falls back to Linux under Tauri", async () => {
    expect(await detectPlatform("Mozilla/5.0 (X11; Linux x86_64)")).toBe("linux");
  });

  it("caches the detected platform", async () => {
    vi.resetModules();
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    vi.stubGlobal("navigator", { userAgent: "Mozilla/5.0 (X11; Linux x86_64)" });
    const mod = await import("./platform");
    expect(mod.hostPlatform()).toBe("linux");
    vi.stubGlobal("navigator", { userAgent: "Mozilla/5.0 (Macintosh)" });
    expect(mod.hostPlatform()).toBe("linux");
  });
});

describe("controlsSide", () => {
  it("puts controls on the left on macOS", () => {
    expect(controlsSide("macos")).toBe("left");
  });

  it("puts controls on the right elsewhere", () => {
    expect(controlsSide("windows")).toBe("right");
    expect(controlsSide("linux")).toBe("right");
    expect(controlsSide("web")).toBe("right");
  });
});

describe("controlOrder", () => {
  it("follows the traffic-light order on macOS", () => {
    expect(controlOrder("macos")).toEqual(["close", "minimize", "maximize"]);
  });

  it("uses minimize → maximize → close on Windows/Linux", () => {
    expect(controlOrder("linux")).toEqual(["minimize", "maximize", "close"]);
    expect(controlOrder("windows")).toEqual(["minimize", "maximize", "close"]);
  });
});
