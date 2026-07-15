import { describe, expect, it } from "vitest";
import { supportsWasmSimd } from "./wasmFeatures";

describe("Wasm feature detection", () => {
  it("recognizes fixed-width SIMD in the current runtime", () => {
    expect(supportsWasmSimd()).toBe(true);
  });
});
