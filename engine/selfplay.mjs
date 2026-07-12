import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { fileURLToPath } from 'node:url'
import { resolve } from 'node:path'

const requireFromWeb = createRequire(new URL('../web/package.json', import.meta.url))
const { Chess } = requireFromWeb('chess.js')
const repositoryRoot = fileURLToPath(new URL('..', import.meta.url))

const defaultOpeningsUrl = new URL('./openings.json', import.meta.url)

const defaults = {
  candidate: './engine/pkg/mojo_engine_bg.wasm',
  depth: 5,
  drawRate: 0.5,
  elo0: 0,
  elo1: 10,
  alpha: 0.05,
  beta: 0.05,
  maxPlies: 200,
  moveTimeMs: undefined,
  openingLimit: undefined,
  openingsFile: undefined,
  winScoreCp: 1200,
  winPlies: 6,
  drawScoreCp: 20,
  drawPlies: 12,
  drawMinPly: 80,
}

function usage() {
  console.log(`Usage: npm --prefix web run selfplay -- --baseline <engine.wasm> [options]

Options:
  --candidate <path>       Candidate Wasm (default: current generated engine)
  --depth <plies>          Fixed iterative-search depth (default: 5)
  --move-time-ms <ms>      Equal time per move; overrides fixed depth
  --openings <count>       Limit the number of opening pairs
  --openings-file <path>   JSON opening suite (default: bundled ECO FEN suite)
  --max-plies <count>      Hard game-length draw limit (default: 200)
  --elo0 <elo>             SPRT null hypothesis (default: 0)
  --elo1 <elo>             SPRT alternative hypothesis (default: 10)
  --draw-rate <0..1>       Assumed per-game draw probability (default: 0.5)
  --alpha <0..1>           False-positive rate (default: 0.05)
  --beta <0..1>            False-negative rate (default: 0.05)
  --help                    Show this message

The baseline and candidate must use the same generated JavaScript ABI.`)
}

