import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { brotliCompressSync, constants, gzipSync } from 'node:zlib'
import { Engine, initSync } from './pkg/mojo_engine.js'

let fixedDepth
let wasmPath
for (let index = 2; index < process.argv.length; index += 1) {
  const argument = process.argv[index]
  const value = process.argv[index + 1]
  if (argument === '--fixed-depth') {
    fixedDepth = Number(value)
    if (!Number.isInteger(fixedDepth) || fixedDepth < 1) {
      throw new Error('--fixed-depth requires a positive integer')
    }
    index += 1
  } else if (argument === '--wasm') {
    if (!value) throw new Error('--wasm requires an artifact path')
    wasmPath = resolve(process.cwd(), value)
    index += 1
  } else {
    throw new Error(`unknown argument: ${argument}`)
  }
}

const defaultWasmUrl = new URL('./pkg/mojo_engine_bg.wasm', import.meta.url)
const wasmBytes = readFileSync(wasmPath ?? defaultWasmUrl)
const simdWasmBytes = wasmPath
  ? undefined
  : readFileSync(new URL('./pkg/mojo_engine_simd_bg.wasm', import.meta.url))
const wasm = initSync({ module: wasmBytes })
const initialMemory = wasm.memory.buffer.byteLength
const positions = [
  ['start', 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1'],
  ['tactical', 'r3k2r/p1ppqpb1/bn2pnp1/2pP4/1p2P3/2N2N2/PPQBBPPP/R3K2R w KQkq - 0 1'],
  ['middlegame', 'r1bq1rk1/pp2bppp/2n1pn2/2pp4/3P4/2P1PN2/PP1NBPPP/R2Q1RK1 w - - 2 9'],
  ['endgame', '8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1'],
]

function runPosition(name, fen, thinkTimeMs, multiPv) {
  // Isolate each sample so an earlier budget or MultiPV run cannot warm the
  // transposition table and make later samples look artificially faster.
  const engine = new Engine()
  try {
    // Match the cross-origin-isolated browser path, including its atomic poll.
    const stopFlag = new Int32Array(new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT))
    engine.set_stop_flag(stopFlag)
    engine.set_stop_request(1)
    engine.set_position(fen, [])
    const started = performance.now()
    let latest
    for (let depth = 1; depth <= 32; depth += 1) {
      const remaining = thinkTimeMs - (performance.now() - started)
      if (remaining <= 0 && latest) break
      const result = engine.analyze_depth(depth, multiPv, Math.max(8, remaining))
      if (!result.timed_out && result.lines.length > 0) latest = result
      if (result.timed_out) break
      if (
        latest
        && performance.now() - started
          >= thinkTimeMs * (result.soft_time_fraction ?? 0.5)
      ) break
      const nextRemaining = thinkTimeMs - (performance.now() - started)
      const predictionSafety = multiPv > 1 ? 1.5 : 1.25
      if (
        !(result.ebf_gate_override ?? false)
        && (result.predicted_next_ms ?? 0) > nextRemaining * predictionSafety
      ) break
    }
    const wallMs = Math.round(performance.now() - started)
    return {
      position: name,
      budget_ms: thinkTimeMs,
      multipv: multiPv,
      wall_ms: wallMs,
      overrun_ms: Math.max(0, wallMs - thinkTimeMs),
      depth: latest?.depth ?? 0,
      nodes: Number(latest?.nodes ?? 0),
      clock_interval: latest?.clock_check_interval ?? null,
      best_move: latest?.lines[0]?.moves[0] ?? null,
      score_cp: latest?.lines[0]?.score_cp ?? null,
    }
  } finally {
    engine.free()
  }
}

function runFixedDepth(name, fen, targetDepth, multiPv) {
  const engine = new Engine()
  try {
    const stopFlag = new Int32Array(new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT))
    engine.set_stop_flag(stopFlag)
    engine.set_stop_request(1)
    engine.set_position(fen, [])
    const started = performance.now()
    let latest
    let cumulativeNodes = 0
    for (let depth = 1; depth <= targetDepth; depth += 1) {
      const result = engine.analyze_depth(depth, multiPv, 60_000)
      if (result.timed_out || result.lines.length === 0) {
        throw new Error(`${name} failed to complete fixed depth ${depth}`)
      }
      cumulativeNodes += Number(result.nodes)
      latest = result
    }
    return {
      position: name,
      target_depth: targetDepth,
      multipv: multiPv,
      wall_ms: Math.round(performance.now() - started),
      cumulative_nodes: cumulativeNodes,
      final_iteration_nodes: Number(latest.nodes),
      best_move: latest.lines[0]?.moves[0] ?? null,
      score_cp: latest.lines[0]?.score_cp ?? null,
    }
  } finally {
    engine.free()
  }
}

const results = []
for (const [name, fen] of positions) {
  if (fixedDepth) {
    results.push(runFixedDepth(name, fen, fixedDepth, 1))
    results.push(runFixedDepth(name, fen, fixedDepth, 3))
  } else {
    for (const budget of [100, 500, 1000]) {
      results.push(runPosition(name, fen, budget, 1))
      results.push(runPosition(name, fen, budget, 3))
    }
  }
}
const peakMemory = wasm.memory.buffer.byteLength
const artifactLabel = wasmPath ? 'tested' : 'baseline'

console.table(results)
console.log({
  [`${artifactLabel}_wasm_raw_bytes`]: wasmBytes.byteLength,
  [`${artifactLabel}_wasm_gzip_bytes`]: gzipSync(wasmBytes).byteLength,
  [`${artifactLabel}_wasm_brotli_bytes`]: brotliCompressSync(wasmBytes, {
    params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
  }).byteLength,
  ...(simdWasmBytes && {
    simd_wasm_raw_bytes: simdWasmBytes.byteLength,
    simd_wasm_gzip_bytes: gzipSync(simdWasmBytes).byteLength,
    simd_wasm_brotli_bytes: brotliCompressSync(simdWasmBytes, {
      params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
    }).byteLength,
  }),
  initial_memory_bytes: initialMemory,
  peak_memory_bytes: peakMemory,
})
