import { getCurrentWindow } from "@tauri-apps/api/window";

const INTERACTIVE_TITLEBAR_SELECTOR =
  "button, a, input, select, textarea, [role='button'], [contenteditable='true']";

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
  try {
    const win = getCurrentWindow();
    await win.toggleMaximize();
    return await win.isMaximized();
  } catch {
    return false;
  }
}

async function startWindowDrag(): Promise<void> {
  try {
    await getCurrentWindow().startDragging();
  } catch {}
}

export function handleTitlebarMouseDown(event: MouseEvent): void {
  const target = event.target as { closest?: (selector: string) => Element | null } | null;
  if (event.button !== 0 || target?.closest?.(INTERACTIVE_TITLEBAR_SELECTOR)) {
    return;
  }

  event.preventDefault();
  event.stopPropagation();
  if (event.detail === 2) {
    void toggleMaximizeWindow();
  } else {
    void startWindowDrag();
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
