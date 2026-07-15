import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { Engine, initSync } from './pkg/mojo_engine.js'

const requireFromWeb = createRequire(new URL('../web/package.json', import.meta.url))
const { Chess } = requireFromWeb('chess.js')
const wasmBytes = readFileSync(new URL('./pkg/mojo_engine_bg.wasm', import.meta.url))
initSync({ module: wasmBytes })

if (typeof Engine.prototype.book_move !== 'function') {
  throw new Error('validate-book requires npm --prefix web run build:engine:book first')
}

const records = readFileSync(new URL('./book-validation.tsv', import.meta.url), 'utf8')
  .trimEnd()
  .split('\n')
  .slice(1)
  .map((line) => {
    const [fen, move, weight] = line.split('\t')
    return { fen, move, weight: Number(weight) }
  })
const byPosition = new Map()
for (const [index, record] of records.entries()) {
  if (!record.fen || !/^[a-h][1-8][a-h][1-8][nbrq]?$/.test(record.move) || record.weight < 1) {
    throw new Error(`invalid validation record ${index + 1}`)
  }
  const chess = new Chess(record.fen)
  const legal = chess.moves({ verbose: true }).some(
    (move) => `${move.from}${move.to}${move.promotion ?? ''}` === record.move,
  )
  if (!legal) throw new Error(`chess.js rejected ${record.move} for ${record.fen}`)
  const replies = byPosition.get(record.fen) ?? new Set()
  replies.add(record.move)
  byPosition.set(record.fen, replies)
}

const engine = new Engine()
try {
  for (const [fen, replies] of byPosition) {
    engine.set_position(fen, [])
    for (let seed = 0; seed < 4; seed += 1) {
      const move = engine.book_move(seed)
      if (!replies.has(move)) throw new Error(`Wasm returned unvetted reply ${move} for ${fen}`)
    }
  }
} finally {
  engine.free()
}

console.log({
  bytes: readFileSync(new URL('./book.bin', import.meta.url)).length,
  positions: byPosition.size,
  replies: records.length,
  chess_js_and_wasm_validation: 'pass',
})
