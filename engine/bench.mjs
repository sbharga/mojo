import { readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'
import { brotliCompressSync, constants, gzipSync } from 'node:zlib'

let fixedDepth
let wasmPath
let compareWasmPath
let gluePath
let jsonOutput
let repetitions = 1
let warmups = 0
for (let index = 2; index < process.argv.length; index += 1) {
  const argument = process.argv[index]
  const value = process.argv[index + 1]
  if (argument === '--fixed-depth') {
    fixedDepth = Number(value)
    if (!Number.isInteger(fixedDepth) || fixedDepth < 1) {
      throw new Error('--fixed-depth requires a positive integer')
    }
    index += 1
  } else if (argument === '--wasm') {
    if (!value) throw new Error('--wasm requires an artifact path')
    wasmPath = resolve(process.cwd(), value)
    index += 1
  } else if (argument === '--compare-wasm') {
    if (!value) throw new Error('--compare-wasm requires an artifact path')
    compareWasmPath = resolve(process.cwd(), value)
    index += 1
  } else if (argument === '--glue') {
    if (!value) throw new Error('--glue requires a module path')
    gluePath = resolve(process.cwd(), value)
    index += 1
  } else if (argument === '--json-output') {
    if (!value) throw new Error('--json-output requires a path')
    jsonOutput = resolve(process.cwd(), value)
    index += 1
  } else if (argument === '--repetitions' || argument === '--warmups') {
    const parsed = Number(value)
    const minimum = argument === '--repetitions' ? 1 : 0
    if (!Number.isInteger(parsed) || parsed < minimum) {
      throw new Error(`${argument} requires an integer >= ${minimum}`)
    }
    if (argument === '--repetitions') repetitions = parsed
    else warmups = parsed
    index += 1
  } else {
    throw new Error(`unknown argument: ${argument}`)
  }
}
if (compareWasmPath && !fixedDepth) {
  throw new Error('--compare-wasm requires --fixed-depth')
}

const defaultWasmUrl = new URL('./pkg/mojo_engine_bg.wasm', import.meta.url)
const wasmBytes = readFileSync(wasmPath ?? defaultWasmUrl)
const simdWasmBytes = wasmPath
  ? undefined
  : readFileSync(new URL('./pkg/mojo_engine_simd_bg.wasm', import.meta.url))
const positions = [
  ['start', 'rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1'],
  ['tactical', 'r3k2r/p1ppqpb1/bn2pnp1/2pP4/1p2P3/2N2N2/PPQBBPPP/R3K2R w KQkq - 0 1'],
  ['middlegame', 'r1bq1rk1/pp2bppp/2n1pn2/2pp4/3P4/2P1PN2/PP1NBPPP/R2Q1RK1 w - - 2 9'],
  ['endgame', '8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1'],
]

function runPosition(Engine, name, fen, thinkTimeMs, multiPv) {
  // Isolate each sample so an earlier budget or MultiPV run cannot warm the
  // transposition table and make later samples look artificially faster.
  const engine = new Engine()
  try {
    // Match the cross-origin-isolated browser path, including its atomic poll.
    const stopFlag = new Int32Array(new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT))
    engine.set_stop_flag(stopFlag)
    engine.set_stop_request(1)
    engine.set_position(fen, [])
    const started = performance.now()
    let latest
    for (let depth = 1; depth <= 32; depth += 1) {
      const remaining = thinkTimeMs - (performance.now() - started)
      if (remaining <= 0 && latest) break
      const result = engine.analyze_depth(depth, multiPv, Math.max(8, remaining))
      if (!result.timed_out && result.lines.length > 0) latest = result
      if (result.timed_out) break
      if (
        latest
        && performance.now() - started
          >= thinkTimeMs * (result.soft_time_fraction ?? 0.5)
      ) break
      const nextRemaining = thinkTimeMs - (performance.now() - started)
      const predictionSafety = multiPv > 1 ? 1.5 : 1.25
      if (
        !(result.ebf_gate_override ?? false)
        && (result.predicted_next_ms ?? 0) > nextRemaining * predictionSafety
      ) break
    }
    const wallMs = performance.now() - started
    return {
      position: name,
      budget_ms: thinkTimeMs,
      multipv: multiPv,
      wall_ms: wallMs,
      overrun_ms: Math.max(0, wallMs - thinkTimeMs),
      depth: latest?.depth ?? 0,
      nodes: Number(latest?.nodes ?? 0),
      clock_interval: latest?.clock_check_interval ?? null,
      best_move: latest?.lines[0]?.moves[0] ?? null,
      score_cp: latest?.lines[0]?.score_cp ?? null,
    }
  } finally {
    engine.free()
  }
}

