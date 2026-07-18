import { describe, expect, it } from 'vitest'
import { playGame, selectOpenings, summarizeGames, type HistoricalEngine } from './match'
import type { GameResult, Opening } from './types'

const openings: Opening[] = [
  { name: 'A', fen: 'fen-a' },
  { name: 'B', fen: 'fen-b' },
  { name: 'Duplicate B', fen: 'fen-b' },
  { name: 'C', fen: 'fen-c' },
  { name: 'D', fen: 'fen-d' },
  { name: 'E', fen: 'fen-e' },
]

function result(pairIndex: number, candidateScore: GameResult['candidateScore']): GameResult {
  const winner = candidateScore === 0.5 ? null : candidateScore === 1 ? 'white' : 'black'
  return {
    pairIndex,
    gameIndex: pairIndex * 2,
    opening: 'Test',
    openingFen: '8/8/8/8/8/8/8/8 w - - 0 1',
    candidateColor: 'white',
    winner,
    result: winner === 'white' ? '1-0' : winner === 'black' ? '0-1' : '1/2-1/2',
    candidateScore,
    reason: 'maximum plies',
    plies: 1,
    pgn: '',
  }
}

describe('selectOpenings', () => {
  it('deduplicates positions and samples the suite deterministically', () => {
    expect(selectOpenings(openings, 3).map((opening) => opening.name)).toEqual(['A', 'C', 'E'])
    expect(selectOpenings(openings, 1)).toEqual([{ name: 'C', fen: 'fen-c' }])
  })

  it('rejects invalid pair counts', () => {
    expect(() => selectOpenings(openings, 0)).toThrow('between 1 and 5')
    expect(() => selectOpenings(openings, 6)).toThrow('between 1 and 5')
  })
})

describe('summarizeGames', () => {
  it('reports candidate results and ignores incomplete pairs in the LLR', () => {
    const summary = summarizeGames([result(0, 1), result(0, 0.5), result(1, 0)])
    expect(summary).toMatchObject({ wins: 1, draws: 1, losses: 1, score: 0.5, completedPairs: 1 })
    expect(summary.decision).toBe('More games needed')
  })

  it('crosses both paired SPRT decision bounds', () => {
    const wins = Array.from({ length: 30 }, (_, pair) => [result(pair, 1), result(pair, 1)]).flat()
    const losses = Array.from({ length: 30 }, (_, pair) => [result(pair, 0), result(pair, 0)]).flat()
    expect(summarizeGames(wins).decision).toBe('Candidate is at least +10 Elo')
    expect(summarizeGames(losses).decision).toBe('Candidate improvement rejected')
  })
})

describe('playGame', () => {
  it('records a terminal opening without asking either engine to search', () => {
    class TerminalEngine implements HistoricalEngine {
      set_position() { throw new Error('search should not run') }
      analyze_depth(): never { throw new Error('search should not run') }
      fallback_move() { return undefined }
      free() {}
    }
    const game = playGame({
      Baseline: TerminalEngine,
      Candidate: TerminalEngine,
      baselineLabel: 'base',
      candidateLabel: 'candidate',
      candidateColor: 'black',
      opening: { name: 'Mate', fen: '7k/6Q1/6K1/8/8/8/8/8 b - - 0 1' },
      pairIndex: 0,
      gameIndex: 0,
      rules: { depth: 1, maxPlies: 200 },
    })
    expect(game).toMatchObject({ winner: 'white', candidateScore: 0, result: '1-0', reason: 'checkmate' })
  })
})
