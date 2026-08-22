// Mojo vs. Stockfish 18 Lite (single-threaded Wasm, the same build the web
// app bundles) at equal per-move time, across move budgets and calibrated
// Stockfish UCI_Elo tiers. The repository's committed stockfish-elo.csv
// predates this script and never recorded what move budget produced it, so
// rather than relabel a number of unknown provenance, this replaces it with
// a fully labelled measurement: every row states its own move-time budget.
//
// Two sweeps:
//   A. Fixed opponent tier, varying Mojo's move budget (the paper's central
//      claim: Mojo's playing strength is a function of its per-move time,
//      and a browser engine's operating point is a couple hundred to a few
//      thousand ms, not a fixed Elo).
//   B. Fixed (generous) Mojo budget, varying the opponent's calibrated tier
//      up to Stockfish's UCI_Elo ceiling, to test whether more thinking time
//      lets Mojo compete with a near-maximum calibrated tier.
import { writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import {
  repositoryRoot,
  loadEngineClass,
  loadChess,
  loadStockfish,
  stockfishPrepare,
  stockfishMove,
  loadOpenings,
  selectOpenings,
  performanceElo,
  deltaElo,
  writeCsv,
} from './lib.mjs'

// Small sample sizes by necessity: this sweeps four move budgets up to 3000
// ms/move against a real Stockfish process, single-threaded, in-process —
// there is no SPRT-style early stopping here, just a fixed, honestly-labelled
// game count per cell. Treat these as directional, not statistically tight.
const OPENING_PAIRS = 3
const OPENING_PAIRS_SLOW = 2 // used only for 3000ms-budget cells
const MAX_PLIES = 60
const WIN_SCORE_CP = 1200
const WIN_PLIES = 6
const DRAW_SCORE_CP = 20
const DRAW_PLIES = 10
const DRAW_MIN_PLY = 40

const SWEEP_A_BUDGETS_MS = [100, 500, 1000, 3000]
const SWEEP_A_SF_ELO = 2400
const SWEEP_B_BUDGET_MS = 3000
const SWEEP_B_SF_ELO = [2400, 2800, 3190]

const Chess = loadChess()
const openingSuite = loadOpenings('engine/openings-balanced.json')
const openings = selectOpenings(openingSuite, OPENING_PAIRS)
const openingsSlow = selectOpenings(openingSuite, OPENING_PAIRS_SLOW)

function mojoSearch(engine, fen, priorFens, budgetMs) {
  engine.set_position(fen, priorFens)
  let completed
  const started = performance.now()
  for (let depth = 1; depth <= 32; depth += 1) {
    const remaining = budgetMs - (performance.now() - started)
    if (remaining <= 0 && completed) break
    const result = engine.analyze_depth(depth, 1, Math.max(5, remaining))
    if (!result.timed_out && result.lines.length > 0) completed = result
    if (result.timed_out) break
    if (completed && performance.now() - started >= budgetMs * (result.soft_time_fraction ?? 0.5)) break
    if (!(result.ebf_gate_override ?? false)) {
      const nextRemaining = budgetMs - (performance.now() - started)
      if ((result.predicted_next_ms ?? 0) > nextRemaining * 1.25) break
    }
  }
  const move = completed?.lines[0]?.moves[0] ?? engine.fallback_move()
  if (!move) throw new Error(`Mojo found no move in a non-terminal position: ${fen}`)
  return { line: completed?.lines[0], move }
}

function uciToMove(uci) {
  return { from: uci.slice(0, 2), to: uci.slice(2, 4), promotion: uci[4] }
}

function whiteScoreCp(line, turn) {
  if (!line) return undefined
  const score = line.mate_in !== undefined ? Math.sign(line.mate_in) * 30_000 : line.score_cp
  if (score === undefined) return undefined
  return turn === 'w' ? score : -score
}

function terminalWinner(game) {
  if (!game.isCheckmate()) return null
  return game.turn() === 'w' ? 'black' : 'white'
}

async function playGame({ MojoEngine, stockfish, mojoColor, budgetMs, sfElo, opening }) {
  const mojo = new MojoEngine()
  const game = new Chess(opening.fen)
  const priorFens = []
  const moveHistory = []
  let winner = null
  let reason = 'maximum plies'
  let whiteWinStreak = 0
  let blackWinStreak = 0
  let drawStreak = 0
  let playedPlies = 0

  try {
    while (playedPlies < MAX_PLIES && !game.isGameOver()) {
      const fen = game.fen()
      const turn = game.turn()
      const isMojoTurn = (turn === 'w' && mojoColor === 'white') || (turn === 'b' && mojoColor === 'black')
      let move
      let evaluation
      if (isMojoTurn) {
        const { line, move: mojoMove } = mojoSearch(mojo, fen, priorFens, budgetMs)
        move = mojoMove
        evaluation = whiteScoreCp(line, turn)
      } else {
        move = await stockfishMove(stockfish, { fen: opening.fen, moves: moveHistory, moveTimeMs: budgetMs })
        if (!move) throw new Error(`Stockfish returned no move for ${fen}`)
      }

      if (evaluation !== undefined && evaluation >= WIN_SCORE_CP) {
        whiteWinStreak += 1
        blackWinStreak = 0
      } else if (evaluation !== undefined && evaluation <= -WIN_SCORE_CP) {
        blackWinStreak += 1
        whiteWinStreak = 0
      } else if (evaluation !== undefined) {
        whiteWinStreak = 0
        blackWinStreak = 0
      }
      drawStreak = evaluation !== undefined && Math.abs(evaluation) <= DRAW_SCORE_CP ? drawStreak + 1 : 0

      priorFens.push(fen)
      moveHistory.push(move)
      const applied = game.move(uciToMove(move))
      if (!applied) throw new Error(`Illegal move ${move} for ${fen}`)
      playedPlies += 1

      if (whiteWinStreak >= WIN_PLIES) {
        winner = 'white'
        reason = 'score adjudication'
        break
      }
      if (blackWinStreak >= WIN_PLIES) {
        winner = 'black'
        reason = 'score adjudication'
        break
      }
      if (playedPlies >= DRAW_MIN_PLY && drawStreak >= DRAW_PLIES) {
        reason = 'draw adjudication'
        break
      }
    }
    if (game.isGameOver()) {
      winner = terminalWinner(game)
      reason = game.isCheckmate() ? 'checkmate' : 'rules draw'
    }
  } finally {
    mojo.free()
  }

  const mojoScore = winner === null ? 0.5 : winner === mojoColor ? 1 : 0
  return { opening: opening.name, mojoColor, plies: playedPlies, reason, mojoScore }
}

async function runCell({ MojoEngine, stockfish, budgetMs, sfElo }) {
  await stockfishPrepare(stockfish, { elo: sfElo })
  const cellOpenings = budgetMs >= 3000 ? openingsSlow : openings
  const games = []
  for (const opening of cellOpenings) {
    games.push(await playGame({ MojoEngine, stockfish, mojoColor: 'white', budgetMs, sfElo, opening }))
    games.push(await playGame({ MojoEngine, stockfish, mojoColor: 'black', budgetMs, sfElo, opening }))
  }
  const wins = games.filter((g) => g.mojoScore === 1).length
  const draws = games.filter((g) => g.mojoScore === 0.5).length
  const losses = games.length - wins - draws
  const scorePct = (100 * (wins + draws / 2)) / games.length
  return {
    budgetMs,
    sfElo,
    games: games.length,
    record: `${wins}-${draws}-${losses}`,
    scorePct,
    deltaElo: deltaElo(scorePct),
    perfElo: performanceElo(sfElo, scorePct),
  }
}

async function main() {
  const MojoEngine = await loadEngineClass('engine/pkg/mojo_engine_bg.wasm', 'elo-vs-budget')
  const stockfish = await loadStockfish()

  const results = []
  for (const budgetMs of SWEEP_A_BUDGETS_MS) {
    const cell = await runCell({ MojoEngine, stockfish, budgetMs, sfElo: SWEEP_A_SF_ELO })
    results.push({ sweep: 'A', ...cell })
    console.log('sweep A', cell)
  }
  for (const sfElo of SWEEP_B_SF_ELO) {
    const cell = await runCell({ MojoEngine, stockfish, budgetMs: SWEEP_B_BUDGET_MS, sfElo })
    results.push({ sweep: 'B', ...cell })
    console.log('sweep B', cell)
  }

  stockfish.sendCommand('quit')

  const csv = writeCsv(
    ['sweep', 'budget_ms', 'sf_elo', 'games', 'mojo_record', 'score_pct', 'delta_elo', 'perf_elo'],
    results.map((r) => [
      r.sweep,
      r.budgetMs,
      r.sfElo,
      r.games,
      r.record,
      r.scorePct.toFixed(1),
      r.deltaElo >= 0 ? `+${r.deltaElo.toFixed(0)}` : r.deltaElo.toFixed(0),
      r.perfElo.toFixed(0),
    ]),
  )
  writeFileSync(resolve(repositoryRoot, 'paper/data/elo-vs-budget.csv'), csv)
  console.log('Wrote paper/data/elo-vs-budget.csv')
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack : error)
  process.exitCode = 1
})
