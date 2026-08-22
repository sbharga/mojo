// Measures true cancellation latency: a worker_thread runs the search while
// this (parent) thread stores into the shared stop flag partway through and
// times how long the worker takes to return afterward. Concurrency is
// essential here — engine/validate-cancellation.mjs only asserts the
// watermark logic is correct within one synchronous call; it cannot measure
// the wall-clock gap a genuinely concurrent cancel/poll relationship produces
// (the same relationship the browser's main-thread/worker-thread split has).
import { writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { Worker } from 'node:worker_threads'
import { fileURLToPath } from 'node:url'
import { repositoryRoot, writeCsv } from './lib.mjs'

const REPETITIONS = 20
const BUDGETS_MS = [100, 500, 1000, 3000]
const workerPath = fileURLToPath(new URL('./cancellation-worker.mjs', import.meta.url))

function runOnce(budgetMs, cancelAfterMs) {
  return new Promise((resolvePromise, reject) => {
    const stopBuffer = new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT)
    const stopFlag = new Int32Array(stopBuffer)
    const worker = new Worker(workerPath, { workerData: { budgetMs, stopBuffer } })
    let cancelStoredAt
    worker.on('message', (message) => {
      if (message.type === 'ready') {
        // The search has actually started; only now does cancelAfterMs
        // measure into live search time rather than worker/Wasm startup.
        setTimeout(() => {
          cancelStoredAt = performance.now()
          Atomics.store(stopFlag, 0, 1)
        }, cancelAfterMs)
        return
      }
      const observedAt = performance.now()
      worker.terminate()
      resolvePromise({
        latencyMs: observedAt - cancelStoredAt,
        timedOut: message.timedOut,
        nodes: message.nodes,
      })
    })
    worker.once('error', reject)
  })
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b)
  const mid = Math.floor(sorted.length / 2)
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2
}

const rows = []
for (const budgetMs of BUDGETS_MS) {
  // Cancel roughly a third into the budget so the search is still mid-flight.
  const cancelAfterMs = Math.max(2, Math.round(budgetMs / 3))
  const samples = []
  for (let index = 0; index < REPETITIONS; index += 1) {
    samples.push(await runOnce(budgetMs, cancelAfterMs))
  }
  if (!samples.every((sample) => sample.timedOut)) {
    throw new Error(`budget ${budgetMs}ms: cancellation was not observed on every sample`)
  }
  const latency = samples.map((sample) => sample.latencyMs)
  const latencyMedian = median(latency)
  const latencyMax = Math.max(...latency)
  rows.push([
    budgetMs,
    cancelAfterMs,
    REPETITIONS,
    latencyMedian.toFixed(3),
    latencyMax.toFixed(3),
  ])
  console.log({ budgetMs, cancelAfterMs, latencyMedian, latencyMax })
}

const csv = writeCsv(
  ['budget_ms', 'cancel_after_ms', 'repetitions', 'latency_median_ms', 'latency_max_ms'],
  rows,
)
writeFileSync(resolve(repositoryRoot, 'paper/data/cancellation.csv'), csv)
console.log('Wrote paper/data/cancellation.csv')
