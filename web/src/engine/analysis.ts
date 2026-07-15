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
// The engine uses the standard negamax convention (positive means the side to
// move is ahead), while every UI score is shown from White's perspective.
export function toWhiteRelative(
  result: Omit<Analysis, "root_fen" | "repetition_fingerprint">,
  fen: string,
): Omit<Analysis, "root_fen" | "repetition_fingerprint"> {
  if (fen.split(" ")[1] !== "b") return result;
  return {
    ...result,
    lines: result.lines.map((line) => ({
      ...line,
      score_cp: line.score_cp == null ? line.score_cp : -line.score_cp,
      mate_in: line.mate_in == null ? line.mate_in : -line.mate_in,
    })),
  };
}
