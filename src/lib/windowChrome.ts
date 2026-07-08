import { getCurrentWindow } from "@tauri-apps/api/window";

export type ResizeDir =
  | "North"
  | "South"
  | "East"
  | "West"
  | "NorthWest"
  | "NorthEast"
  | "SouthWest"
  | "SouthEast";

export function minimizeWindow(): Promise<void> {
  return getCurrentWindow()
    .minimize()
    .catch(() => {});
}

export function closeWindow(): Promise<void> {
  return getCurrentWindow()
    .close()
    .catch(() => {});
}

export async function toggleMaximizeWindow(): Promise<boolean> {
  const win = getCurrentWindow();
  try {
    await win.toggleMaximize();
    return await win.isMaximized();
  } catch {
    return false;
  }
}

export async function readMaximized(): Promise<boolean> {
  try {
    return await getCurrentWindow().isMaximized();
  } catch {
    return false;
  }
}

export function startResize(dir: ResizeDir): Promise<void> {
  return getCurrentWindow()
    .startResizeDragging(dir)
    .catch(() => {});
}
