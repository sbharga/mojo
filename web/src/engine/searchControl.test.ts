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

  it("stops after the soft deadline", () => {
    expect(shouldStopBeforeNextIteration({
      elapsedMs: 500,
      thinkTimeMs: 1_000,
      softTimeFraction: 0.5,
      predictedNextMs: 1,
      ebfGateOverride: false,
      multiPv: 1,
    })).toBe(true);
  });

  it("uses the purpose-specific prediction margin before another depth", () => {
    const input = {
      elapsedMs: 600,
      thinkTimeMs: 1_000,
      softTimeFraction: 1,
      predictedNextMs: 550,
      ebfGateOverride: false,
    };
    expect(shouldStopBeforeNextIteration({ ...input, multiPv: 1 })).toBe(true);
    expect(shouldStopBeforeNextIteration({ ...input, multiPv: 3 })).toBe(false);
  });

  it("honors an engine override for an unusually favorable next iteration", () => {
    expect(shouldStopBeforeNextIteration({
      elapsedMs: 700,
      thinkTimeMs: 1_000,
      softTimeFraction: 1,
      predictedNextMs: 10_000,
      ebfGateOverride: true,
      multiPv: 1,
    })).toBe(false);
  });
});
