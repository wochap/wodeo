import { describe, expect, it } from "vitest";
import { isAltArrow, navigationTarget } from "./navigation";

const ctx = {
  value: 30_000_000,
  duration: 100_000_000,
  step: 40_000,
  keyframes: [0, 10_000_000, 30_000_000, 45_000_000],
};
const k = (key: string, mods: Partial<Record<string, boolean>> = {}) => ({
  key,
  shiftKey: false,
  altKey: false,
  ctrlKey: false,
  metaKey: false,
  ...mods,
});

describe("navigationTarget", () => {
  it("steps one frame with arrows", () => {
    expect(navigationTarget(k("ArrowLeft"), ctx)).toBe(29_960_000);
    expect(navigationTarget(k("ArrowRight"), ctx)).toBe(30_040_000);
  });
  it("steps one second with Shift", () => {
    expect(navigationTarget(k("ArrowLeft", { shiftKey: true }), ctx)).toBe(
      29_000_000,
    );
    expect(navigationTarget(k("ArrowRight", { shiftKey: true }), ctx)).toBe(
      31_000_000,
    );
  });
  it("steps ten seconds with PageUp/PageDown", () => {
    expect(navigationTarget(k("PageUp"), ctx)).toBe(40_000_000);
    expect(navigationTarget(k("PageDown"), ctx)).toBe(20_000_000);
  });
  it("jumps to strictly adjacent keyframes with Alt, taking precedence over Shift", () => {
    expect(navigationTarget(k("ArrowLeft", { altKey: true }), ctx)).toBe(
      10_000_000,
    );
    expect(
      navigationTarget(k("ArrowRight", { altKey: true, shiftKey: true }), ctx),
    ).toBe(45_000_000);
  });
  it("returns null for Alt with no keyframe in that direction", () => {
    expect(
      navigationTarget(k("ArrowRight", { altKey: true }), {
        ...ctx,
        value: 50_000_000,
      }),
    ).toBeNull();
    expect(
      navigationTarget(k("ArrowLeft", { altKey: true }), {
        ...ctx,
        keyframes: [],
      }),
    ).toBeNull();
  });
  it("jumps to tenths of the duration with digits", () => {
    expect(navigationTarget(k("0"), ctx)).toBe(0);
    expect(navigationTarget(k("5"), ctx)).toBe(50_000_000);
    // NumLock-on numpad keys report the digit in `e.key`.
    expect(navigationTarget(k("9"), ctx)).toBe(90_000_000);
  });
  it("ignores digits with Ctrl, Alt or Meta", () => {
    for (const m of ["ctrlKey", "altKey", "metaKey"])
      expect(navigationTarget(k("3", { [m]: true }), ctx)).toBeNull();
  });
  it("ignores other keys", () => {
    expect(navigationTarget(k("a"), ctx)).toBeNull();
    expect(navigationTarget(k("Home"), ctx)).toBeNull();
  });
  it("clamps at both ends", () => {
    expect(navigationTarget(k("PageDown"), { ...ctx, value: 5 })).toBe(0);
    expect(navigationTarget(k("PageUp"), { ...ctx, value: 95_000_000 })).toBe(
      100_000_000,
    );
  });
  it("detects Alt+arrows", () => {
    expect(isAltArrow(k("ArrowLeft", { altKey: true }))).toBe(true);
    expect(isAltArrow(k("ArrowLeft"))).toBe(false);
  });
});
