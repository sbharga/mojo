import { describe, expect, it } from "vitest";
import { AnalysisCache } from "./analysisCache";
import type { Analysis } from "./types";
import { repetitionFingerprint } from "./repetitionFingerprint";

function makeAnalysis(overrides: Partial<Analysis> = {}): Analysis {
  return {
    root_fen: "fen-a",
    repetition_fingerprint: "0000000000000000",
    depth: 4,
    nodes: 100,
    root_node_fraction: 0.5,
    soft_time_fraction: 0.5,
    predicted_next_ms: 100,
    ebf_gate_override: false,
    clock_check_interval: 256,
    elapsed_ms: 10,
    timed_out: false,
    lines: [{ score_cp: 20, moves: ["e2e4"] }],
    ...overrides,
  };
}

describe("AnalysisCache", () => {
  it("misses on an unknown fen", () => {
    const cache = new AnalysisCache();
    expect(cache.get("fen-a", [], 1)).toBeNull();
  });

  it("hits once a result has been stored", () => {
    const cache = new AnalysisCache();
    const analysis = makeAnalysis();
    cache.set(analysis);
    expect(cache.get("fen-a", [], 1)).toBe(analysis);
  });

  it("misses when the cached entry has fewer lines than required", () => {
    const cache = new AnalysisCache();
    cache.set(makeAnalysis({ lines: [{ moves: ["e2e4"] }] }));
    expect(cache.get("fen-a", [], 3)).toBeNull();
    expect(cache.get("fen-a", [], 1)).not.toBeNull();
  });

  it("does not let a shallower result overwrite a deeper one for the same fen", () => {
    const cache = new AnalysisCache();
    const deep = makeAnalysis({ depth: 10, lines: [{ moves: ["e2e4"] }, { moves: ["d2d4"] }, { moves: ["c2c4"] }] });
    cache.set(deep);
    cache.set(makeAnalysis({ depth: 1, lines: [{ moves: ["a2a3"] }] }));
    expect(cache.get("fen-a", [], 1)).toBe(deep);
  });

  it("replaces an entry with a deeper result for the same fen", () => {
    const cache = new AnalysisCache();
    cache.set(makeAnalysis({ depth: 1 }));
    const deeper = makeAnalysis({ depth: 5 });
    cache.set(deeper);
    expect(cache.get("fen-a", [], 1)).toBe(deeper);
  });

  it("separates identical FENs reached through different reversible histories", () => {
    const cache = new AnalysisCache();
    const fen = "8/8/8/8/8/8/8/K6k w - - 2 10";
    const history = [
      "8/8/8/8/8/8/K7/7k b - - 1 9",
      "8/8/8/8/8/8/1K6/7k b - - 1 9",
    ];
    const stored = makeAnalysis({
      root_fen: fen,
      repetition_fingerprint: repetitionFingerprint(fen, history),
    });
    cache.set(stored);
    expect(cache.get(fen, history, 1)).toBe(stored);
    expect(cache.get(fen, [history[0], history[0]], 1)).toBeNull();
  });

  it("evicts the oldest entry once past capacity", () => {
    const cache = new AnalysisCache();
    for (let i = 0; i < 257; i++) {
      cache.set(makeAnalysis({ root_fen: `fen-${i}` }));
    }
    expect(cache.get("fen-0", [], 1)).toBeNull();
    expect(cache.get("fen-256", [], 1)).not.toBeNull();
  });
});
