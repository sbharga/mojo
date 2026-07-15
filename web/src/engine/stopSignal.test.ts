import { afterEach, describe, expect, it, vi } from "vitest";
import { cancelThrough, createStopSignal, isCancelled } from "./stopSignal";

afterEach(() => vi.unstubAllGlobals());

describe("shared search stop signal", () => {
  it("uses a monotonic request watermark", () => {
    const signal = new Int32Array(new SharedArrayBuffer(4));
    cancelThrough(signal, 4);
    expect(isCancelled(signal, 4)).toBe(true);
    expect(isCancelled(signal, 5)).toBe(false);
    cancelThrough(signal, 5);
    expect(isCancelled(signal, 5)).toBe(true);
  });

  it("is enabled only for cross-origin-isolated pages", () => {
    vi.stubGlobal("crossOriginIsolated", false);
    expect(createStopSignal()).toBeNull();
    vi.stubGlobal("crossOriginIsolated", true);
    expect(createStopSignal()).not.toBeNull();
  });
});
