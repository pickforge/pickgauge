// Single source of truth for which update flow may run where, so the legacy
// `window.confirm` updater (./updater.ts, driven from main.ts) and the shared
// dialog (./updateDialog.ts, mounted from App.svelte) can never both prompt
// for the same window. Both derive from the same `studioUpdateDialog` flag
// value (see ./flags.ts) through these two functions instead of duplicating
// the condition inline in each call site.

/** True only for the main window while the flag is off — the legacy
 * `checkForUpdatesWhenVisible` flow. */
export function shouldRunLegacyUpdateCheck(windowLabel: string, studioDialogEnabled: boolean): boolean {
  return windowLabel === "main" && !studioDialogEnabled;
}

/** True once the flag is on — App.svelte (the main-window component; the
 * floating capsule never imports the dialog module at all) mounts the shared
 * controller only when this is true. */
export function shouldMountSharedUpdateDialog(studioDialogEnabled: boolean): boolean {
  return studioDialogEnabled;
}
