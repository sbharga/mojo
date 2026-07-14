// @vitest-environment jsdom

import { Chess } from 'chess.js'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import App from './App'

const stockfishMock = vi.hoisted(() => ({ start: vi.fn(), cancel: vi.fn() }))

vi.mock('react-chessboard', () => ({
  Chessboard: ({ position, onPieceDrop }: { position: string; onPieceDrop: (from: string, to: string) => boolean }) => <div data-testid="chessboard" data-position={position}><button onClick={() => onPieceDrop('c7', 'c5')}>Play c5</button></div>,
}))

vi.mock('./engine/useEngine', () => ({
  useEngine: () => ({
    analysis: null,
    isReady: false,
    error: null,
    start: vi.fn(),
    cancel: vi.fn(),
  }),
}))

vi.mock('./engine/useStockfish', () => ({
  useStockfish: () => ({
    isReady: true,
    isThinking: false,
    error: null,
    start: stockfishMock.start,
    cancel: stockfishMock.cancel,
  }),
}))

class ResizeObserverStub {
  observe() {}
  disconnect() {}
}

beforeEach(() => {
  localStorage.clear()
  stockfishMock.start.mockClear()
  stockfishMock.cancel.mockClear()
  vi.stubGlobal('ResizeObserver', ResizeObserverStub)
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

describe('App', () => {
  it('restores a saved game into the board and move history', () => {
    const game = new Chess()
    game.move('e4')
    game.move('e5')
    localStorage.setItem('mojo-game', game.pgn())

    render(<App />)

    expect(screen.getByTestId('chessboard').getAttribute('data-position')).toBe(game.fen())
    expect(screen.getByRole('button', { name: 'Go to 1. e4' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Go to 1... e5' })).toBeTruthy()
  })

  it('opens the settings dialog and dismisses it with Escape', async () => {
    const user = userEvent.setup()
    render(<App />)

    await user.click(screen.getByRole('button', { name: 'Settings' }))
    expect(screen.getByRole('dialog', { name: 'Settings' })).toBeTruthy()
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('dialog', { name: 'Settings' })).toBeNull()
  })

  it('branches from an earlier position and clears the later history', async () => {
    const user = userEvent.setup()
    const game = new Chess()
    game.move('e4')
    game.move('e5')
    localStorage.setItem('mojo-game', game.pgn())
    render(<App />)

    await user.click(screen.getByRole('button', { name: 'Previous move' }))
    await user.click(screen.getByRole('button', { name: 'Play c5' }))
    expect(screen.queryByRole('button', { name: 'Go to 1... e5' })).toBeNull()
    expect(screen.getByRole('button', { name: 'Go to 1... c5' })).toBeTruthy()
  })

  it('preserves the current position when changing modes and resets only on request', async () => {
    const user = userEvent.setup()
    const game = new Chess()
    game.move('e4')
    game.move('e5')
    localStorage.setItem('mojo-game', game.pgn())
    render(<App />)

    await user.click(screen.getByRole('button', { name: 'Settings' }))
    await user.selectOptions(screen.getByLabelText('Mode'), 'human-human')
    expect(screen.getByTestId('chessboard').getAttribute('data-position')).toBe(game.fen())

    await user.click(screen.getByRole('button', { name: 'Reset game' }))
    expect(screen.getByTestId('chessboard').getAttribute('data-position')).toBe(new Chess().fen())
    expect(screen.queryByRole('dialog', { name: 'Settings' })).toBeNull()
  })

  it('shows configurable Stockfish controls for supported matchups', async () => {
    const user = userEvent.setup()
    render(<App />)

    await user.click(screen.getByRole('button', { name: 'Settings' }))
    await user.selectOptions(screen.getByLabelText('Mode'), 'human-stockfish')
    expect(screen.getByLabelText('Your color')).toBeTruthy()
    expect(screen.getByLabelText(/Stockfish target Elo/)).toHaveProperty('value', '2000')
    expect(screen.getByLabelText(/Stockfish time/)).toHaveProperty('value', '500')

    await user.selectOptions(screen.getByLabelText('Mode'), 'mojo-stockfish')
    expect(screen.getByLabelText('Stockfish color')).toHaveProperty('value', 'black')
    expect(screen.getByRole('button', { name: 'Start game' })).toBeTruthy()
  })

  it('starts Stockfish with persisted strength, time, and game history', () => {
    const game = new Chess()
    game.move('e4')
    localStorage.setItem('mojo-game', game.pgn())
    localStorage.setItem('mojo-settings', JSON.stringify({
      mode: 'human-stockfish',
      humanSide: 'white',
      stockfishElo: 2250,
      stockfishThinkTime: 900,
    }))

    render(<App />)

    expect(stockfishMock.start).toHaveBeenCalledWith({
      rootFen: new Chess().fen(),
      moves: ['e2e4'],
      elo: 2250,
      thinkTimeMs: 900,
    })
  })
})
