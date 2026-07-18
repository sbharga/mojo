import { useCallback, useEffect, useRef, useState } from 'react'
import { selectOpenings, summarizeGames } from './match'
import type {
  ComparisonWorkerMessage,
  EngineVersion,
  GameResult,
  MatchConfiguration,
  MatchExport,
  MatchSummary,
  Opening,
} from './types'

type MatchStatus = 'idle' | 'running' | 'completed' | 'cancelled' | 'error'

interface MatchState {
  status: MatchStatus
  games: GameResult[]
  summary: MatchSummary
  configuration: MatchConfiguration | null
  startedAt: string | null
  finishedAt: string | null
  error: string | null
}

const emptySummary = summarizeGames([])
const initialState: MatchState = {
  status: 'idle',
  games: [],
  summary: emptySummary,
  configuration: null,
  startedAt: null,
  finishedAt: null,
  error: null,
}

function moduleUrl(version: EngineVersion) {
  return new URL(`${import.meta.env.BASE_URL}engine-versions/${version.modulePath}`, window.location.origin).href
}

export function useComparisonMatch() {
  const [state, setState] = useState<MatchState>(initialState)
  const workers = useRef<Worker[]>([])
  const runId = useRef(0)

  const terminate = useCallback(() => {
    workers.current.forEach((worker) => worker.terminate())
    workers.current = []
  }, [])

  useEffect(() => terminate, [terminate])

  const cancel = useCallback(() => {
    if (workers.current.length === 0) return
    runId.current += 1
    terminate()
    setState((current) => current.status !== 'running' ? current : ({
      ...current,
      status: 'cancelled',
      finishedAt: new Date().toISOString(),
    }))
  }, [terminate])

  const start = useCallback((configuration: MatchConfiguration, openingSuite: Opening[]) => {
    terminate()
    const selectedOpenings = selectOpenings(openingSuite, configuration.games / 2)
    const currentRun = runId.current + 1
    runId.current = currentRun
    const startedAt = new Date().toISOString()
    setState({
      status: 'running',
      games: [],
      summary: emptySummary,
      configuration,
      startedAt,
      finishedAt: null,
      error: null,
    })

    let nextPair = 0
    let completedPairs = 0
    let gameResults: GameResult[] = []
    let stopped = false
    const hardwareWorkers = Math.max(1, (navigator.hardwareConcurrency ?? 2) - 1)
    const workerCount = Math.min(4, hardwareWorkers, selectedOpenings.length)

    const fail = (message: string) => {
      if (stopped || currentRun !== runId.current) return
      stopped = true
      terminate()
      setState((current) => ({
        ...current,
        status: 'error',
        finishedAt: new Date().toISOString(),
        error: message,
      }))
    }

    const assign = (worker: Worker) => {
      if (stopped || currentRun !== runId.current) return
      if (nextPair >= selectedOpenings.length) return
      const pairIndex = nextPair
      nextPair += 1
      worker.postMessage({
        type: 'pair',
        runId: currentRun,
        pairIndex,
        opening: selectedOpenings[pairIndex],
      })
    }

    const instances = Array.from({ length: workerCount }, () => {
      const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' })
      worker.onerror = (event) => fail(event.message || 'Comparison worker crashed')
      worker.onmessage = (event: MessageEvent<ComparisonWorkerMessage>) => {
        const message = event.data
        if (message.runId !== currentRun || stopped) return
        if (message.type === 'ready') {
          assign(worker)
          return
        }
        if (message.type === 'error') {
          fail(`${message.pairIndex === undefined ? 'Engine initialization' : `Opening pair ${message.pairIndex + 1}`} failed: ${message.message}`)
          return
        }
        if (message.type === 'game') {
          gameResults = [...gameResults, message.game].sort((a, b) => a.gameIndex - b.gameIndex)
          const summary = summarizeGames(gameResults)
          setState((current) => ({ ...current, games: gameResults, summary }))
          return
        }
        completedPairs += 1
        if (completedPairs === selectedOpenings.length) {
          stopped = true
          terminate()
          setState((current) => ({
            ...current,
            status: 'completed',
            finishedAt: new Date().toISOString(),
          }))
        } else {
          assign(worker)
        }
      }
      worker.postMessage({
        type: 'initialize',
        runId: currentRun,
        baselineModuleUrl: moduleUrl(configuration.baseline),
        candidateModuleUrl: moduleUrl(configuration.candidate),
        baselineLabel: `${configuration.baseline.shortSha} ${configuration.baseline.subject}`,
        candidateLabel: `${configuration.candidate.shortSha} ${configuration.candidate.subject}`,
        depth: configuration.depth,
        maxPlies: configuration.maxPlies,
      })
      return worker
    })
    workers.current = instances
  }, [terminate])

  const exportData: MatchExport | null = state.configuration && state.startedAt && state.finishedAt
    ? {
        status: state.status === 'completed' ? 'completed' : state.status === 'error' ? 'error' : 'cancelled',
        startedAt: state.startedAt,
        finishedAt: state.finishedAt,
        configuration: state.configuration,
        summary: state.summary,
        games: state.games,
        ...(state.error ? { error: state.error } : {}),
      }
    : null

  return { ...state, isRunning: state.status === 'running', start, cancel, exportData }
}
