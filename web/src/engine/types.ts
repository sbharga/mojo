export type EngineMode =
  | "human-engine"
  | "human-stockfish"
  | "engine-engine"
  | "mojo-stockfish"
  | "human-human";
export type Side = "white" | "black";

export interface AnalysisLine {
  score_cp?: number;
  mate_in?: number;
  moves: string[];
}

export interface Analysis {
  root_fen: string;
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

export interface AnalyzeRequest {
  type: "analyze";
  requestId: number;
  fen: string;
  historyFens: string[];
  thinkTimeMs: number;
  purpose: "analysis" | "move";
  seed?: SearchSeed;
}

export interface SearchSeed {
  moves: string[];
  depth: number;
  score_cp?: number;
  mate_in?: number;
}

export interface InitializeRequest {
  type: "initialize";
  stopBuffer?: SharedArrayBuffer;
}
export interface CancelRequest {
  type: "cancel";
  requestId: number;
}
export type WorkerRequest = InitializeRequest | AnalyzeRequest | CancelRequest;

export interface AnalysisMessage {
  type: "analysis";
  requestId: number;
  analysis: Analysis;
}
export interface CompleteMessage {
  type: "complete";
  requestId: number;
  purpose: "analysis" | "move";
  analysis: Analysis | null;
}
export interface ErrorMessage {
  type: "error";
  requestId: number;
  message: string;
}
export interface ReadyMessage {
  type: "ready";
}
export type WorkerMessage =
  | AnalysisMessage
  | CompleteMessage
  | ErrorMessage
  | ReadyMessage;
