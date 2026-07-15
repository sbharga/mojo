import { createHash } from 'node:crypto'
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const requireFromWeb = createRequire(new URL('../web/package.json', import.meta.url))
const { Chess } = requireFromWeb('chess.js')
const repositoryRoot = fileURLToPath(new URL('..', import.meta.url))
const [inputArgument, outputArgument = 'engine/book.bin'] = process.argv.slice(2)
if (!inputArgument) {
  console.error('Usage: node engine/generate-book.mjs <eco.pgn> [book.bin]')
  process.exit(2)
}

const source = readFileSync(resolve(repositoryRoot, inputArgument), 'utf8')
const sourceHash = createHash('sha256').update(source).digest('hex')
const expectedHash = JSON.parse(
  readFileSync(new URL('./openings.json', import.meta.url), 'utf8'),
).source_sha256
if (sourceHash !== expectedHash) throw new Error(`ECO source hash mismatch: ${sourceHash}`)

const records = source.split(/(?=^\[ECO )/m).slice(1)
const lines = records.map((record, index) => {
  const chess = new Chess()
  try {
    chess.loadPgn(record)
  } catch (error) {
    throw new Error(`Invalid PGN record ${index + 1}: ${error instanceof Error ? error.message : error}`)
  }
  return chess.history({ verbose: true })
    .map((move) => `${move.from}${move.to}${move.promotion ?? ''}`)
    .join(' ')
})

const temporary = mkdtempSync(join(tmpdir(), 'mojo-book-'))
try {
  const linePath = join(temporary, 'lines.txt')
  writeFileSync(linePath, `${lines.join('\n')}\n`)
  const result = spawnSync('cargo', [
    'run', '--quiet', '--manifest-path', resolve(repositoryRoot, 'engine/Cargo.toml'),
    '--features', 'bookgen', '--bin', 'bookgen', '--',
    linePath,
    resolve(repositoryRoot, outputArgument),
    resolve(repositoryRoot, 'engine/book-validation.tsv'),
    sourceHash,
    '10',
    '2048',
  ], { cwd: repositoryRoot, stdio: 'inherit' })
  if (result.error) throw result.error
  if (result.status !== 0) process.exit(result.status ?? 1)
} finally {
  rmSync(temporary, { recursive: true, force: true })
}
