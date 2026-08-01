/**
 * Policy for deciding when the worker should start another iterative-deepening
 * pass. Keeping this independent of the Worker/Wasm boundary makes the time
 * budget contract explicit and straightforward to test.
 */
export const MAX_SEARCH_DEPTH = 32;
const MIN_ITERATION_BUDGET_MS = 8;
const ANALYSIS_PREDICTION_SAFETY = 1.5;

/** Gives Rust a small positive budget so it can return a legal fallback. */
export function iterationBudget(remainingMs: number) {
  return Math.max(MIN_ITERATION_BUDGET_MS, remainingMs);
}

interface NextIterationInput {
  elapsedMs: number;
  thinkTimeMs: number;
  softTimeFraction: number;
  predictedNextMs: number;
  ebfGateOverride: boolean;
  multiPv: number;
}

/**
 * Stops at the soft deadline, or — for multi-PV analysis only — before an
 * estimated next iteration would overrun the hard deadline.
 *
 * A move search (`multiPv === 1`) never applies the prediction gate: if a
 * deeper iteration times out mid-search, `SearchCore::analyze_depth` still
 * returns a sound partial line for every root move that finished (see its
 * `partial` field), so there is nothing lost by starting an iteration that
 * cannot complete — only the soft deadline above needs to hold. Multi-PV
 * analysis has no such partial fallback (a truncated line count would make
 * the analysis panel flicker), so it keeps predicting ahead to avoid
 * starting work it cannot finish.
 */
export function shouldStopBeforeNextIteration({
  elapsedMs,
  thinkTimeMs,
  softTimeFraction,
  predictedNextMs,
  ebfGateOverride,
  multiPv,
}: NextIterationInput) {
  if (elapsedMs >= thinkTimeMs * softTimeFraction) return true;
  if (multiPv === 1) return false;
  if (ebfGateOverride) return false;
  return predictedNextMs > (thinkTimeMs - elapsedMs) * ANALYSIS_PREDICTION_SAFETY;
}
