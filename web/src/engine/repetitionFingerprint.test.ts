import { describe, expect, it } from "vitest";
import { repetitionFingerprint } from "./repetitionFingerprint";

const root = "8/8/8/8/8/8/8/K6k w - - 2 10";
const a = "8/8/8/8/8/8/K7/7k b - - 1 9";
const b = "8/8/8/8/8/8/1K6/7k b - - 1 9";
const old = "8/8/8/8/8/8/2K5/7k b - - 9 1";

describe("repetition fingerprint", () => {
  it("is order-independent but preserves multiplicity", () => {
    expect(repetitionFingerprint(root, [a, b])).toBe(repetitionFingerprint(root, [b, a]));
    expect(repetitionFingerprint(root, [a, a])).not.toBe(repetitionFingerprint(root, [a, b]));
  });

  it("ignores history before the reversible-move window", () => {
    expect(repetitionFingerprint(root, [old, a, b])).toBe(
      repetitionFingerprint(root, [a, b]),
    );
    const reset = root.replace(" 2 10", " 0 10");
    expect(repetitionFingerprint(reset, [a])).toBe("0000000000000000");
  });
});
