// Shared helpers for the paper's reproduction harnesses. Mirrors patterns
// already used by engine/selfplay.mjs and engine/bench.mjs so these scripts
// measure the same engine paths the shipped tooling exercises, rather than a
// parallel implementation that could silently drift from it.
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { resolve } from 'node:path'

export const repositoryRoot = fileURLToPath(new URL('../..', import.meta.url))
const requireFromWeb = createRequire(new URL('../../web/package.json', import.meta.url))

export function loadChess() {
  return requireFromWeb('chess.js').Chess
}

/** Loads a distinct `Engine` class bound to one Wasm instance, keyed by `instance`. */
export async function loadEngineClass(wasmPath, instance, gluePath = 'engine/pkg/mojo_engine.js') {
  const glueUrl = pathToFileURL(resolve(repositoryRoot, gluePath))
  glueUrl.searchParams.set('instance', instance)
  const engineModule = await import(glueUrl.href)
  const wasmBytes = readFileSync(resolve(repositoryRoot, wasmPath))
  engineModule.initSync({ module: wasmBytes })
  return engineModule.Engine
}

/** Boots the same Stockfish 18 Lite (single-threaded) build the web app bundles. */
export function loadStockfish() {
  const initEngine = requireFromWeb('stockfish')
  return initEngine('lite-single')
}

/**
 * Runs the one-time UCI handshake and Elo calibration, mirroring what
 * web/src/engine/stockfishClient.ts does once per Worker/game rather than
 * once per move. Resolves once the engine is ready to search.
 */
export function stockfishPrepare(engine, { elo }) {
  return new Promise((resolvePromise, reject) => {
    let stage = 'uci'
    const timeout = setTimeout(() => reject(new Error('Stockfish handshake timed out')), 30_000)
    engine.listener = (line) => {
      if (typeof line !== 'string') return
      if (stage === 'uci' && line === 'uciok') {
        stage = 'limit'
        engine.sendCommand('setoption name UCI_LimitStrength value true')
        if (elo !== undefined) engine.sendCommand(`setoption name UCI_Elo value ${Math.round(elo)}`)
        engine.sendCommand('isready')
        return
      }
      if (stage === 'limit' && line === 'readyok') {
        clearTimeout(timeout)
        resolvePromise()
      }
    }
    engine.sendCommand('uci')
  })
}

/** One `position` + `go movetime` search against an already-prepared engine. */
export function stockfishMove(engine, { fen, moves, moveTimeMs }) {
  return new Promise((resolvePromise, reject) => {
    const timeout = setTimeout(
      () => reject(new Error('Stockfish search timed out')),
      moveTimeMs + 10_000,
    )
    engine.listener = (line) => {
      if (typeof line !== 'string' || !line.startsWith('bestmove')) return
      clearTimeout(timeout)
      const move = line.split(/\s+/, 2)[1]
      resolvePromise(move === '(none)' ? null : move)
    }
    const movesArg = moves.length ? ` moves ${moves.join(' ')}` : ''
    engine.sendCommand(`position fen ${fen}${movesArg}`)
    engine.sendCommand(`go movetime ${Math.max(1, Math.round(moveTimeMs))}`)
  })
}

export function loadOpenings(path) {
  const source = resolve(repositoryRoot, path)
  const parsed = JSON.parse(readFileSync(source, 'utf8'))
  const entries = Array.isArray(parsed) ? parsed : parsed.positions
  return entries.filter((entry) => typeof entry?.fen === 'string')
}

/** Evenly spaced sample of `count` openings across the suite (same rule as selfplay.mjs). */
export function selectOpenings(openingSuite, count) {
  if (count >= openingSuite.length) return openingSuite
  if (count === 1) return [openingSuite[Math.floor(openingSuite.length / 2)]]
  return Array.from({ length: count }, (_, index) => {
    const sourceIndex = Math.round((index * (openingSuite.length - 1)) / (count - 1))
    return openingSuite[sourceIndex]
  })
}

function logisticScore(elo) {
  return 1 / (1 + 10 ** (-elo / 400))
}

/** Performance rating of a candidate scoring `scorePct` (0-100) against a fixed-strength opponent. */
export function performanceElo(opponentElo, scorePct) {
  const score = Math.min(0.995, Math.max(0.005, scorePct / 100))
  return opponentElo + 400 * Math.log10(score / (1 - score))
}

export function deltaElo(scorePct) {
  const score = Math.min(0.995, Math.max(0.005, scorePct / 100))
  return 400 * Math.log10(score / (1 - score))
}

/** Builds CSV text (does not write to disk — callers own the destination path). */
export function writeCsv(header, rows) {
  const lines = [header.join(','), ...rows.map((row) => row.join(','))]
  return `${lines.join('\n')}\n`
}
