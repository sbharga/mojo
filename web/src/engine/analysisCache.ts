import type { Analysis } from "./types";

const MAX_ENTRIES = 256;

// Positions recur constantly (move-history navigation, pause/resume onto the
// same fen across the 'move'/'analysis' workers) and the fixed-size Wasm TT
// has almost certainly evicted an old position's entries by the time it's
// revisited. This cache lets the UI skip re-searching from depth 1 for a fen
// it has already fully solved.
export class AnalysisCache {
  private entries = new Map<string, Analysis>();

  get(fen: string, minLines: number): Analysis | null {
    const entry = this.entries.get(fen);
    if (!entry || entry.lines.length < minLines) return null;
    this.entries.delete(fen);
    this.entries.set(fen, entry);
    return entry;
  }

  set(fen: string, analysis: Analysis): void {
    const existing = this.entries.get(fen);
    if (
      existing &&
      existing.depth >= analysis.depth &&
      existing.lines.length >= analysis.lines.length
    )
      return;
    this.entries.delete(fen);
    this.entries.set(fen, analysis);
    if (this.entries.size > MAX_ENTRIES) {
      const oldest = this.entries.keys().next().value;
      if (oldest !== undefined) this.entries.delete(oldest);
    }
  }
}
