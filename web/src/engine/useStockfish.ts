import { useCallback, useEffect, useRef, useState } from "react";
import {
  StockfishClient,
  type StockfishSearchRequest,
} from "./stockfishClient";

export function useStockfish(
  enabled: boolean,
  onMove: (uci: string) => void,
) {
  const client = useRef<StockfishClient | null>(null);
  const onMoveRef = useRef(onMove);
  onMoveRef.current = onMove;
  const [isReady, setIsReady] = useState(false);
  const [isThinking, setIsThinking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!enabled) {
      setIsReady(false);
      setIsThinking(false);
      setError(null);
      return;
    }
    const path = `${import.meta.env.BASE_URL}stockfish/stockfish-18-lite-single.js`;
    const worker = new Worker(new URL(path, window.location.origin));
    const instance = new StockfishClient(worker, {
      onReady: () => setIsReady(true),
      onThinking: setIsThinking,
      onMove: (move) => onMoveRef.current(move),
      onError: setError,
    });
    client.current = instance;
    return () => {
      client.current = null;
      instance.destroy();
    };
  }, [enabled]);

  const start = useCallback((request: StockfishSearchRequest) => {
    setError(null);
    client.current?.start(request);
  }, []);

  const cancel = useCallback(() => client.current?.cancel(), []);

  return { isReady, isThinking, error, start, cancel };
}
