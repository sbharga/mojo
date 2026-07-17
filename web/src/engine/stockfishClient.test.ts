import { describe, expect, it, vi } from "vitest";
import {
  StockfishClient,
  type UciWorker,
} from "./stockfishClient";

class FakeWorker implements UciWorker {
  onmessage: ((event: MessageEvent<string>) => void) | null = null;
  onerror: ((event: ErrorEvent) => void) | null = null;
  messages: string[] = [];
  terminated = false;

  postMessage(message: string) {
    this.messages.push(message);
  }

  terminate() {
    this.terminated = true;
  }

  emit(line: string) {
    this.onmessage?.({ data: line } as MessageEvent<string>);
  }
}

function setup() {
  const worker = new FakeWorker();
  const callbacks = {
    onReady: vi.fn(),
    onThinking: vi.fn(),
    onMove: vi.fn(),
    onError: vi.fn(),
  };
  const client = new StockfishClient(worker, callbacks);
  return { worker, callbacks, client };
}

function initialize(worker: FakeWorker) {
  worker.emit("uciok");
  worker.emit("readyok");
}

describe("StockfishClient", () => {
  it("initializes UCI and starts a configured search with full history", () => {
    const { worker, callbacks, client } = setup();
    expect(worker.messages).toEqual(["uci"]);

    client.start({
      rootFen: "root fen",
      moves: ["e2e4", "e7e5"],
      elo: 2000,
      thinkTimeMs: 500,
    });
    initialize(worker);
    worker.emit("readyok");

    expect(callbacks.onReady).toHaveBeenCalledOnce();
    expect(worker.messages).toContain(
      "setoption name UCI_LimitStrength value true",
    );
    expect(worker.messages).toContain("setoption name UCI_Elo value 2000");
    expect(worker.messages).toContain(
      "position fen root fen moves e2e4 e7e5",
    );
    expect(worker.messages).toContain("go movetime 500");

    worker.emit("bestmove g1f3 ponder b8c6");
    expect(callbacks.onMove).toHaveBeenCalledWith("g1f3");
  });

  it("stops an old search and ignores its untagged bestmove", () => {
    const { worker, callbacks, client } = setup();
    initialize(worker);
    client.start({ rootFen: "first", moves: [], elo: 1500, thinkTimeMs: 100 });
    worker.emit("readyok");
    client.start({ rootFen: "second", moves: ["d2d4"], elo: 2100, thinkTimeMs: 700 });

    expect(worker.messages.at(-1)).toBe("stop");
    worker.emit("bestmove e2e4");
    expect(callbacks.onMove).not.toHaveBeenCalled();
    worker.emit("readyok");
    worker.emit("bestmove d7d5");
    expect(callbacks.onMove).toHaveBeenCalledWith("d7d5");
  });

  it("reports a missing move and terminates cleanly", () => {
    const { worker, callbacks, client } = setup();
    initialize(worker);
    client.start({ rootFen: "position", moves: [], elo: 2000, thinkTimeMs: 500 });
    worker.emit("readyok");
    worker.emit("bestmove (none)");
    expect(callbacks.onError).toHaveBeenCalledWith(
      "Stockfish did not return a legal move",
    );

    client.destroy();
    expect(worker.messages.at(-1)).toBe("quit");
    expect(worker.terminated).toBe(true);
  });

  it("continues with a newer queued position after a search error", () => {
    const { worker, callbacks, client } = setup();
    initialize(worker);
    client.start({ rootFen: "first", moves: [], elo: 1500, thinkTimeMs: 100 });
    worker.emit("readyok");
    client.start({ rootFen: "second", moves: ["d2d4"], elo: 2100, thinkTimeMs: 700 });

    worker.emit("info string CRITICAL ERROR search aborted");
    expect(callbacks.onError).toHaveBeenCalledWith("CRITICAL ERROR search aborted");
    expect(worker.messages).toContain("setoption name UCI_Elo value 2100");

    worker.emit("readyok");
    expect(worker.messages).toContain("position fen second moves d2d4");
    expect(worker.messages).toContain("go movetime 700");
  });
});
