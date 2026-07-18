/// <reference lib="webworker" />

import { playGame, type HistoricalEngineConstructor } from './match'
import type { ComparisonWorkerMessage, ComparisonWorkerRequest, WorkerInitializeRequest } from './types'

interface HistoricalModule {
  default: () => Promise<unknown>
  Engine: HistoricalEngineConstructor
}

let configuration: WorkerInitializeRequest | null = null
let Baseline: HistoricalEngineConstructor | null = null
let Candidate: HistoricalEngineConstructor | null = null

function send(message: ComparisonWorkerMessage) {
  postMessage(message)
}

async function initialize(request: WorkerInitializeRequest) {
  configuration = request
  const [baselineModule, candidateModule] = await Promise.all([
    import(/* @vite-ignore */ request.baselineModuleUrl) as Promise<HistoricalModule>,
    import(/* @vite-ignore */ request.candidateModuleUrl) as Promise<HistoricalModule>,
  ])
  await Promise.all([baselineModule.default(), candidateModule.default()])
  Baseline = baselineModule.Engine
  Candidate = candidateModule.Engine
  send({ type: 'ready', runId: request.runId })
}

self.onmessage = (event: MessageEvent<ComparisonWorkerRequest>) => {
  const request = event.data
  if (request.type === 'initialize') {
    void initialize(request).catch((error) => send({
      type: 'error',
      runId: request.runId,
      message: error instanceof Error ? error.message : String(error),
    }))
    return
  }
  if (!configuration || !Baseline || !Candidate || request.runId !== configuration.runId) return
  const BaselineEngine = Baseline
  const CandidateEngine = Candidate
  try {
    const colors = ['white', 'black'] as const
    colors.forEach((candidateColor, offset) => {
      const game = playGame({
        Baseline: BaselineEngine,
        Candidate: CandidateEngine,
        baselineLabel: configuration!.baselineLabel,
        candidateLabel: configuration!.candidateLabel,
        candidateColor,
        opening: request.opening,
        pairIndex: request.pairIndex,
        gameIndex: request.pairIndex * 2 + offset,
        rules: { depth: configuration!.depth, maxPlies: configuration!.maxPlies },
      })
      send({ type: 'game', runId: request.runId, game })
    })
    send({ type: 'pair-complete', runId: request.runId, pairIndex: request.pairIndex })
  } catch (error) {
    send({
      type: 'error',
      runId: request.runId,
      pairIndex: request.pairIndex,
      message: error instanceof Error ? error.message : String(error),
    })
  }
}
