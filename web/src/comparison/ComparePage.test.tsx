// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ComparePage } from './ComparePage'
import type { EngineVersionManifest } from './types'

const matchMock = vi.hoisted(() => ({
  status: 'idle' as const,
  games: [],
  summary: { wins: 0, draws: 0, losses: 0, score: 0, completedPairs: 0, llr: 0, lower: -2.944, upper: 2.944, decision: 'More games needed' },
  configuration: null,
  startedAt: null,
  finishedAt: null,
  error: null,
  isRunning: false,
  start: vi.fn(),
  cancel: vi.fn(),
  exportData: null,
}))

vi.mock('./useComparisonMatch', () => ({ useComparisonMatch: () => matchMock }))

const manifest: EngineVersionManifest = {
  generatedAt: '2026-07-17T00:00:00Z',
  versions: [
    { sha: 'candidate', shortSha: 'candida', committedAt: '2026-07-17T00:00:00Z', subject: 'Candidate', modulePath: 'candidate/mojo_engine.js', wasmPath: 'candidate/mojo_engine_bg.wasm' },
    { sha: 'baseline', shortSha: 'baselin', committedAt: '2026-07-16T00:00:00Z', subject: 'Baseline', modulePath: 'baseline/mojo_engine.js', wasmPath: 'baseline/mojo_engine_bg.wasm' },
  ],
}

beforeEach(() => {
  matchMock.start.mockClear()
  matchMock.cancel.mockClear()
  vi.stubGlobal('fetch', vi.fn()
    .mockResolvedValueOnce({ ok: true, json: async () => manifest })
    .mockResolvedValueOnce({ ok: true, json: async () => ({ positions: [{ name: 'Opening', fen: 'fen' }] }) }))
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

describe('ComparePage', () => {
  it('loads newest commits by default and starts a fixed-depth paired match', async () => {
    const user = userEvent.setup()
    render(<ComparePage />)

    await screen.findAllByRole('option', { name: /Candidate/ })
    expect(screen.getByLabelText('Baseline commit')).toHaveProperty('value', 'baseline')
    expect(screen.getByLabelText('Candidate commit')).toHaveProperty('value', 'candidate')
    await user.click(screen.getByRole('button', { name: 'Start match' }))

    expect(matchMock.start).toHaveBeenCalledWith(
      expect.objectContaining({
        baseline: expect.objectContaining({ sha: 'baseline' }),
        candidate: expect.objectContaining({ sha: 'candidate' }),
        games: 100,
        depth: 5,
        maxPlies: 200,
      }),
      [{ name: 'Opening', fen: 'fen' }],
    )
  })

  it('requires different commits and an even number of games', async () => {
    const user = userEvent.setup()
    render(<ComparePage />)
    await screen.findAllByRole('option', { name: /Candidate/ })

    await user.selectOptions(screen.getByLabelText('Baseline commit'), 'candidate')
    expect(screen.getByText('Choose two different commits.')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Start match' })).toHaveProperty('disabled', true)

    await user.selectOptions(screen.getByLabelText('Baseline commit'), 'baseline')
    await user.clear(screen.getByLabelText('Total games'))
    await user.type(screen.getByLabelText('Total games'), '3')
    await waitFor(() => expect(screen.getByRole('button', { name: 'Start match' })).toHaveProperty('disabled', true))
  })
})
