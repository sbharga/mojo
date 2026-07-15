import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { Engine, initSync } from './pkg/mojo_engine.js'

const wasmArgument = process.argv.indexOf('--wasm')
if (wasmArgument !== -1 && !process.argv[wasmArgument + 1]) {
  throw new Error('--wasm requires an artifact path')
}
const wasmSource = wasmArgument === -1
  ? new URL('./pkg/mojo_engine_bg.wasm', import.meta.url)
  : resolve(process.cwd(), process.argv[wasmArgument + 1])
const wasmBytes = readFileSync(wasmSource)
initSync({ module: wasmBytes })

// Small, deterministic positions are intentional here. This is a regression
// gate for search correctness, not an Elo estimate: every expected move can be
// inspected by hand, and a fixed depth keeps results reproducible in CI.
const cases = [
  {
    name: 'white back-rank mate',
    fen: '6k1/5ppp/8/8/8/8/5PPP/3R2K1 w - - 0 1',
    depth: 2,
    moves: ['d1d8'],
    mateIn: 1,
  },
  {
    name: 'black back-rank mate',
    fen: '3r2k1/5ppp/8/8/8/8/5PPP/6K1 b - - 0 1',
    depth: 2,
    moves: ['d8d1'],
    mateIn: 1,
  },
  {
    name: 'supported queen mate',
    fen: '7k/5Q2/6K1/8/8/8/8/8 w - - 0 1',
    depth: 2,
    moves: ['f7f8', 'f7g7'],
    fallbackMoves: ['f7f8', 'f7g7', 'f7h7'],
    mateIn: 1,
  },
  {
    name: 'promote with tempo',
    fen: '7k/5P2/5K2/8/8/8/8/8 w - - 0 1',
    depth: 4,
    moves: ['f7f8q'],
    mateIn: 2,
  },
  {
    name: 'capture hanging queen',
    fen: '7k/3q4/8/8/8/8/3R4/7K w - - 0 1',
    depth: 4,
    moves: ['d2d7'],
    minScoreCp: 600,
  },
  {
    name: 'knight fork saves the game',
    fen: 'q3k3/8/8/1N6/8/8/8/4K3 w - - 0 1',
    depth: 5,
    moves: ['b5c7'],
    minScoreCp: 0,
  },
  {
    name: 'capture promotion',
    fen: 'k6r/6P1/2K5/8/8/8/8/8 w - - 0 1',
    depth: 5,
    moves: ['g7h8q'],
    mateIn: 3,
  },
  {
    name: 'capture promotion through delta window',
    fen: 'k5nr/6P1/2K5/8/8/8/8/8 w - - 0 1',
    depth: 1,
    moves: ['g7h8q'],
    minScoreCp: 700,
  },
]

function run(testCase) {
  const engine = new Engine()
  try {
    engine.set_position(testCase.fen, [])
    const result = engine.analyze_depth(testCase.depth, 1, 10_000)
    const line = result.lines[0]
    const bestMove = line?.moves[0] ?? null
    const fallbackMove = testCase.fallbackMoves ? engine.fallback_move() : null
    const failures = []

    if (result.timed_out) failures.push('timed out')
    if (!testCase.moves.includes(bestMove)) {
      failures.push(`expected ${testCase.moves.join('/')} but found ${bestMove ?? 'no move'}`)
    }
    if (testCase.fallbackMoves && !testCase.fallbackMoves.includes(fallbackMove)) {
      failures.push(
        `expected fallback ${testCase.fallbackMoves.join('/')} but found ${fallbackMove ?? 'no move'}`,
      )
    }
    if (testCase.mateIn !== undefined && line?.mate_in !== testCase.mateIn) {
      failures.push(`expected mate in ${testCase.mateIn} but found ${line?.mate_in ?? 'no mate'}`)
    }
    if (
      testCase.minScoreCp !== undefined
      && (line?.score_cp === undefined || line.score_cp < testCase.minScoreCp)
    ) {
      failures.push(
        `expected at least ${testCase.minScoreCp} cp but found ${line?.score_cp ?? 'no score'}`,
      )
    }

    return {
      position: testCase.name,
      depth: testCase.depth,
      best_move: bestMove,
      fallback_move: fallbackMove,
      score: line?.mate_in === undefined ? line?.score_cp : `M${line.mate_in}`,
      nodes: Number(result.nodes),
      status: failures.length === 0 ? 'pass' : failures.join('; '),
      passed: failures.length === 0,
    }
  } finally {
    engine.free()
  }
}

const results = cases.map(run)
console.table(results.map(({ passed: _, ...result }) => result))

const failed = results.filter((result) => !result.passed)
if (failed.length > 0) {
  console.error(`${failed.length}/${results.length} tactical regression cases failed`)
  process.exitCode = 1
} else {
  console.log(`${results.length}/${results.length} tactical regression cases passed`)
}
