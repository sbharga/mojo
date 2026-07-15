import type { Analysis } from "./types";
import { repetitionFingerprint } from "./repetitionFingerprint";

const MAX_ENTRIES = 256;

// Positions recur constantly (move-history navigation, pause/resume onto the
// same fen across the 'move'/'analysis' workers) and the fixed-size Wasm TT
// has almost certainly evicted an old position's entries by the time it's
// revisited. This cache lets the UI skip re-searching from depth 1 for a fen
// it has already fully solved.
export class AnalysisCache {
  private entries = new Map<string, Analysis>();

  private key(fen: string, priorFens: string[]) {
    return `${fen}\u0000${repetitionFingerprint(fen, priorFens)}`;
  }

  get(fen: string, priorFens: string[], minLines: number): Analysis | null {
    const key = this.key(fen, priorFens);
    const entry = this.entries.get(key);
    if (!entry || entry.lines.length < minLines) return null;
    this.entries.delete(key);
    this.entries.set(key, entry);
    return entry;
  }

  peek(fen: string, priorFens: string[], minLines: number): Analysis | null {
    const entry = this.entries.get(this.key(fen, priorFens));
    return entry && entry.lines.length >= minLines ? entry : null;
  }

  set(analysis: Analysis): void {
    const key = `${analysis.root_fen}\u0000${analysis.repetition_fingerprint}`;
    const existing = this.entries.get(key);
    if (
      existing &&
      existing.depth >= analysis.depth &&
      existing.lines.length >= analysis.lines.length
    )
      return;
    this.entries.delete(key);
    this.entries.set(key, analysis);
    if (this.entries.size > MAX_ENTRIES) {
      const oldest = this.entries.keys().next().value;
      if (oldest !== undefined) this.entries.delete(oldest);
    }
  }
}