function runFixedDepth(Engine, name, fen, targetDepth, multiPv) {
  const engine = new Engine()
  try {
    const stopFlag = new Int32Array(new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT))
    engine.set_stop_flag(stopFlag)
    engine.set_stop_request(1)
    engine.set_position(fen, [])
    const started = performance.now()
    let latest
    let cumulativeNodes = 0
    for (let depth = 1; depth <= targetDepth; depth += 1) {
      const result = engine.analyze_depth(depth, multiPv, 60_000)
      if (result.timed_out || result.lines.length === 0) {
        throw new Error(`${name} failed to complete fixed depth ${depth}`)
      }
      cumulativeNodes += Number(result.nodes)
      latest = result
    }
    return {
      position: name,
      target_depth: targetDepth,
      multipv: multiPv,
      wall_ms: performance.now() - started,
      cumulative_nodes: cumulativeNodes,
      final_iteration_nodes: Number(latest.nodes),
      best_move: latest.lines[0]?.moves[0] ?? null,
      score_cp: latest.lines[0]?.score_cp ?? null,
    }
  } finally {
    engine.free()
  }
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b)
  const middle = Math.floor(sorted.length / 2)
  return sorted.length % 2 === 0
    ? (sorted[middle - 1] + sorted[middle]) / 2
    : sorted[middle]
}

function summarize(samples) {
  const first = samples[0]
  const wallMs = median(samples.map((sample) => sample.wall_ms))
  if (!fixedDepth) {
    return {
      ...first,
      wall_ms: wallMs,
      overrun_ms: median(samples.map((sample) => sample.overrun_ms)),
      samples: samples.length,
    }
  }
  const nodeCounts = new Set(samples.map((sample) => sample.cumulative_nodes))
  const bestMoves = new Set(samples.map((sample) => sample.best_move))
  const scores = new Set(samples.map((sample) => sample.score_cp))
  if (nodeCounts.size !== 1 || bestMoves.size !== 1 || scores.size !== 1) {
    throw new Error(`non-deterministic fixed-depth result for ${first.position} MultiPV ${first.multipv}`)
  }
  return {
    ...first,
    wall_ms: wallMs,
    median_nps: first.cumulative_nodes / (wallMs / 1000),
    samples: samples.length,
  }
}

async function artifact(bytes, label, instance) {
  const glueUrl = gluePath
    ? pathToFileURL(gluePath)
    : new URL('./pkg/mojo_engine.js', import.meta.url)
  glueUrl.searchParams.set('instance', instance)
  const api = await import(glueUrl.href)
  const wasm = api.initSync({ module: bytes })
  return {
    bytes,
    label,
    Engine: api.Engine,
    initialMemory: wasm.memory.buffer.byteLength,
    wasm,
  }
}

function wasmSizes(target) {
  return {
    raw_bytes: target.bytes.byteLength,
    gzip_bytes: gzipSync(target.bytes).byteLength,
    brotli_bytes: brotliCompressSync(target.bytes, {
      params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
    }).byteLength,
    initial_memory_bytes: target.initialMemory,
    peak_memory_bytes: target.wasm.memory.buffer.byteLength,
  }
}

function runSuite(target) {
  const results = []
  for (const [name, fen] of positions) {
    const configurations = fixedDepth
      ? [[fixedDepth, 1], [fixedDepth, 3]]
      : [[100, 1], [100, 3], [500, 1], [500, 3], [1000, 1], [1000, 3]]
    for (const [limit, multiPv] of configurations) {
      for (let iteration = 0; iteration < warmups; iteration += 1) {
        if (fixedDepth) runFixedDepth(target.Engine, name, fen, limit, multiPv)
        else runPosition(target.Engine, name, fen, limit, multiPv)
      }
      const samples = []
      for (let iteration = 0; iteration < repetitions; iteration += 1) {
        samples.push(
          fixedDepth
            ? runFixedDepth(target.Engine, name, fen, limit, multiPv)
            : runPosition(target.Engine, name, fen, limit, multiPv),
        )
      }
      results.push(summarize(samples))
    }
  }
  return {
    label: target.label,
    results,
    wasm: wasmSizes(target),
  }
}

