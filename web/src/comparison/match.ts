import { Chess, type Square } from 'chess.js'
import type { Color, GameResult, MatchSummary, Opening, Winner } from './types'

export interface AnalysisLineLike {
  score_cp?: number
  mate_in?: number
  moves: string[]
}

interface AnalysisLike {
  timed_out?: boolean
  lines?: AnalysisLineLike[]
}

export interface HistoricalEngine {
  set_position(fen: string, priorFens: string[]): void
  analyze_depth(depth: number, multiPv: number, timeLimitMs: number): AnalysisLike
  fallback_move(): string | undefined
  free(): void
}

export interface HistoricalEngineConstructor {
  new (): HistoricalEngine
}

export interface MatchRules {
  depth: number
  maxPlies: number
  winScoreCp?: number
  winPlies?: number
  drawScoreCp?: number
  drawPlies?: number
  drawMinPly?: number
}

const DEFAULT_RULES = {
  winScoreCp: 1200,
  winPlies: 6,
  drawScoreCp: 20,
  drawPlies: 12,
  drawMinPly: 80,
}

function playUci(game: Chess, uci: string) {
  game.move({
    from: uci.slice(0, 2) as Square,
    to: uci.slice(2, 4) as Square,
    promotion: uci[4] as 'q' | 'r' | 'b' | 'n' | undefined,
  })
}

function gamePly(game: Chess) {
  const fullmove = Number(game.fen().split(' ')[5])
  return (fullmove - 1) * 2 + (game.turn() === 'b' ? 1 : 0)
}

function search(engine: HistoricalEngine, fen: string, priorFens: string[], depth: number) {
  engine.set_position(fen, priorFens)
  let completed: AnalysisLike | undefined
  for (let currentDepth = 1; currentDepth <= depth; currentDepth += 1) {
    const result = engine.analyze_depth(currentDepth, 1, 60_000)
    if (!result.timed_out && result.lines?.length) completed = result
    if (result.timed_out) break
  }
  const line = completed?.lines?.[0]
  const move = line?.moves[0] ?? engine.fallback_move()
  if (!move) throw new Error(`Engine found no move in non-terminal position ${fen}`)
  return { line, move }
}

function whiteScore(line: AnalysisLineLike | undefined, turn: 'w' | 'b') {
  let score: number | undefined
  if (typeof line?.mate_in === 'number') score = Math.sign(line.mate_in) * 30_000
  else if (typeof line?.score_cp === 'number') score = line.score_cp
  return score === undefined || turn === 'w' ? score : -score
}

function terminalWinner(game: Chess): Winner {
  if (!game.isCheckmate()) return null
  return game.turn() === 'w' ? 'black' : 'white'
}

function resultToken(winner: Winner): GameResult['result'] {
  return winner === 'white' ? '1-0' : winner === 'black' ? '0-1' : '1/2-1/2'
}

function candidateScore(winner: Winner, candidateColor: Color): GameResult['candidateScore'] {
  if (winner === null) return 0.5
  return winner === candidateColor ? 1 : 0
}

export function selectOpenings(openings: Opening[], pairCount: number) {
  const seen = new Set<string>()
  const unique = openings.filter((opening) => {
    if (seen.has(opening.fen)) return false
    seen.add(opening.fen)
    return true
  })
  if (!Number.isInteger(pairCount) || pairCount < 1 || pairCount > unique.length) {
    throw new Error(`Opening pair count must be between 1 and ${unique.length}`)
  }
  if (pairCount === 1) return [unique[Math.floor(unique.length / 2)]]
  return Array.from({ length: pairCount }, (_, index) =>
    unique[Math.round((index * (unique.length - 1)) / (pairCount - 1))],
  )
}

