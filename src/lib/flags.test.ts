import { describe, expect, it } from "vitest";
import { flagEnabled, setFlagOverride } from "./flags";

describe("studioUpdateDialog flag", () => {
  it("defaults to off", () => {
    expect(flagEnabled("studioUpdateDialog")).toBe(false);
  });

  it("can be flipped on for the current process via an override", () => {
    setFlagOverride("studioUpdateDialog", true);
    expect(flagEnabled("studioUpdateDialog")).toBe(true);

    setFlagOverride("studioUpdateDialog", undefined);
    expect(flagEnabled("studioUpdateDialog")).toBe(false);
  });
});
