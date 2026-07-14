export const MIN_STOCKFISH_ELO = 1320;
export const MAX_STOCKFISH_ELO = 3190;

export interface StockfishSearchRequest {
  rootFen: string;
  moves: string[];
  elo: number;
  thinkTimeMs: number;
}

export interface UciWorker {
  onmessage: ((event: MessageEvent<string>) => void) | null;
  onerror: ((event: ErrorEvent) => void) | null;
  postMessage(message: string): void;
  terminate(): void;
}

interface Search {
  id: number;
  request: StockfishSearchRequest;
}

interface Callbacks {
  onReady: () => void;
  onThinking: (thinking: boolean) => void;
  onMove: (move: string) => void;
  onError: (message: string) => void;
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value));
}

/** Serializes UCI searches and rejects untagged results from stale positions. */
export class StockfishClient {
  private initialized = false;
  private latestId = 0;
  private pending: Search | null = null;
  private preparing: Search | null = null;
  private current: Search | null = null;

  constructor(
    private readonly worker: UciWorker,
    private readonly callbacks: Callbacks,
  ) {
    worker.onmessage = (event) => this.handleLine(String(event.data));
    worker.onerror = (event) => {
      this.callbacks.onThinking(false);
      this.callbacks.onError(event.message || "Stockfish failed to load");
    };
    worker.postMessage("uci");
  }

  start(request: StockfishSearchRequest) {
    this.latestId += 1;
    this.pending = { id: this.latestId, request };
    if (this.current) this.worker.postMessage("stop");
    else this.schedule();
  }

  cancel() {
    this.latestId += 1;
    this.pending = null;
    this.callbacks.onThinking(false);
    if (this.current) this.worker.postMessage("stop");
  }

  destroy() {
    this.cancel();
    this.worker.postMessage("quit");
    this.worker.terminate();
  }

  private schedule() {
    if (
      !this.initialized ||
      this.current ||
      this.preparing ||
      !this.pending
    )
      return;
    this.preparing = this.pending;
    this.pending = null;
    const elo = Math.round(
      clamp(
        this.preparing.request.elo,
        MIN_STOCKFISH_ELO,
        MAX_STOCKFISH_ELO,
      ),
    );
    this.worker.postMessage(`setoption name UCI_Elo value ${elo}`);
    this.worker.postMessage("isready");
  }

  private handleLine(line: string) {
    if (line === "uciok") {
      this.worker.postMessage("setoption name UCI_LimitStrength value true");
      this.worker.postMessage("isready");
      return;
    }
    if (line === "readyok") {
      if (!this.initialized) {
        this.initialized = true;
        this.callbacks.onReady();
      }
      if (!this.preparing) {
        this.schedule();
        return;
      }
      const search = this.preparing;
      this.preparing = null;
      if (search.id !== this.latestId) {
        this.schedule();
        return;
      }
      this.current = search;
      const moves = search.request.moves.length
        ? ` moves ${search.request.moves.join(" ")}`
        : "";
      const thinkTime = Math.max(1, Math.round(search.request.thinkTimeMs));
      this.worker.postMessage(`position fen ${search.request.rootFen}${moves}`);
      this.worker.postMessage(`go movetime ${thinkTime}`);
      this.callbacks.onThinking(true);
      return;
    }
    if (line.startsWith("info string CRITICAL ERROR")) {
      this.callbacks.onThinking(false);
      this.callbacks.onError(line.replace(/^info string /, ""));
      return;
    }
    if (!line.startsWith("bestmove ")) return;

    const search = this.current;
    this.current = null;
    this.callbacks.onThinking(false);
    const move = line.split(/\s+/, 2)[1];
    if (search?.id === this.latestId) {
      if (move && move !== "(none)") this.callbacks.onMove(move);
      else this.callbacks.onError("Stockfish did not return a legal move");
    }
    this.schedule();
  }
}