export function playGame(args: {
  Baseline: HistoricalEngineConstructor
  Candidate: HistoricalEngineConstructor
  baselineLabel: string
  candidateLabel: string
  candidateColor: Color
  opening: Opening
  pairIndex: number
  gameIndex: number
  rules: MatchRules
}): GameResult {
  const {
    Baseline,
    Candidate,
    baselineLabel,
    candidateLabel,
    candidateColor,
    opening,
    pairIndex,
    gameIndex,
  } = args
  const rules = { ...DEFAULT_RULES, ...args.rules }
  const baseline = new Baseline()
  const candidate = new Candidate()
  const game = new Chess(opening.fen)
  const priorFens: string[] = []
  const white = candidateColor === 'white' ? candidate : baseline
  const black = candidateColor === 'black' ? candidate : baseline
  let whiteWinStreak = 0
  let blackWinStreak = 0
  let drawStreak = 0
  let reason: GameResult['reason'] = 'maximum plies'
  let winner: Winner = null
  let playedPlies = gamePly(game)

  game.header(
    'Event', 'Mojo commit comparison',
    'White', candidateColor === 'white' ? candidateLabel : baselineLabel,
    'Black', candidateColor === 'black' ? candidateLabel : baselineLabel,
    'SetUp', '1',
    'FEN', opening.fen,
  )

  try {
    while (playedPlies < rules.maxPlies && !game.isGameOver()) {
      const fen = game.fen()
      const turn = game.turn()
      const actor = turn === 'w' ? white : black
      const { line, move } = search(actor, fen, priorFens, rules.depth)
      const evaluation = whiteScore(line, turn)

      if (evaluation !== undefined && evaluation >= rules.winScoreCp) {
        whiteWinStreak += 1
        blackWinStreak = 0
      } else if (evaluation !== undefined && evaluation <= -rules.winScoreCp) {
        blackWinStreak += 1
        whiteWinStreak = 0
      } else {
        whiteWinStreak = 0
        blackWinStreak = 0
      }
      drawStreak = evaluation !== undefined && Math.abs(evaluation) <= rules.drawScoreCp
        ? drawStreak + 1
        : 0

      priorFens.push(fen)
      playUci(game, move)
      playedPlies += 1

      if (whiteWinStreak >= rules.winPlies) {
        winner = 'white'
        reason = 'score adjudication'
        break
      }
      if (blackWinStreak >= rules.winPlies) {
        winner = 'black'
        reason = 'score adjudication'
        break
      }
      if (playedPlies >= rules.drawMinPly && drawStreak >= rules.drawPlies) {
        reason = 'draw adjudication'
        break
      }
    }

    if (game.isGameOver()) {
      winner = terminalWinner(game)
      reason = game.isCheckmate() ? 'checkmate' : 'rules draw'
    }
    const result = resultToken(winner)
    game.header('Result', result, 'Termination', reason, 'Opening', opening.name)
    return {
      pairIndex,
      gameIndex,
      opening: opening.name,
      openingFen: opening.fen,
      candidateColor,
      winner,
      result,
      candidateScore: candidateScore(winner, candidateColor),
      reason,
      plies: playedPlies,
      pgn: game.pgn({ maxWidth: 80, newline: '\n' }),
    }
  } finally {
    baseline.free()
    candidate.free()
  }
}

function logisticScore(elo: number) {
  return 1 / (1 + 10 ** (-elo / 400))
}

function pairedProbabilities(elo: number, drawRate: number) {
  const score = logisticScore(elo)
  const win = score - drawRate / 2
  const loss = 1 - score - drawRate / 2
  return [loss * loss, 2 * loss * drawRate, drawRate * drawRate + 2 * loss * win, 2 * win * drawRate, win * win]
}

export function summarizeGames(games: GameResult[]): MatchSummary {
  const wins = games.filter((game) => game.candidateScore === 1).length
  const draws = games.filter((game) => game.candidateScore === 0.5).length
  const losses = games.length - wins - draws
  const pairs = new Map<number, number[]>()
  for (const game of games) {
    const scores = pairs.get(game.pairIndex) ?? []
    scores.push(game.candidateScore)
    pairs.set(game.pairIndex, scores)
  }
  const pairScores = [...pairs.values()].filter((scores) => scores.length === 2).map((scores) => scores[0] + scores[1])
  const counts = [0, 0, 0, 0, 0]
  for (const score of pairScores) counts[Math.round(score * 2)] += 1
  const h0 = pairedProbabilities(0, 0.5)
  const h1 = pairedProbabilities(10, 0.5)
  const llr = counts.reduce((total, count, index) => total + count * Math.log(h1[index] / h0[index]), 0)
  const lower = Math.log(0.05 / 0.95)
  const upper = Math.log(0.95 / 0.05)
  const decision = llr >= upper ? 'Candidate is at least +10 Elo' : llr <= lower ? 'Candidate improvement rejected' : 'More games needed'
  return {
    wins,
    draws,
    losses,
    score: games.length ? (wins + draws / 2) / games.length : 0,
    completedPairs: pairScores.length,
    llr,
    lower,
    upper,
    decision,
  }
}
