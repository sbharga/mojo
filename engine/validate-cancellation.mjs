import { readFileSync } from 'node:fs'
import { Engine, initSync } from './pkg/mojo_engine.js'

const wasmBytes = readFileSync(new URL('./pkg/mojo_engine_bg.wasm', import.meta.url))
initSync({ module: wasmBytes })

const engine = new Engine()
const stopFlag = new Int32Array(new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT))
engine.set_stop_flag(stopFlag)
engine.set_position('rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1', [])

engine.set_stop_request(8)
Atomics.store(stopFlag, 0, 7)
const allowed = engine.analyze_depth(1, 1, 10_000)
if (allowed.timed_out || allowed.lines.length === 0) {
  throw new Error('A cancellation watermark for an older request stopped the current search')
}

Atomics.store(stopFlag, 0, 8)
const cancelled = engine.analyze_depth(32, 1, 10_000)
if (!cancelled.timed_out || cancelled.nodes > cancelled.clock_check_interval * 2) {
  throw new Error(
    `Shared cancellation was not observed promptly: ${cancelled.nodes} nodes, interval ${cancelled.clock_check_interval}`,
  )
}

console.log({
  stale_watermark_ignored: true,
  cancellation_observed: true,
  nodes: cancelled.nodes,
  clock_check_interval: cancelled.clock_check_interval,
})