function configurations() {
  return fixedDepth
    ? [[fixedDepth, 1], [fixedDepth, 3]]
    : [[100, 1], [100, 3], [500, 1], [500, 3], [1000, 1], [1000, 3]]
}

function runSample(target, name, fen, limit, multiPv) {
  return fixedDepth
    ? runFixedDepth(target.Engine, name, fen, limit, multiPv)
    : runPosition(target.Engine, name, fen, limit, multiPv)
}

function runComparedSuites(baselineArtifact, candidateArtifact) {
  const collected = [
    { label: baselineArtifact.label, results: [] },
    { label: candidateArtifact.label, results: [] },
  ]
  for (const [name, fen] of positions) {
    for (const [limit, multiPv] of configurations()) {
      for (const target of [baselineArtifact, candidateArtifact]) {
        for (let iteration = 0; iteration < warmups; iteration += 1) {
          runSample(target, name, fen, limit, multiPv)
        }
      }
      const samples = [[], []]
      for (let iteration = 0; iteration < repetitions; iteration += 1) {
        const order = iteration % 2 === 0 ? [0, 1] : [1, 0]
        for (const index of order) {
          const target = index === 0 ? baselineArtifact : candidateArtifact
          samples[index].push(runSample(target, name, fen, limit, multiPv))
        }
      }
      collected[0].results.push(summarize(samples[0]))
      collected[1].results.push(summarize(samples[1]))
    }
  }
  collected[0].wasm = wasmSizes(baselineArtifact)
  collected[1].wasm = wasmSizes(candidateArtifact)
  return collected
}

function comparison(baseline, candidate) {
  const rows = baseline.results.map((before, index) => {
    const after = candidate.results[index]
    if (
      before.position !== after.position
      || before.multipv !== after.multipv
      || before.target_depth !== after.target_depth
    ) {
      throw new Error('benchmark suites do not have matching rows')
    }
    return {
      position: before.position,
      multipv: before.multipv,
      baseline_nps: before.median_nps,
      candidate_nps: after.median_nps,
      speedup: after.median_nps / before.median_nps,
      baseline_nodes: before.cumulative_nodes,
      candidate_nodes: after.cumulative_nodes,
      same_result: before.best_move === after.best_move && before.score_cp === after.score_cp,
    }
  })
  const singlePv = rows.filter((row) => row.multipv === 1)
  const geometricMean = Math.exp(
    singlePv.reduce((total, row) => total + Math.log(row.speedup), 0) / singlePv.length,
  )
  return { rows, single_pv_geomean_speedup: geometricMean }
}

const baselineArtifact = await artifact(
  wasmBytes,
  wasmPath ? 'tested' : 'baseline',
  'baseline',
)
const candidateArtifact = compareWasmPath
  ? await artifact(readFileSync(compareWasmPath), 'candidate', 'candidate')
  : undefined
const suites = candidateArtifact
  ? runComparedSuites(baselineArtifact, candidateArtifact)
  : [runSuite(baselineArtifact)]
const [baseline, candidate] = suites
const report = {
  fixed_depth: fixedDepth ?? null,
  warmups,
  repetitions,
  suites,
  comparison: candidate ? comparison(baseline, candidate) : undefined,
  ...(simdWasmBytes && {
    simd_wasm: {
      raw_bytes: simdWasmBytes.byteLength,
      gzip_bytes: gzipSync(simdWasmBytes).byteLength,
      brotli_bytes: brotliCompressSync(simdWasmBytes, {
        params: { [constants.BROTLI_PARAM_QUALITY]: 11 },
      }).byteLength,
    },
  }),
}

for (const suite of report.suites) {
  console.log(suite.label)
  console.table(suite.results.map((result) => ({
    ...result,
    wall_ms: Number(result.wall_ms.toFixed(3)),
    ...(result.median_nps && { median_nps: Math.round(result.median_nps) }),
  })))
  console.log(suite.wasm)
}
if (report.comparison) {
  console.table(report.comparison.rows.map((row) => ({
    ...row,
    baseline_nps: Math.round(row.baseline_nps),
    candidate_nps: Math.round(row.candidate_nps),
    speedup: `${((row.speedup - 1) * 100).toFixed(2)}%`,
  })))
  console.log({
    single_pv_geomean_speedup:
      `${((report.comparison.single_pv_geomean_speedup - 1) * 100).toFixed(2)}%`,
  })
}
if (jsonOutput) {
  writeFileSync(jsonOutput, `${JSON.stringify(report, null, 2)}\n`)
}
