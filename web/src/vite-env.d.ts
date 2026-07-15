/// <reference types="vite/client" />
declare module "../../../engine/pkg/mojo_engine.js" {
  export default function init(): Promise<void>;
  export class Engine {
    constructor();
    set_stop_flag(stopFlag: Int32Array): void;
    set_stop_request(requestId: number): void;
    seed_pv(
      moves: string[],
      depth: number,
      scoreCp?: number,
      mateIn?: number,
    ): number;
    set_position(fen: string, priorFens: string[]): void;
    analyze_depth(
      depth: number,
      multiPv: number,
      timeLimitMs: number,
    ): AnalysisResult;
    fallback_move(): string | undefined;
    book_move?(seed: number): string | undefined;
    free(): void;
  }
  interface AnalysisLine {
    score_cp: number | null;
    mate_in: number | null;
    moves: string[];
  }
  interface AnalysisResult {
    depth: number;
    nodes: number;
    root_node_fraction: number;
    soft_time_fraction: number;
    predicted_next_ms: number;
    ebf_gate_override: boolean;
    clock_check_interval: number;
    elapsed_ms: number;
    timed_out: boolean;
    lines: AnalysisLine[];
  }
}
