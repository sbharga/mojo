import { describe, expect, it } from 'vitest'
import { boardOrientation, loadGame, loadSettings, rootFenForGame, saveSession } from './appState'

function memoryStorage(initial: Record<string, string> = {}) {
  const values = new Map(Object.entries(initial))
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value) },
    value: (key: string) => values.get(key),
  }
}

describe('session persistence', () => {
  it('validates settings loaded from storage', () => {
    const storage = memoryStorage({
      'mojo-settings': JSON.stringify({ mode: 'invalid', humanSide: 'black', thinkTime: 99_000, flipped: true, showBestMove: false }),
    })
    expect(loadSettings(storage)).toEqual({ mode: 'human-engine', humanSide: 'black', thinkTime: 10_000, flipped: true, showBestMove: false })
    expect(loadSettings(memoryStorage({ 'mojo-settings': '{bad json' })).mode).toBe('human-engine')
  })

  it('shows best-move arrows by default for existing saved settings', () => {
    const settings = loadSettings(memoryStorage({ 'mojo-settings': JSON.stringify({ flipped: true }) }))
    expect(settings.showBestMove).toBe(true)
  })

  it('round-trips a game with a non-standard starting position', () => {
    const storage = memoryStorage()
    const game = loadGame(memoryStorage())
    game.load('8/8/8/8/8/8/4K3/6k1 w - - 0 1')
    saveSession(storage, loadSettings(memoryStorage()), game)

    const restored = loadGame(storage)
    expect(restored.fen()).toBe(game.fen())
    expect(rootFenForGame(restored, 'unused')).toBe(game.fen())
  })

  it('falls back to a new game for corrupt PGN', () => {
    expect(loadGame(memoryStorage({ 'mojo-game': 'not pgn [' })).fen()).toBe(loadGame(memoryStorage()).fen())
  })
})

describe('board orientation', () => {
  it('uses the selected side only in human-versus-engine mode', () => {
    expect(boardOrientation('human-engine', 'black', false)).toBe('black')
    expect(boardOrientation('human-engine', 'black', true)).toBe('white')
    expect(boardOrientation('human-human', 'black', false)).toBe('white')
    expect(boardOrientation('human-human', 'black', true)).toBe('black')
  })
})
