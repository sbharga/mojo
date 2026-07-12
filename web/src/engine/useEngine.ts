import { useCallback, useEffect, useRef, useState } from "react";
import { AnalysisCache } from "./analysisCache";
import type { Analysis, WorkerMessage } from "./types";

// The 'move' and 'analysis' purposes each get their own Worker (and Wasm
// Engine instance) rather than sharing one. A single analyze_depth call is
// synchronous and uninterruptible, so a background 'analysis' search run
// while it's the human's turn can consume its entire time budget before a
// queued cancellation is even processed. On a shared worker that stale
// search stacks its own full budget in front of the engine's subsequent
// 'move' search, breaking the "Engine time is a maximum per move" contract.
// Separate Worker threads make that impossible: they run concurrently, so
// starting a 'move' request is never delayed by whatever the analysis
// worker happens to be doing.
const PURPOSES = ["move", "analysis"] as const;
type Purpose = (typeof PURPOSES)[number];

export function useEngine(onMove: (uci: string) => void) {
  const workers = useRef<Record<Purpose, Worker | null>>({
    move: null,
    analysis: null,
  });
  const request = useRef(0);
  const cache = useRef(new AnalysisCache());
  const [analysis, setAnalysis] = useState<Analysis | null>(null);
  const [readyWorkers, setReadyWorkers] = useState<Set<Purpose>>(new Set());
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const instances = PURPOSES.map((purpose) => {
      const instance = new Worker(new URL("./worker.ts", import.meta.url), {
        type: "module",
      });
      workers.current[purpose] = instance;
      instance.onmessage = (event: MessageEvent<WorkerMessage>) => {
        const message = event.data;
        if (message.type === "ready") {
          setReadyWorkers((current) => new Set(current).add(purpose));
          setError(null);
        }
        // Request zero is initialization. Errors from superseded searches
        // must not replace the status of the current position.
        if (
          message.type === "error" &&
          (message.requestId === 0 || message.requestId === request.current)
        )
          setError(message.message);
        // Cache every valid result we see, even from a superseded request —
        // it's still a correct, reusable result for its own (older) fen.
        if (message.type === "analysis")
          cache.current.set(message.analysis.root_fen, message.analysis);
        if (message.type === "complete" && message.analysis)
          cache.current.set(message.analysis.root_fen, message.analysis);
        if (
          message.type === "analysis" &&
          message.requestId === request.current
        )
          setAnalysis(message.analysis);
        if (
          message.type === "complete" &&
          message.requestId === request.current &&
          message.analysis
        ) {
          setAnalysis(message.analysis);
          if (message.purpose === "move" && message.analysis.lines[0]?.moves[0])
            onMove(message.analysis.lines[0].moves[0]);
        }
      };
      instance.postMessage({ type: "initialize" });
      return instance;
    });
    return () => instances.forEach((instance) => instance.terminate());
  }, [onMove]);

  const start = useCallback(
    (
      fen: string,
      historyFens: string[],
      thinkTimeMs: number,
      purpose: Purpose,
    ) => {
      request.current += 1;
      setError(null);
      const requestId = request.current;
      // Cancel on both workers: the previous request may have used either
      // purpose, and either worker may still hold an older stale request.
      for (const p of PURPOSES)
        workers.current[p]?.postMessage({
          type: "cancel",
          requestId: requestId - 1,
        });
      // A fen already fully analyzed (history navigation, or pause/resume
      // landing back on the same position) doesn't need to be re-searched
      // from depth 1 by the other worker.
      const cached = cache.current.get(fen, purpose === "move" ? 1 : 3);
      if (cached) {
        setAnalysis(cached);
        if (purpose === "move" && cached.lines[0]?.moves[0])
          onMove(cached.lines[0].moves[0]);
        return;
      }
      setAnalysis(null);
      workers.current[purpose]?.postMessage({
        type: "analyze",
        requestId,
        fen,
        historyFens,
        thinkTimeMs,
        purpose,
      });
    },
    [onMove],
  );

  const cancel = useCallback(() => {
    request.current += 1;
    setAnalysis(null);
    for (const p of PURPOSES)
      workers.current[p]?.postMessage({
        type: "cancel",
        requestId: request.current,
      });
  }, []);

  return {
    analysis,
    isReady: readyWorkers.size === PURPOSES.length,
    error,
    start,
    cancel,
  };
}
