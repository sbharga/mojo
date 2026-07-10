import type { Analysis } from "./types";

export function isCurrentAnalysis(analysis: Analysis | null, fen: string) {
  return analysis?.root_fen === fen;
}

export function bestMoveForPosition(analysis: Analysis | null, fen: string) {
  if (!isCurrentAnalysis(analysis, fen)) return null;
  return analysis?.lines[0]?.moves[0] ?? null;
}

export function formatAnalysisScore(line: Analysis["lines"][number]) {
  if (typeof line.mate_in === "number") return `M${line.mate_in}`;
  const score = line.score_cp ?? 0;
  return `${score >= 0 ? "+" : ""}${(score / 100).toFixed(2)}`;
}
