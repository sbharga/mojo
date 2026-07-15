import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const repositoryRoot = fileURLToPath(new URL('..', import.meta.url))
const specs = {
  aspiration_initial_delta: { value: 20, min: 5, max: 100, c: 4, a: 2 },
  rfp_margin_per_ply: { value: 120, min: 40, max: 240, c: 12, a: 8 },
  futility_margin_base: { value: 100, min: 20, max: 240, c: 12, a: 8 },
  futility_margin_per_ply: { value: 100, min: 20, max: 240, c: 12, a: 8 },
  probcut_margin: { value: 180, min: 60, max: 320, c: 16, a: 10 },
  delta_pruning_margin: { value: 120, min: 40, max: 240, c: 12, a: 8 },
}

function parseArgs(argv) {
  const options = {
    iterations: 10,
    openings: 8,
    depth: 4,
    seed: 0x4d4f4a4f,
    wasm: 'engine/pkg-spsa/mojo_engine_bg.wasm',
    glue: 'engine/pkg-spsa/mojo_engine.js',
    output: 'engine/spsa-parameters.json',
  }
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index]?.slice(2).replace(/-([a-z])/g, (_, letter) => letter.toUpperCase())
    const value = argv[index + 1]
    if (!key || value === undefined || !(key in options)) throw new Error(`Invalid SPSA option ${argv[index] ?? ''}`)
    options[key] = ['wasm', 'glue', 'output'].includes(key) ? value : Number(value)
  }
  for (const key of ['iterations', 'openings', 'depth', 'seed']) {
    if (!Number.isInteger(options[key]) || options[key] < (key === 'iterations' ? 0 : 1)) {
      throw new Error(`--${key} must be an integer in range`)
    }
  }
  return options
}

function randomSigns(seed) {
  let state = seed >>> 0
  return () => {
    state ^= state << 13
    state ^= state >>> 17
    state ^= state << 5
    return (state >>> 0) & 1 ? 1 : -1
  }
}

function bounded(name, value) {
  const spec = specs[name]
  return Math.max(spec.min, Math.min(spec.max, Math.round(value)))
}

function writeParameters(path, values) {
  writeFileSync(path, `${JSON.stringify(values, null, 2)}\n`)
}

const options = parseArgs(process.argv.slice(2))
const values = Object.fromEntries(Object.entries(specs).map(([name, spec]) => [name, spec.value]))
const output = resolve(repositoryRoot, options.output)
if (options.iterations === 0) {
  writeParameters(output, values)
  console.log({ iterations: 0, parameters: values, confirmation: 'run normal fixed-time SPRT before merging' })
  process.exit(0)
}

const temporary = mkdtempSync(join(tmpdir(), 'mojo-spsa-'))
const nextSign = randomSigns(options.seed)
try {
  for (let iteration = 0; iteration < options.iterations; iteration += 1) {
    const decay = (iteration + 1) ** -0.101
    const learningDecay = (iteration + 10) ** -0.602
    const signs = Object.fromEntries(Object.keys(specs).map((name) => [name, nextSign()]))
    const plus = {}
    const minus = {}
    for (const [name, spec] of Object.entries(specs)) {
      plus[name] = bounded(name, values[name] + spec.c * decay * signs[name])
      minus[name] = bounded(name, values[name] - spec.c * decay * signs[name])
    }
    const plusPath = join(temporary, 'plus.json')
    const minusPath = join(temporary, 'minus.json')
    const reportPath = join(temporary, 'report.json')
    writeParameters(plusPath, plus)
    writeParameters(minusPath, minus)
    const match = spawnSync(process.execPath, [
      resolve(repositoryRoot, 'engine/selfplay.mjs'),
      '--baseline', options.wasm,
      '--candidate', options.wasm,
      '--glue', options.glue,
      '--baseline-params', minusPath,
      '--candidate-params', plusPath,
      '--depth', String(options.depth),
      '--openings', String(options.openings),
      '--json-output', reportPath,
    ], { cwd: repositoryRoot, stdio: 'inherit' })
    if (match.status !== 0) throw new Error(`self-play failed with status ${match.status ?? 1}`)
    const score = Number(JSON.parse(readFileSync(reportPath, 'utf8')).candidate_score)
    const signal = 2 * (score - 0.5)
    for (const [name, spec] of Object.entries(specs)) {
      values[name] = bounded(
        name,
        values[name] + spec.a * learningDecay * signal * signs[name],
      )
    }
    writeParameters(output, values)
    console.log({ iteration: iteration + 1, score, parameters: values })
  }
  console.log({ output, confirmation: 'run normal fixed-time SPRT before merging tuned values' })
} finally {
  rmSync(temporary, { recursive: true, force: true })
}
