export interface EngineVersion {
  sha: string
  shortSha: string
  committedAt: string
  subject: string
  modulePath: string
  wasmPath: string
}

export interface EngineVersionManifest {
  generatedAt: string
  versions: EngineVersion[]
}

export interface Opening {
  name: string
  fen: string
}

export type Color = 'white' | 'black'
export type Winner = Color | null
export type TerminationReason = 'checkmate' | 'rules draw' | 'score adjudication' | 'draw adjudication' | 'maximum plies'

export interface GameResult {
  pairIndex: number
  gameIndex: number
  opening: string
  openingFen: string
  candidateColor: Color
  winner: Winner
  result: '1-0' | '0-1' | '1/2-1/2'
  candidateScore: 0 | 0.5 | 1
  reason: TerminationReason
  plies: number
  pgn: string
}

export interface MatchSummary {
  wins: number
  draws: number
  losses: number
  score: number
  completedPairs: number
  llr: number
  lower: number
  upper: number
  decision: string
}

export interface MatchConfiguration {
  baseline: EngineVersion
  candidate: EngineVersion
  games: number
  depth: number
  maxPlies: number
}

export interface MatchExport {
  status: 'completed' | 'cancelled' | 'error'
  startedAt: string
  finishedAt: string
  configuration: MatchConfiguration
  summary: MatchSummary
  games: GameResult[]
  error?: string
}

export interface WorkerInitializeRequest {
  type: 'initialize'
  runId: number
  baselineModuleUrl: string
  candidateModuleUrl: string
  baselineLabel: string
  candidateLabel: string
  depth: number
  maxPlies: number
}

export interface WorkerPairRequest {
  type: 'pair'
  runId: number
  pairIndex: number
  opening: Opening
}

export type ComparisonWorkerRequest = WorkerInitializeRequest | WorkerPairRequest

export type ComparisonWorkerMessage =
  | { type: 'ready'; runId: number }
  | { type: 'game'; runId: number; game: GameResult }
  | { type: 'pair-complete'; runId: number; pairIndex: number }
  | { type: 'error'; runId: number; message: string; pairIndex?: number }
