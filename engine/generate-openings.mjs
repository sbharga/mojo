import { createHash } from 'node:crypto'
import { readFileSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { resolve } from 'node:path'

const SOURCE_URL = 'https://raw.githubusercontent.com/kevinludwig/chess-eco-codes/refs/heads/master/eco.pgn'
const requireFromWeb = createRequire(new URL('../web/package.json', import.meta.url))
const { Chess } = requireFromWeb('chess.js')

const [inputArgument, outputArgument = 'engine/openings.json'] = process.argv.slice(2)
if (!inputArgument) {
  console.error('Usage: node engine/generate-openings.mjs <eco.pgn> [output.json]')
  process.exit(1)
}

const inputPath = resolve(inputArgument)
const outputPath = resolve(outputArgument)
const source = readFileSync(inputPath, 'utf8')
const records = source.split(/(?=^\[ECO )/m).slice(1)
if (records.length === 0) throw new Error('No [ECO] records found in input PGN')

const positions = records.map((record, index) => {
  const game = new Chess()
  try {
    game.loadPgn(record)
  } catch (error) {
    throw new Error(`Invalid PGN record ${index + 1}: ${error instanceof Error ? error.message : error}`)
  }
  const headers = game.getHeaders()
  if (!headers.ECO || !headers.Opening) {
    throw new Error(`PGN record ${index + 1} is missing ECO or Opening metadata`)
  }
  return {
    eco: headers.ECO,
    opening: headers.Opening,
    ...(headers.Variation ? { variation: headers.Variation } : {}),
    name: [headers.ECO, headers.Opening, headers.Variation].filter(Boolean).join(' · '),
    fen: game.fen(),
  }
})

const uniqueFens = new Set(positions.map((position) => position.fen))
const output = {
  source: SOURCE_URL,
  source_sha256: createHash('sha256').update(source).digest('hex'),
  license: 'MIT, see engine/OPENINGS_LICENSE',
  record_count: positions.length,
  unique_fen_count: uniqueFens.size,
  positions,
}

writeFileSync(outputPath, `${JSON.stringify(output, null, 2)}\n`)
console.log({
  input: inputPath,
  output: outputPath,
  records: positions.length,
  unique_fens: uniqueFens.size,
  source_sha256: output.source_sha256,
})