function parseArgs(argv) {
  const options = { ...defaults }
  const numeric = new Map([
    ['--depth', 'depth'],
    ['--openings', 'openingLimit'],
    ['--max-plies', 'maxPlies'],
    ['--move-time-ms', 'moveTimeMs'],
    ['--elo0', 'elo0'],
    ['--elo1', 'elo1'],
    ['--draw-rate', 'drawRate'],
    ['--alpha', 'alpha'],
    ['--beta', 'beta'],
  ])

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index]
    if (argument === '--help') {
      usage()
      process.exit(0)
    }
    if (['--baseline', '--candidate', '--openings-file'].includes(argument)) {
      const value = argv[index + 1]
      if (!value) throw new Error(`${argument} requires a path`)
      const property = argument === '--openings-file' ? 'openingsFile' : argument.slice(2)
      options[property] = value
      index += 1
      continue
    }
    const property = numeric.get(argument)
    if (property) {
      const value = Number(argv[index + 1])
      if (!Number.isFinite(value)) throw new Error(`${argument} requires a number`)
      options[property] = value
      index += 1
      continue
    }
    throw new Error(`Unknown argument: ${argument}`)
  }

  if (!options.baseline) throw new Error('--baseline is required')
  for (const property of ['depth', 'maxPlies']) {
    if (!Number.isInteger(options[property]) || options[property] < 1) {
      throw new Error(`--${property.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`)} must be a positive integer`)
    }
  }
  if (
    options.openingLimit !== undefined
    && (!Number.isInteger(options.openingLimit) || options.openingLimit < 1)
  ) {
    throw new Error('--openings must be a positive integer')
  }
  if (!(options.elo1 > options.elo0)) throw new Error('--elo1 must be greater than --elo0')
  for (const property of ['drawRate', 'alpha', 'beta']) {
    if (!(options[property] > 0 && options[property] < 1)) {
      throw new Error(`${property} must be between 0 and 1`)
    }
  }
  if (options.moveTimeMs !== undefined && options.moveTimeMs < 5) {
    throw new Error('--move-time-ms must be at least 5')
  }
  return options
}

function loadOpenings(path) {
  const source = path ? resolve(repositoryRoot, path) : defaultOpeningsUrl
  const parsed = JSON.parse(readFileSync(source, 'utf8'))
  const entries = Array.isArray(parsed) ? parsed : parsed.positions
  if (!Array.isArray(entries) || entries.length === 0) {
    throw new Error('opening suite must be a non-empty JSON array or contain a positions array')
  }
  const validated = entries.map((opening, index) => {
    const hasMoves = Array.isArray(opening?.moves)
      && opening.moves.every((move) => typeof move === 'string')
    const hasFen = typeof opening?.fen === 'string'
    if (
      typeof opening?.name !== 'string'
      || (!hasMoves && !hasFen)
      || (hasMoves && hasFen)
    ) {
      throw new Error(
        `invalid opening at index ${index}; expected { name, fen } or { name, moves: [UCI...] }`,
      )
    }
    return opening
  })
  const seen = new Set()
  return validated.filter((opening) => {
    const fen = createOpening(opening).game.fen()
    if (seen.has(fen)) return false
    seen.add(fen)
    return true
  })
}

async function loadEngineClass(wasmPath, instance) {
  const glueUrl = new URL('./pkg/mojo_engine.js', import.meta.url)
  glueUrl.searchParams.set('instance', instance)
  const engineModule = await import(glueUrl.href)
  const wasmBytes = readFileSync(resolve(repositoryRoot, wasmPath))
  engineModule.initSync({ module: wasmBytes })
  return engineModule.Engine
}

function playUci(game, uci) {
  const move = game.move({
    from: uci.slice(0, 2),
    to: uci.slice(2, 4),
    promotion: uci[4],
  })
  if (!move) throw new Error(`Engine returned illegal move ${uci} for ${game.fen()}`)
}

function createOpening(opening) {
  if (opening.fen) return { game: new Chess(opening.fen), priorFens: [] }
  const game = new Chess()
  const priorFens = []
  for (const move of opening.moves) {
    priorFens.push(game.fen())
    playUci(game, move)
  }
  return { game, priorFens }
}

function gamePly(game) {
  const fullmove = Number(game.fen().split(' ')[5])
  return (fullmove - 1) * 2 + (game.turn() === 'b' ? 1 : 0)
}

function search(engine, fen, priorFens, options) {
  engine.set_position(fen, priorFens)
  let completed
  const started = performance.now()
  const maximumDepth = options.moveTimeMs === undefined ? options.depth : 32
  for (let currentDepth = 1; currentDepth <= maximumDepth; currentDepth += 1) {
    const remaining = options.moveTimeMs === undefined
      ? 60_000
      : options.moveTimeMs - (performance.now() - started)
    if (remaining <= 0 && completed) break
    const result = engine.analyze_depth(currentDepth, 1, Math.max(5, remaining))
    if (!result.timed_out && result.lines.length > 0) completed = result
    if (result.timed_out) break
  }
  const line = completed?.lines[0]
  const move = line?.moves[0] ?? engine.fallback_move()
  if (!move) throw new Error(`Engine found no move in a non-terminal position: ${fen}`)
  return { line, move }
}

function whiteScore(line, turn) {
  let score
  if (line?.mate_in !== undefined) {
    score = Math.sign(line.mate_in) * 30_000
  } else if (line?.score_cp !== undefined) {
    score = line.score_cp
  } else {
    return undefined
  }
  return turn === 'w' ? score : -score
}

function terminalWinner(game) {
  if (!game.isCheckmate()) return null
  return game.turn() === 'w' ? 'black' : 'white'
}

function candidateResult(winner, candidateColor) {
  if (winner === null) return 0.5
  return winner === candidateColor ? 1 : 0
}

function playGame({ Baseline, Candidate, candidateColor, maxPlies, opening, options }) {
  const baseline = new Baseline()
  const candidate = new Candidate()
  const { game, priorFens } = createOpening(opening)
  const white = candidateColor === 'white' ? candidate : baseline
  const black = candidateColor === 'black' ? candidate : baseline
  let whiteWinStreak = 0
  let blackWinStreak = 0
  let drawStreak = 0
  let reason = 'maximum plies'
  let winner = null
  let playedPlies = gamePly(game)

  try {
    while (playedPlies < maxPlies && !game.isGameOver()) {
      const fen = game.fen()
      const turn = game.turn()
      const actor = turn === 'w' ? white : black
      const { line, move } = search(actor, fen, priorFens, options)
      const evaluation = whiteScore(line, turn)

      if (evaluation !== undefined && evaluation >= options.winScoreCp) {
        whiteWinStreak += 1
        blackWinStreak = 0
      } else if (evaluation !== undefined && evaluation <= -options.winScoreCp) {
        blackWinStreak += 1
        whiteWinStreak = 0
      } else {
        whiteWinStreak = 0
        blackWinStreak = 0
      }
      drawStreak = evaluation !== undefined && Math.abs(evaluation) <= options.drawScoreCp
        ? drawStreak + 1
        : 0

      priorFens.push(fen)
      playUci(game, move)
      playedPlies += 1

      if (whiteWinStreak >= options.winPlies) {
        winner = 'white'
        reason = 'score adjudication'
        break
      }
      if (blackWinStreak >= options.winPlies) {
        winner = 'black'
        reason = 'score adjudication'
        break
      }
      if (playedPlies >= options.drawMinPly && drawStreak >= options.drawPlies) {
        reason = 'draw adjudication'
        break
      }
    }

    if (game.isGameOver()) {
      winner = terminalWinner(game)
      reason = game.isCheckmate() ? 'checkmate' : 'rules draw'
    }
    return {
      opening: opening.name,
      candidate_color: candidateColor,
      plies: playedPlies,
      result: winner === null ? 'draw' : `${winner} wins`,
      reason,
      candidateScore: candidateResult(winner, candidateColor),
    }
  } finally {
    baseline.free()
    candidate.free()
  }
}

function logisticScore(elo) {
  return 1 / (1 + 10 ** (-elo / 400))
}

function pairedProbabilities(elo, drawRate) {
  const score = logisticScore(elo)
  const win = score - drawRate / 2
  const loss = 1 - score - drawRate / 2
  if (!(win > 0 && loss > 0)) {
    throw new Error(`draw rate ${drawRate} is incompatible with Elo hypothesis ${elo}`)
  }
  return [
    loss * loss,
    2 * loss * drawRate,
    drawRate * drawRate + 2 * loss * win,
    2 * win * drawRate,
    win * win,
  ]
}

function selectOpenings(openingSuite, limit) {
  if (limit === undefined) return openingSuite
  if (limit > openingSuite.length) {
    throw new Error(`--openings cannot exceed ${openingSuite.length} unique positions`)
  }
  if (limit === 1) return [openingSuite[Math.floor(openingSuite.length / 2)]]
  return Array.from({ length: limit }, (_, index) => {
    const sourceIndex = Math.round((index * (openingSuite.length - 1)) / (limit - 1))
    return openingSuite[sourceIndex]
  })
}

function sprt(pairScores, options) {
  const counts = [0, 0, 0, 0, 0]
  for (const score of pairScores) counts[Math.round(score * 2)] += 1
  const hypothesis0 = pairedProbabilities(options.elo0, options.drawRate)
  const hypothesis1 = pairedProbabilities(options.elo1, options.drawRate)
  const llr = counts.reduce(
    (total, count, index) => total + count * Math.log(hypothesis1[index] / hypothesis0[index]),
    0,
  )
  const lower = Math.log(options.beta / (1 - options.alpha))
  const upper = Math.log((1 - options.beta) / options.alpha)
  const decision = llr >= upper
    ? `accept H1 (${options.elo1} Elo)`
    : llr <= lower
      ? `accept H0 (${options.elo0} Elo)`
      : 'continue testing'
  return { counts, llr, lower, upper, decision }
}

async function main() {
  const options = parseArgs(process.argv.slice(2))
  const [Baseline, Candidate] = await Promise.all([
    loadEngineClass(options.baseline, 'baseline'),
    loadEngineClass(options.candidate, 'candidate'),
  ])
  const games = []
  const pairScores = []
  const openingSuite = loadOpenings(options.openingsFile)
  const selectedOpenings = selectOpenings(openingSuite, options.openingLimit)

  for (const opening of selectedOpenings) {
    const first = playGame({
      Baseline,
      Candidate,
      candidateColor: 'white',
      maxPlies: options.maxPlies,
      opening,
      options,
    })
    const second = playGame({
      Baseline,
      Candidate,
      candidateColor: 'black',
      maxPlies: options.maxPlies,
      opening,
      options,
    })
    games.push(first, second)
    pairScores.push(first.candidateScore + second.candidateScore)
  }

  console.table(games.map(({ candidateScore: _, ...game }) => game))
  const wins = games.filter((game) => game.candidateScore === 1).length
  const draws = games.filter((game) => game.candidateScore === 0.5).length
  const losses = games.length - wins - draws
  const report = sprt(pairScores, options)
  console.log({
    baseline: options.baseline,
    candidate: options.candidate,
    search_limit: options.moveTimeMs === undefined
      ? `depth ${options.depth}`
      : `${options.moveTimeMs} ms/move`,
    opening_pairs: pairScores.length,
    hypotheses_elo: [options.elo0, options.elo1],
    assumed_draw_rate: options.drawRate,
    candidate_wdl: `${wins}-${draws}-${losses}`,
    candidate_score: ((wins + draws / 2) / games.length).toFixed(3),
    pair_counts_0_to_2: report.counts,
    llr: report.llr.toFixed(4),
    boundaries: [report.lower.toFixed(4), report.upper.toFixed(4)],
    decision: report.decision,
  })
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : error)
  process.exitCode = 1
})
