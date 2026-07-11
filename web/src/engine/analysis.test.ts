import { describe, expect, it } from "vitest";
import {
  bestMoveForPosition,
  formatAnalysisScore,
  isCurrentAnalysis,
  toWhiteRelative,
} from "./analysis";
import type { Analysis } from "./types";

const result: Analysis = {
  root_fen: "old position",
  depth: 4,
  nodes: 100,
  elapsed_ms: 10,
  timed_out: false,
  lines: [{ score_cp: 20, moves: ["e2e4", "e7e5"] }],
};

describe("analysis roots", () => {
  it("accepts analysis only for the board that produced it", () => {
    expect(isCurrentAnalysis(result, "old position")).toBe(true);
    expect(isCurrentAnalysis(result, "new position")).toBe(false);
    expect(isCurrentAnalysis(null, "old position")).toBe(false);
  });

  it("returns only the top move for the current position", () => {
    expect(bestMoveForPosition(result, "old position")).toBe("e2e4");
    expect(bestMoveForPosition(result, "new position")).toBeNull();
  });

  it("formats mate distance in moves", () => {
    expect(formatAnalysisScore({ mate_in: 3, moves: ["e2e4"] })).toBe("M3");
    expect(formatAnalysisScore({ mate_in: -2, moves: ["e2e4"] })).toBe("M-2");
  });

  it("keeps UI scores consistently relative to White", () => {
    const engineResult = { ...result, lines: [{ score_cp: 1500, mate_in: 3, moves: ["e7e5"] }] };
    const withoutRoot = {
      depth: engineResult.depth,
      nodes: engineResult.nodes,
      elapsed_ms: engineResult.elapsed_ms,
      timed_out: engineResult.timed_out,
      lines: engineResult.lines,
    };

    expect(toWhiteRelative(withoutRoot, "8/8/8/8/8/8/8/8 w - - 0 1").lines[0]).toMatchObject({
      score_cp: 1500,
      mate_in: 3,
    });
    expect(toWhiteRelative(withoutRoot, "8/8/8/8/8/8/8/8 b - - 0 1").lines[0]).toMatchObject({
      score_cp: -1500,
      mate_in: -3,
    });
  });
});
