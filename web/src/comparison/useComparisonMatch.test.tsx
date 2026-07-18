// @vitest-environment jsdom

import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useComparisonMatch } from './useComparisonMatch'
import type { ComparisonWorkerMessage, EngineVersion, GameResult, MatchConfiguration, Opening } from './types'

class WorkerStub {
  static instances: WorkerStub[] = []
  onmessage: ((event: MessageEvent<ComparisonWorkerMessage>) => void) | null = null
  onerror: ((event: ErrorEvent) => void) | null = null
  posted: unknown[] = []
  terminated = false

  constructor() {
    WorkerStub.instances.push(this)
  }

  postMessage(message: unknown) { this.posted.push(message) }
  terminate() { this.terminated = true }
  emit(message: ComparisonWorkerMessage) { this.onmessage?.({ data: message } as MessageEvent<ComparisonWorkerMessage>) }
}

const version = (sha: string): EngineVersion => ({
  sha,
  shortSha: sha.slice(0, 7),
  committedAt: '2026-07-17T00:00:00Z',
  subject: sha,
  modulePath: `${sha}/mojo_engine.js`,
  wasmPath: `${sha}/mojo_engine_bg.wasm`,
})

const configuration: MatchConfiguration = {
  baseline: version('baseline'),
  candidate: version('candidate'),
  games: 4,
  depth: 3,
  maxPlies: 200,
}

const openings: Opening[] = [
  { name: 'A', fen: 'fen-a' },
  { name: 'B', fen: 'fen-b' },
]

function game(pairIndex: number, gameIndex: number): GameResult {
  return {
    pairIndex,
    gameIndex,
    opening: openings[pairIndex].name,
    openingFen: openings[pairIndex].fen,
    candidateColor: gameIndex % 2 === 0 ? 'white' : 'black',
    winner: null,
    result: '1/2-1/2',
    candidateScore: 0.5,
    reason: 'rules draw',
    plies: 20,
    pgn: '',
  }
}

beforeEach(() => {
  WorkerStub.instances = []
  vi.stubGlobal('Worker', WorkerStub)
  Object.defineProperty(navigator, 'hardwareConcurrency', { configurable: true, value: 3 })
})

afterEach(() => vi.unstubAllGlobals())

describe('useComparisonMatch', () => {
  it('schedules opening pairs across workers and completes with sorted results', () => {
    const { result } = renderHook(() => useComparisonMatch())
    act(() => result.current.start(configuration, openings))

    expect(WorkerStub.instances).toHaveLength(2)
    const [first, second] = WorkerStub.instances
    act(() => {
      first.emit({ type: 'ready', runId: 1 })
      second.emit({ type: 'ready', runId: 1 })
    })
    expect(first.posted.at(-1)).toMatchObject({ type: 'pair', pairIndex: 0 })
    expect(second.posted.at(-1)).toMatchObject({ type: 'pair', pairIndex: 1 })

    act(() => {
      second.emit({ type: 'game', runId: 1, game: game(1, 3) })
      first.emit({ type: 'game', runId: 1, game: game(0, 0) })
      second.emit({ type: 'game', runId: 1, game: game(1, 2) })
      first.emit({ type: 'game', runId: 1, game: game(0, 1) })
      first.emit({ type: 'pair-complete', runId: 1, pairIndex: 0 })
      second.emit({ type: 'pair-complete', runId: 1, pairIndex: 1 })
    })

    expect(result.current.status).toBe('completed')
    expect(result.current.games.map((entry) => entry.gameIndex)).toEqual([0, 1, 2, 3])
    expect(result.current.summary).toMatchObject({ draws: 4, completedPairs: 2 })
    expect(result.current.exportData?.status).toBe('completed')
    expect(WorkerStub.instances.every((worker) => worker.terminated)).toBe(true)
  })

  it('terminates an active run and preserves its partial report', () => {
    const { result } = renderHook(() => useComparisonMatch())
    act(() => result.current.start(configuration, openings))
    act(() => WorkerStub.instances[0].emit({ type: 'game', runId: 1, game: game(0, 0) }))
    act(() => result.current.cancel())

    expect(result.current.status).toBe('cancelled')
    expect(result.current.games).toHaveLength(1)
    expect(result.current.exportData?.status).toBe('cancelled')
    expect(WorkerStub.instances.every((worker) => worker.terminated)).toBe(true)
  })
})
