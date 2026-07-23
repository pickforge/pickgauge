import { describe, expect, it } from "vitest";
import { shouldMountSharedUpdateDialog, shouldRunLegacyUpdateCheck } from "./updateRouting";

describe("update flow routing (no-double-prompt guarantee)", () => {
  it("runs only the legacy check on the main window while the flag is off", () => {
    expect(shouldRunLegacyUpdateCheck("main", false)).toBe(true);
    expect(shouldMountSharedUpdateDialog(false)).toBe(false);
  });

  it("runs only the shared dialog once the flag is on", () => {
    expect(shouldRunLegacyUpdateCheck("main", true)).toBe(false);
    expect(shouldMountSharedUpdateDialog(true)).toBe(true);
  });

  it("never runs the legacy check outside the main window, flag on or off", () => {
    expect(shouldRunLegacyUpdateCheck("float", false)).toBe(false);
    expect(shouldRunLegacyUpdateCheck("float", true)).toBe(false);
  });
});
