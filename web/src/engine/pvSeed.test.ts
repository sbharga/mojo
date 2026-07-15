import { Chess } from "chess.js";
import { describe, expect, it } from "vitest";
import { ponderSeed } from "./pvSeed";
import type { Analysis } from "./types";

function analysis(overrides: Partial<Analysis> = {}): Analysis {
  return {
    root_fen: new Chess().fen(),
    repetition_fingerprint: "0000000000000000",
    depth: 8,
    nodes: 1_000,
    root_node_fraction: 0.5,
    soft_time_fraction: 0.5,
    predicted_next_ms: 10,
    ebf_gate_override: false,
    clock_check_interval: 256,
    elapsed_ms: 20,
    timed_out: false,
    lines: [{ score_cp: 30, moves: ["e2e4", "e7e5", "g1f3"] }],
    ...overrides,
  };
}

describe("ponder PV seeding", () => {
  it("forwards the suffix with the new side-to-move score", () => {
    const position = new Chess();
    position.move("e4");
    expect(ponderSeed(position.fen(), analysis())).toEqual({
      moves: ["e7e5", "g1f3"],
      depth: 7,
      score_cp: -30,
    });
  });

  it("flips mate scores and rejects a missed ponder", () => {
    const position = new Chess();
    position.move("e4");
    const mate = analysis({ lines: [{ mate_in: 3, moves: ["e2e4", "e7e5"] }] });
    expect(ponderSeed(position.fen(), mate)?.mate_in).toBe(-3);
    position.move("c5");
    expect(ponderSeed(position.fen(), analysis())).toBeNull();
  });
});
