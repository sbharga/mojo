import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { Engine, initSync } from './pkg/mojo_engine.js'

const requireFromWeb = createRequire(new URL('../web/package.json', import.meta.url))
const { Chess } = requireFromWeb('chess.js')
const data = JSON.parse(readFileSync(new URL('./openings.json', import.meta.url), 'utf8'))
const wasmBytes = readFileSync(new URL('./pkg/mojo_engine_bg.wasm', import.meta.url))
initSync({ module: wasmBytes })

if (!Array.isArray(data.positions) || data.positions.length !== data.record_count) {
  throw new Error('record_count does not match the generated positions array')
}
if (!/^[a-f0-9]{64}$/.test(data.source_sha256)) {
  throw new Error('source_sha256 is missing or invalid')
}

const engine = new Engine()
const uniqueFens = new Set()
try {
  for (const [index, position] of data.positions.entries()) {
    if (!/^[A-E]\d{2}$/.test(position.eco) || typeof position.name !== 'string') {
      throw new Error(`invalid ECO metadata at position ${index}`)
    }
    const canonicalFen = new Chess(position.fen).fen()
    if (canonicalFen !== position.fen) {
      throw new Error(`non-canonical FEN at position ${index}: ${position.fen}`)
    }
    engine.set_position(position.fen, [])
    uniqueFens.add(position.fen)
  }
} finally {
  engine.free()
}

if (uniqueFens.size !== data.unique_fen_count) {
  throw new Error(
    `unique_fen_count is ${data.unique_fen_count}, but validation found ${uniqueFens.size}`,
  )
}

console.log({
  records: data.positions.length,
  unique_fens: uniqueFens.size,
  chess_js_and_wasm_validation: 'pass',
})
