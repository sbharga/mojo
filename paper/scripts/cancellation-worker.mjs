// Runs inside a worker_thread: performs one long search whose stop flag lives
// in a SharedArrayBuffer, so the parent's Atomics.store is genuinely
// concurrent with this thread's Atomics.load polling — the same relationship
// as the browser's main-thread-cancels / worker-thread-searches split. A
// single-threaded harness cannot observe real cancellation latency because
// nothing can interrupt a synchronous Wasm call from the calling thread.
import { parentPort, workerData } from 'node:worker_threads'
import { loadEngineClass } from './lib.mjs'

const { budgetMs, stopBuffer } = workerData
const START_FEN = 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1'

const Engine = await loadEngineClass('engine/pkg/mojo_engine_bg.wasm', `cancel-${budgetMs}-${Math.random()}`)
const engine = new Engine()
const stopFlag = new Int32Array(stopBuffer)
engine.set_stop_flag(stopFlag)
engine.set_stop_request(1)
engine.set_position(START_FEN, [])

// Signal readiness before starting the search so the parent's cancel timer
// only runs against actual search time, not worker/Wasm startup overhead
// (module load, compile, instantiate) which would otherwise dominate the
// measurement at short budgets.
parentPort.postMessage({ type: 'ready' })

const started = performance.now()
const result = engine.analyze_depth(32, 1, budgetMs)
const elapsedMs = performance.now() - started
engine.free()

parentPort.postMessage({ type: 'done', elapsedMs, timedOut: result.timed_out, nodes: result.nodes })
