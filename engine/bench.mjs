import { readFileSync } from 'node:fs'
import { gzipSync } from 'node:zlib'
import { Engine, initSync } from './pkg/mojo_engine.js'

const wasmBytes = readFileSync(new URL('./pkg/mojo_engine_bg.wasm', import.meta.url))
const wasm = initSync({ module: wasmBytes })
const initialMemory = wasm.memory.buffer.byteLength
const positions = [
  ['start', 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1'],
  ['tactical', 'r3k2r/p1ppqpb1/bn2pnp1/2pP4/1p2P3/2N2N2/PPQBBPPP/R3K2R w KQkq - 0 1'],
  ['middlegame', 'r1bq1rk1/pp2bppp/2n1pn2/2pp4/3P4/2P1PN2/PP1NBPPP/R2Q1RK1 w - - 2 9'],
  ['endgame', '8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1'],
]

function runPosition(engine, name, fen, thinkTimeMs, multiPv) {
  engine.set_position(fen, [])
  const started = performance.now()
  let latest
  for (let depth = 1; depth <= 32; depth += 1) {
    const remaining = thinkTimeMs - (performance.now() - started)
    if (remaining <= 0 && latest) break
    const result = engine.analyze_depth(depth, multiPv, Math.max(8, remaining))
    if (!result.timed_out && result.lines.length > 0) latest = result
    if (result.timed_out) break
  }
  return {
    position: name,
    budget_ms: thinkTimeMs,
    multipv: multiPv,
    wall_ms: Math.round(performance.now() - started),
    depth: latest?.depth ?? 0,
    nodes: Number(latest?.nodes ?? 0),
    best_move: latest?.lines[0]?.moves[0] ?? null,
    score_cp: latest?.lines[0]?.score_cp ?? null,
  }
}

const engine = new Engine()
const results = []
for (const [name, fen] of positions) {
  for (const budget of [100, 500, 1000]) {
    results.push(runPosition(engine, name, fen, budget, 1))
    results.push(runPosition(engine, name, fen, budget, 3))
  }
}
const peakMemory = wasm.memory.buffer.byteLength
engine.free()

console.table(results)
console.log({
  wasm_raw_bytes: wasmBytes.byteLength,
  wasm_gzip_bytes: gzipSync(wasmBytes).byteLength,
  initial_memory_bytes: initialMemory,
  peak_memory_bytes: peakMemory,
})
