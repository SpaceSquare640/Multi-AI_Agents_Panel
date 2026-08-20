import { describe, expect, it } from "vitest";
import { isOverSoftCap } from "./Usage";

describe("isOverSoftCap", () => {
  it("is false when no cap is set (blank input)", () => {
    expect(isOverSoftCap(500, "")).toBe(false);
    expect(isOverSoftCap(500, "   ")).toBe(false);
  });

  it("is false while under the cap, true at or above it", () => {
    expect(isOverSoftCap(99, "100")).toBe(false);
    expect(isOverSoftCap(100, "100")).toBe(true);
    expect(isOverSoftCap(101, "100")).toBe(true);
  });

  it("treats a cap of zero as unset rather than 'always over budget'", () => {
    expect(isOverSoftCap(0, "0")).toBe(false);
    expect(isOverSoftCap(1000, "0")).toBe(false);
  });

  it("ignores a negative or non-numeric cap instead of warning nonsensically", () => {
    expect(isOverSoftCap(1000, "-5")).toBe(false);
    expect(isOverSoftCap(1000, "not-a-number")).toBe(false);
  });
});
