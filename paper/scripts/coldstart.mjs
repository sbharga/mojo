// Time from Wasm bytes-in-hand to a returned first move, split into
// (a) module compile/instantiate and (b) the first analyze_depth() call at a
// typical interactive budget. This is the number a browser session actually
// waits on and it does not appear anywhere in a conventional engine paper:
// throughput benchmarks assume the engine is already warm.
import { readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'
import { repositoryRoot, writeCsv } from './lib.mjs'

const REPETITIONS = 15
const BUDGET_MS = 100 // the app's MIN_THINK_TIME_MS / slider floor
const START_FEN = 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1'

const variants = [
  { name: 'baseline', wasm: 'engine/pkg/mojo_engine_bg.wasm' },
  { name: 'simd', wasm: 'engine/pkg/mojo_engine_simd_bg.wasm' },
]

async function measureOnce(variant, instance) {
  // A fresh glue-module instance per sample: wasm-bindgen glue caches its
  // module-level Wasm instance, so re-importing the same URL would reuse it
  // and hide instantiation cost on every sample after the first.
  const glueUrl = pathToFileURL(resolve(repositoryRoot, 'engine/pkg/mojo_engine.js'))
  glueUrl.searchParams.set('instance', `${variant.name}-${instance}`)
  const engineModule = await import(glueUrl.href)
  const wasmBytes = readFileSync(resolve(repositoryRoot, variant.wasm))

  const instantiateStart = performance.now()
  engineModule.initSync({ module: wasmBytes })
  const instantiateMs = performance.now() - instantiateStart

  const firstMoveStart = performance.now()
  const engine = new engineModule.Engine()
  engine.set_position(START_FEN, [])
  let result
  for (let depth = 1; depth <= 32; depth += 1) {
    result = engine.analyze_depth(depth, 1, BUDGET_MS - (performance.now() - firstMoveStart))
    if (result.timed_out || result.lines.length === 0) break
  }
  const move = result?.lines[0]?.moves[0] ?? engine.fallback_move()
  const firstMoveMs = performance.now() - firstMoveStart
  engine.free()
  if (!move) throw new Error(`${variant.name}: no move returned within cold-start budget`)
  return { instantiateMs, firstMoveMs, totalMs: instantiateMs + firstMoveMs }
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b)
  const mid = Math.floor(sorted.length / 2)
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2
}

const rows = []
for (const variant of variants) {
  const samples = []
  for (let index = 0; index < REPETITIONS; index += 1) {
    samples.push(await measureOnce(variant, index))
  }
  const instantiate = median(samples.map((s) => s.instantiateMs))
  const firstMove = median(samples.map((s) => s.firstMoveMs))
  const total = median(samples.map((s) => s.totalMs))
  rows.push([variant.name, REPETITIONS, BUDGET_MS, instantiate.toFixed(3), firstMove.toFixed(3), total.toFixed(3)])
  console.log(variant.name, { instantiate, firstMove, total })
}

const csv = writeCsv(
  ['variant', 'repetitions', 'budget_ms', 'instantiate_ms', 'first_move_ms', 'total_ms'],
  rows,
)
writeFileSync(resolve(repositoryRoot, 'paper/data/coldstart.csv'), csv)
console.log('Wrote paper/data/coldstart.csv')
