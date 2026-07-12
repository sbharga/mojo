import { useCallback, useEffect, useRef, useState } from "react";
import type { Analysis, WorkerMessage } from "./types";

export function useEngine(onMove: (uci: string) => void) {
  const worker = useRef<Worker | null>(null);
  const request = useRef(0);
  const [analysis, setAnalysis] = useState<Analysis | null>(null);
  const [isReady, setIsReady] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const instance = new Worker(new URL("./worker.ts", import.meta.url), {
      type: "module",
    });
    worker.current = instance;
    instance.onmessage = (event: MessageEvent<WorkerMessage>) => {
      const message = event.data;
      if (message.type === "ready") {
        setIsReady(true);
        setError(null);
      }
      // Request zero is initialization. Errors from superseded searches must
      // not replace the status of the current position.
      if (
        message.type === "error" &&
        (message.requestId === 0 || message.requestId === request.current)
      )
        setError(message.message);
      if (message.type === "analysis" && message.requestId === request.current)
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
    return () => instance.terminate();
  }, [onMove]);

  const start = useCallback(
    (
      fen: string,
      historyFens: string[],
      thinkTimeMs: number,
      purpose: "analysis" | "move",
    ) => {
      request.current += 1;
      setAnalysis(null);
      setError(null);
      const requestId = request.current;
      worker.current?.postMessage({ type: "cancel", requestId: requestId - 1 });
      worker.current?.postMessage({
        type: "analyze",
        requestId,
        fen,
        historyFens,
        thinkTimeMs,
        purpose,
      });
    },
    [],
  );

  const cancel = useCallback(() => {
    request.current += 1;
    setAnalysis(null);
    worker.current?.postMessage({ type: "cancel", requestId: request.current });
  }, []);

  return { analysis, isReady, error, start, cancel };
}
