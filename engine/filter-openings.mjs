// Builds a balanced opening suite for the self-play harness by scoring every
// ECO position in openings.json with Mojo and keeping only positions Mojo
// judges roughly equal. Lopsided openings turn a color-swapped game pair into
// two near-forced decisive games regardless of engine strength, which just
// adds variance to the SPRT; dropping them keeps the signal about the engines.
//
// Usage: node engine/filter-openings.mjs
//   DEPTH=<n>      fixed search depth per position (default 10)
//   THRESHOLD=<cp> keep |white eval| <= THRESHOLD centipawns (default 50)
//   OUT=<path>     output suite (default engine/openings-balanced.json)

import { readFileSync, writeFileSync } from 'node:fs'
import { Engine, initSync } from './pkg/mojo_engine.js'

const DEPTH = Number(process.env.DEPTH ?? 10)
const THRESHOLD = Number(process.env.THRESHOLD ?? 50)
const OUT = process.env.OUT
  ? new URL(process.env.OUT, `file://${process.cwd()}/`)
  : new URL('./openings-balanced.json', import.meta.url)

const data = JSON.parse(readFileSync(new URL('./openings.json', import.meta.url), 'utf8'))
initSync({ module: readFileSync(new URL('./pkg/mojo_engine_bg.wasm', import.meta.url)) })
const engine = new Engine()

// Mojo's best-line score, converted to White's perspective. `score_cp` is
// reported from the side to move (negamax root), matching selfplay.mjs.
function whiteEval(fen) {
  engine.set_position(fen, [])
  let done
  for (let depth = 1; depth <= DEPTH; depth += 1) {
    const result = engine.analyze_depth(depth, 1, 60_000)
    if (!result.timed_out && result.lines.length) done = result
    if (result.timed_out) break
  }
  const line = done?.lines[0]
  const forcedMate = line?.mate_in !== undefined
  const raw = forcedMate ? Math.sign(line.mate_in) * 30_000 : line?.score_cp ?? 0
  const stm = fen.split(' ')[1]
  return { cp: stm === 'w' ? raw : -raw, forcedMate }
}

const t0 = performance.now()
const balanced = []
let mates = 0
for (const [index, position] of data.positions.entries()) {
  const { cp, forcedMate } = whiteEval(position.fen)
  if (forcedMate) {
    mates += 1
  } else if (Math.abs(cp) <= THRESHOLD) {
    balanced.push({ ...position, white_eval_cp: cp })
  }
  if ((index + 1) % 200 === 0) {
    const rate = (performance.now() - t0) / (index + 1)
    process.stderr.write(
      `${index + 1}/${data.positions.length}  ~${((data.positions.length - index - 1) * rate / 1000).toFixed(0)}s left\n`,
    )
  }
}
engine.free()

const output = {
  source: 'engine/openings.json',
  generator: 'engine/filter-openings.mjs',
  balance_depth: DEPTH,
  balance_threshold_cp: THRESHOLD,
  record_count: balanced.length,
  positions: balanced,
}
writeFileSync(OUT, `${JSON.stringify(output, null, 2)}\n`)

console.log({
  scored: data.positions.length,
  kept: balanced.length,
  dropped_unbalanced: data.positions.length - balanced.length - mates,
  dropped_forced_mate: mates,
  depth: DEPTH,
  threshold_cp: THRESHOLD,
  seconds: Math.round((performance.now() - t0) / 1000),
  output: OUT.pathname,
})
