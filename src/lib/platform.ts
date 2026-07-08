export type HostPlatform = "macos" | "windows" | "linux" | "web";
export type WindowControl = "minimize" | "maximize" | "close";

let cached: HostPlatform | undefined;

export function isTauri(): boolean {
  return (
    typeof window !== "undefined" &&
    "__TAURI_INTERNALS__" in (window as Window & { __TAURI_INTERNALS__?: unknown })
  );
}

export function hostPlatform(): HostPlatform {
  if (cached) {
    return cached;
  }
  if (!isTauri()) {
    return (cached = "web");
  }
  const ua = typeof navigator !== "undefined" ? navigator.userAgent : "";
  if (/Macintosh|Mac OS X/.test(ua)) {
    cached = "macos";
  } else if (/Windows/.test(ua)) {
    cached = "windows";
  } else {
    cached = "linux";
  }
  return cached;
}

export function controlsSide(platform: HostPlatform): "left" | "right" {
  return platform === "macos" ? "left" : "right";
}

export function controlOrder(platform: HostPlatform): WindowControl[] {
  return platform === "macos"
    ? ["close", "minimize", "maximize"]
    : ["minimize", "maximize", "close"];
}
