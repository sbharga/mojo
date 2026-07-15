import { readFileSync } from 'node:fs'
import { Engine, initSync } from './pkg/mojo_engine.js'

const wasmBytes = readFileSync(new URL('./pkg/mojo_engine_bg.wasm', import.meta.url))
initSync({ module: wasmBytes })

const fen = 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1'
const depth = 7

function search(engine) {
  engine.set_position(fen, [])
  return engine.analyze_depth(depth, 1, 10_000)
}

const source = new Engine()
const sourceResult = search(source)
const sourceLine = sourceResult.lines[0]
if (!sourceLine || sourceResult.timed_out) throw new Error('Source PV search failed')

const cold = new Engine()
const coldResult = search(cold)

const warm = new Engine()
warm.set_position(fen, [])
const seededEntries = warm.seed_pv(
  sourceLine.moves,
  depth,
  sourceLine.score_cp ?? undefined,
  sourceLine.mate_in ?? undefined,
)
const warmResult = warm.analyze_depth(depth, 1, 10_000)

if (seededEntries === 0) throw new Error('No PV entries were seeded')
if (warmResult.lines[0]?.moves[0] !== sourceLine.moves[0]) {
  throw new Error('Warm search did not preserve the seeded principal move')
}
if (warmResult.nodes >= coldResult.nodes) {
  throw new Error(`Warm search used ${warmResult.nodes} nodes versus ${coldResult.nodes} cold`)
}

console.log({
  seeded_entries: seededEntries,
  cold_nodes: coldResult.nodes,
  warm_nodes: warmResult.nodes,
  best_move: warmResult.lines[0]?.moves[0],
})
