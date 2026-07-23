// Feature-flag registry (@pickforge/flags). Mirrors the shape PickForge's
// src/stores/flags.ts uses for the same package: definitions default off
// unless `default: true` is set, and a launch flip means changing the
// definition and releasing. PickGauge has no flag-override UI yet, so this
// uses the package's default in-memory store — an override only lives for
// the current process, which is enough for manual dev testing.
import { createFlags } from "@pickforge/flags";

const definitions = {
  studioUpdateDialog: {
    description: "Shared Pickforge update dialog (pickforge/pickforge-platform#36)",
  },
} as const;

export type FlagKey = keyof typeof definitions;

const flags = createFlags(definitions);

export function flagEnabled(key: FlagKey): boolean {
  return flags.isEnabled(key);
}

export function setFlagOverride(key: FlagKey, value: boolean | undefined): void {
  flags.setOverride(key, value);
}
