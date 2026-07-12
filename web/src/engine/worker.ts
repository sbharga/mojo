/// <reference lib="webworker" />

import init, { Engine } from "../../../engine/pkg/mojo_engine.js";
import wasmUrl from "../../../engine/pkg/mojo_engine_bg.wasm?url";
import type {
  Analysis,
  AnalyzeRequest,
  WorkerMessage,
  WorkerRequest,
} from "./types";
import { toWhiteRelative } from "./analysis";

let initialized = false;
let cancelledBefore = 0;
let engine: Engine | null = null;
let initialization: Promise<void> | null = null;

async function ensureEngine() {
  if (initialized) return;
  initialization ??= (async () => {
    await init({ module_or_path: wasmUrl });
    engine = new Engine();
    initialized = true;
    postMessage({ type: "ready" } satisfies WorkerMessage);
  })();
  try {
    await initialization;
  } catch (error) {
    // Permit a later request to retry a transient initialization failure.
    initialization = null;
    throw error;
  }
}

async function analyze(request: AnalyzeRequest) {
  try {
    await ensureEngine();
    if (!engine) throw new Error("Engine failed to initialize");
    engine.set_position(request.fen, request.historyFens);
    const started = performance.now();
    let depth = 1;
    let latest: Analysis | null = null;
    const maxDepth = 32;
    const multiPv = request.purpose === "move" ? 1 : 3;
    while (request.requestId > cancelledBefore && depth <= maxDepth) {
      const remaining = request.thinkTimeMs - (performance.now() - started);
      if (remaining <= 0 && latest) break;
      // A full remaining budget lets the selected engine-time preset reach
      // meaningfully deeper searches; the Rust search checks its deadline.
      const budget = Math.max(8, remaining);
      const result = toWhiteRelative(
        engine.analyze_depth(depth, multiPv, budget) as Omit<
          Analysis,
          "root_fen"
        >,
        request.fen,
      );
      const rootedResult: Analysis = { ...result, root_fen: request.fen };
      if (request.requestId <= cancelledBefore) return;
      // A partial deeper iteration is useful search work, but it must never
      // replace the last fully completed and internally consistent result.
      if (!result.timed_out && result.lines.length > 0) {
        latest = rootedResult;
        postMessage({
          type: "analysis",
          requestId: request.requestId,
          analysis: rootedResult,
        } satisfies WorkerMessage);
      }
      if (result.timed_out) break;
      depth += 1;
      // Each analyze_depth call is a single synchronous, uninterruptible Wasm
      // call, so this loop must yield back to the event loop between depths.
      // Without it, a queued "cancel" for this request (e.g. because the
      // human just moved) cannot be handled until this search burns its
      // entire time budget on its own — stacking a stale search's full
      // budget in front of the next request's. A microtask isn't enough:
      // queued worker "message" events are macrotasks, so the yield must be
      // a real macrotask (setTimeout) to let them run first.
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
    }
    if (request.purpose === "move" && latest === null) {
      const move = engine.fallback_move();
      if (move) {
        latest = {
          root_fen: request.fen,
          depth: 0,
          nodes: 0,
          elapsed_ms: performance.now() - started,
          timed_out: true,
          lines: [{ score_cp: 0, moves: [move] }],
        };
      }
    }
    if (request.requestId > cancelledBefore) {
      postMessage({
        type: "complete",
        requestId: request.requestId,
        purpose: request.purpose,
        analysis: latest,
      } satisfies WorkerMessage);
    }
  } catch (error) {
    postMessage({
      type: "error",
      requestId: request.requestId,
      message: error instanceof Error ? error.message : String(error),
    } satisfies WorkerMessage);
  }
}

self.onmessage = (event: MessageEvent<WorkerRequest>) => {
  const request = event.data;
  if (request.type === "initialize") {
    void ensureEngine().catch((error) => {
      postMessage({
        type: "error",
        requestId: 0,
        message: error instanceof Error ? error.message : String(error),
      } satisfies WorkerMessage);
    });
    return;
  }
  if (request.type === "cancel") {
    cancelledBefore = Math.max(cancelledBefore, request.requestId);
    return;
  }
  void analyze(request);
};
