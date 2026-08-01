import { describe, expect, it } from "vitest";
import {
  iterationBudget,
  MAX_SEARCH_DEPTH,
  shouldStopBeforeNextIteration,
} from "./searchControl";

describe("worker search-control policy", () => {
  it("keeps an iteration budget positive when the deadline has just elapsed", () => {
    expect(iterationBudget(-4)).toBe(8);
    expect(iterationBudget(125)).toBe(125);
    expect(MAX_SEARCH_DEPTH).toBe(32);
  });

  it("stops after the soft deadline regardless of purpose", () => {
    const input = {
      elapsedMs: 500,
      thinkTimeMs: 1_000,
      softTimeFraction: 0.5,
      predictedNextMs: 1,
      ebfGateOverride: false,
    };
    expect(shouldStopBeforeNextIteration({ ...input, multiPv: 1 })).toBe(true);
    expect(shouldStopBeforeNextIteration({ ...input, multiPv: 3 })).toBe(true);
  });

  it("never applies the prediction gate to a move search, since a timed-out iteration still returns a sound partial", () => {
    expect(shouldStopBeforeNextIteration({
      elapsedMs: 900,
      thinkTimeMs: 1_000,
      softTimeFraction: 1,
      predictedNextMs: 10_000,
      ebfGateOverride: false,
      multiPv: 1,
    })).toBe(false);
  });

  it("stops an analysis search before a predicted overrun, since multi-PV has no partial fallback", () => {
    // remaining = 400ms; 700 > 400 * 1.5, so the next depth is predicted to
    // overrun the deadline and analysis stops here instead of starting it.
    expect(shouldStopBeforeNextIteration({
      elapsedMs: 600,
      thinkTimeMs: 1_000,
      softTimeFraction: 1,
      predictedNextMs: 700,
      ebfGateOverride: false,
      multiPv: 3,
    })).toBe(true);
  });

  it("honors an engine override for an unusually favorable next analysis iteration", () => {
    expect(shouldStopBeforeNextIteration({
      elapsedMs: 700,
      thinkTimeMs: 1_000,
      softTimeFraction: 1,
      predictedNextMs: 10_000,
      ebfGateOverride: true,
      multiPv: 3,
    })).toBe(false);
  });
});
