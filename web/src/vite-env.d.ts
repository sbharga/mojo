/// <reference types="vite/client" />
declare module "../../../engine/pkg/mojo_engine.js" {
  export default function init(): Promise<void>;
  export class Engine {
    constructor();
    set_position(fen: string, priorFens: string[]): void;
    analyze_depth(
      depth: number,
      multiPv: number,
      timeLimitMs: number,
    ): AnalysisResult;
    fallback_move(): string | undefined;
    free(): void;
  }
  export function analyze_step(
    fen: string,
    depth: number,
    multiPv: number,
    timeLimitMs: number,
  ): AnalysisResult;
  export function fallback_move(fen: string): string | undefined;
  interface AnalysisLine {
    score_cp: number | null;
    mate_in: number | null;
    moves: string[];
  }
  interface AnalysisResult {
    depth: number;
    nodes: number;
    elapsed_ms: number;
    timed_out: boolean;
    lines: AnalysisLine[];
  }
}
