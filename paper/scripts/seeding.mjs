// Measures the actual ponder-seeding path (web/src/engine/pvSeed.ts), not a
// same-position/same-depth reseed: the analysis worker searches the current
// position to depth D, and if the opponent's move matches the PV's first
// move, the move worker seeds its own search of the resulting position from
// the remainder of that PV at depth D-1. This compares that seeded search
// against a cold search of the same child position at the same depth.
import { writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { repositoryRoot, loadEngineClass, loadChess, writeCsv } from './lib.mjs'

const Engine = await loadEngineClass('engine/pkg/mojo_engine_bg.wasm', 'seeding')
const Chess = loadChess()

const positions = [
  ['start', 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1'],
  ['tactical', 'r3k2r/p1ppqpb1/bn2pnp1/2pP4/1p2P3/2N2N2/PPQBBPPP/R3K2R w KQkq - 0 1'],
  ['middlegame', 'r1bq1rk1/pp2bppp/2n1pn2/2pp4/3P4/2P1PN2/PP1NBPPP/R2Q1RK1 w - - 2 9'],
  ['endgame', '8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1'],
]
const ANALYSIS_DEPTHS = [9, 10, 11]

function search(engine, fen, priorFens, depth) {
  engine.set_position(fen, priorFens)
  return engine.analyze_depth(depth, 1, 30_000)
}

function uciToMove(uci) {
  return { from: uci.slice(0, 2), to: uci.slice(2, 4), promotion: uci[4] }
}

const rows = []
for (const [name, fen] of positions) {
  for (const analysisDepth of ANALYSIS_DEPTHS) {
    const analysisEngine = new Engine()
    const analysisResult = search(analysisEngine, fen, [], analysisDepth)
    analysisEngine.free()
    const line = analysisResult.lines[0]
    if (analysisResult.timed_out || !line || line.moves.length < 2) {
      console.warn(`  skip: ${name} d${analysisDepth} produced no usable PV`)
      continue
    }

    const game = new Chess(fen)
    game.move(uciToMove(line.moves[0]))
    const childFen = game.fen()
    const childPriorFens = [fen]
    const childDepth = analysisDepth - 1
    const seedMoves = line.moves.slice(1)

    const cold = new Engine()
    const coldResult = search(cold, childFen, childPriorFens, childDepth)
    cold.free()

    const warm = new Engine()
    warm.set_position(childFen, childPriorFens)
    const seededEntries = warm.seed_pv(
      seedMoves,
      childDepth,
      line.score_cp === undefined ? undefined : -line.score_cp,
      line.mate_in === undefined ? undefined : -line.mate_in,
    )
    const warmResult = search(warm, childFen, childPriorFens, childDepth)
    warm.free()

    const reduction = coldResult.nodes === 0 ? 0 : 1 - warmResult.nodes / coldResult.nodes
    rows.push([
      name,
      analysisDepth,
      childDepth,
      seededEntries,
      coldResult.nodes,
      warmResult.nodes,
      (reduction * 100).toFixed(2),
    ])
    console.log({
      name,
      analysisDepth,
      childDepth,
      seededEntries,
      cold: coldResult.nodes,
      warm: warmResult.nodes,
      reductionPct: reduction * 100,
    })
  }
}

const csv = writeCsv(
  ['position', 'analysis_depth', 'seeded_search_depth', 'seeded_entries', 'cold_nodes', 'warm_nodes', 'node_reduction_pct'],
  rows,
)
writeFileSync(resolve(repositoryRoot, 'paper/data/seeding.csv'), csv)
console.log('Wrote paper/data/seeding.csv')
