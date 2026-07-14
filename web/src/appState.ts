import { Chess } from 'chess.js'
import type { EngineMode, Side } from './engine/types'

export const DEFAULT_SETTINGS = {
  mode: 'human-engine' as EngineMode,
  humanSide: 'white' as Side,
  thinkTime: 500,
  stockfishElo: 2000,
  stockfishThinkTime: 500,
  stockfishSide: 'black' as Side,
  flipped: false,
  showBestMove: true,
}

const MIN_THINK_TIME_MS = 100
const MAX_THINK_TIME_MS = 10_000
const MIN_STOCKFISH_ELO = 1320
const MAX_STOCKFISH_ELO = 3190

type StorageReader = Pick<Storage, 'getItem'>
type StorageWriter = Pick<Storage, 'setItem'>

/** Reads settings defensively so stale or hand-edited storage cannot break the UI. */
export function loadSettings(storage: StorageReader): typeof DEFAULT_SETTINGS {
  try {
    const raw = storage.getItem('mojo-settings')
    if (!raw) return DEFAULT_SETTINGS
    const value = JSON.parse(raw) as Record<string, unknown>
    return {
      mode: value.mode === 'human-engine' || value.mode === 'human-stockfish' || value.mode === 'engine-engine' || value.mode === 'mojo-stockfish' || value.mode === 'human-human'
        ? value.mode
        : DEFAULT_SETTINGS.mode,
      humanSide: value.humanSide === 'white' || value.humanSide === 'black'
        ? value.humanSide
        : DEFAULT_SETTINGS.humanSide,
      thinkTime: typeof value.thinkTime === 'number' && Number.isFinite(value.thinkTime)
        ? Math.min(MAX_THINK_TIME_MS, Math.max(MIN_THINK_TIME_MS, value.thinkTime))
        : DEFAULT_SETTINGS.thinkTime,
      stockfishElo: typeof value.stockfishElo === 'number' && Number.isFinite(value.stockfishElo)
        ? Math.round(Math.min(MAX_STOCKFISH_ELO, Math.max(MIN_STOCKFISH_ELO, value.stockfishElo)))
        : DEFAULT_SETTINGS.stockfishElo,
      stockfishThinkTime: typeof value.stockfishThinkTime === 'number' && Number.isFinite(value.stockfishThinkTime)
        ? Math.min(MAX_THINK_TIME_MS, Math.max(MIN_THINK_TIME_MS, value.stockfishThinkTime))
        : DEFAULT_SETTINGS.stockfishThinkTime,
      stockfishSide: value.stockfishSide === 'white' || value.stockfishSide === 'black'
        ? value.stockfishSide
        : DEFAULT_SETTINGS.stockfishSide,
      flipped: typeof value.flipped === 'boolean' ? value.flipped : DEFAULT_SETTINGS.flipped,
      showBestMove: typeof value.showBestMove === 'boolean' ? value.showBestMove : DEFAULT_SETTINGS.showBestMove,
    }
  } catch {
    return DEFAULT_SETTINGS
  }
}

/** Restores a saved PGN, falling back to a fresh game when storage is unavailable or corrupt. */
export function loadGame(storage: StorageReader): Chess {
  try {
    const pgn = storage.getItem('mojo-game')
    if (pgn) {
      const game = new Chess()
      game.loadPgn(pgn)
      return game
    }
  } catch {
    // A game must remain playable when local storage contains invalid data.
  }
  return new Chess()
}

export function saveSession(storage: StorageWriter, settings: typeof DEFAULT_SETTINGS, game: Chess) {
  try {
    storage.setItem('mojo-settings', JSON.stringify(settings))
    storage.setItem('mojo-game', game.pgn())
  } catch {
    // Private browsing and storage quotas must not interrupt a live game.
  }
}

export function rootFenForGame(game: Chess, defaultFen: string) {
  return game.getHeaders().FEN ?? defaultFen
}

export function boardOrientation(mode: EngineMode, humanSide: Side, flipped: boolean): Side {
  const base = mode === 'human-engine' || mode === 'human-stockfish' ? humanSide : 'white'
  if (!flipped) return base
  return base === 'white' ? 'black' : 'white'
}
