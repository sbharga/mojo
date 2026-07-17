/**
 * Policy for deciding when the worker should start another iterative-deepening
 * pass. Keeping this independent of the Worker/Wasm boundary makes the time
 * budget contract explicit and straightforward to test.
 */
export const MAX_SEARCH_DEPTH = 32;
const MIN_ITERATION_BUDGET_MS = 8;
const MOVE_PREDICTION_SAFETY = 1.25;
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
 * Stops at the soft deadline, or before an estimated next iteration would
 * overrun the hard deadline. Multi-PV work receives extra prediction slack:
 * its next-depth cost is noisier because each root line is searched.
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
  if (ebfGateOverride) return false;
  const safety = multiPv > 1
    ? ANALYSIS_PREDICTION_SAFETY
    : MOVE_PREDICTION_SAFETY;
  return predictedNextMs > (thinkTimeMs - elapsedMs) * safety;
}
