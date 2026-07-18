import { useEffect, useMemo, useState } from 'react'
import { useComparisonMatch } from './useComparisonMatch'
import type { EngineVersionManifest, MatchConfiguration, MatchExport, Opening } from './types'

interface OpeningFile {
  positions: Opening[]
}

function versionLabel(version: EngineVersionManifest['versions'][number]) {
  return `${version.shortSha} · ${version.subject} · ${new Date(version.committedAt).toLocaleDateString()}`
}

function download(name: string, contents: string, type: string) {
  const url = URL.createObjectURL(new Blob([contents], { type }))
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = name
  anchor.click()
  URL.revokeObjectURL(url)
}

function exportJson(match: MatchExport) {
  download(
    `mojo-${match.configuration.baseline.shortSha}-vs-${match.configuration.candidate.shortSha}.json`,
    `${JSON.stringify(match, null, 2)}\n`,
    'application/json',
  )
}

function exportPgn(match: MatchExport) {
  download(
    `mojo-${match.configuration.baseline.shortSha}-vs-${match.configuration.candidate.shortSha}.pgn`,
    `${match.games.map((game) => game.pgn).join('\n\n')}\n`,
    'application/x-chess-pgn',
  )
}

interface ComparePageProps {
  onRunningChange?: (running: boolean) => void
}

export function ComparePage({ onRunningChange }: ComparePageProps) {
  const [manifest, setManifest] = useState<EngineVersionManifest | null>(null)
  const [openings, setOpenings] = useState<Opening[]>([])
  const [loadError, setLoadError] = useState<string | null>(null)
  const [baselineSha, setBaselineSha] = useState('')
  const [candidateSha, setCandidateSha] = useState('')
  const [games, setGames] = useState(100)
  const [depth, setDepth] = useState(5)
  const [clock, setClock] = useState(Date.now)
  const match = useComparisonMatch()

  useEffect(() => {
    const controller = new AbortController()
    Promise.all([
      fetch(`${import.meta.env.BASE_URL}engine-versions/manifest.json`, { signal: controller.signal }).then((response) => {
        if (!response.ok) throw new Error(`Engine catalog returned ${response.status}`)
        return response.json() as Promise<EngineVersionManifest>
      }),
      fetch(`${import.meta.env.BASE_URL}engine-versions/openings.json`, { signal: controller.signal }).then((response) => {
        if (!response.ok) throw new Error(`Opening suite returned ${response.status}`)
        return response.json() as Promise<OpeningFile>
      }),
    ]).then(([loadedManifest, openingFile]) => {
      if (loadedManifest.versions.length < 2) throw new Error('At least two historical engine versions are required')
      setManifest(loadedManifest)
      setCandidateSha(loadedManifest.versions[0].sha)
      setBaselineSha(loadedManifest.versions[1].sha)
      setOpenings(openingFile.positions)
    }).catch((error) => {
      if (error instanceof DOMException && error.name === 'AbortError') return
      setLoadError(error instanceof Error ? error.message : String(error))
    })
    return () => controller.abort()
  }, [])

  useEffect(() => {
    if (!match.isRunning) return
    const warn = (event: BeforeUnloadEvent) => {
      event.preventDefault()
      event.returnValue = true
    }
    window.addEventListener('beforeunload', warn)
    return () => window.removeEventListener('beforeunload', warn)
  }, [match.isRunning])

  useEffect(() => {
    onRunningChange?.(match.isRunning)
    return () => onRunningChange?.(false)
  }, [match.isRunning, onRunningChange])

  useEffect(() => {
    if (!match.isRunning) return
    setClock(Date.now())
    const interval = window.setInterval(() => setClock(Date.now()), 1_000)
    return () => window.clearInterval(interval)
  }, [match.isRunning])

  const configuration = useMemo<MatchConfiguration | null>(() => {
    const baseline = manifest?.versions.find((version) => version.sha === baselineSha)
    const candidate = manifest?.versions.find((version) => version.sha === candidateSha)
    return baseline && candidate ? { baseline, candidate, games, depth, maxPlies: 200 } : null
  }, [baselineSha, candidateSha, depth, games, manifest])
  const validGames = Number.isInteger(games) && games >= 2 && games <= 1000 && games % 2 === 0
  const validDepth = Number.isInteger(depth) && depth >= 1 && depth <= 6
  const valid = configuration !== null && baselineSha !== candidateSha && validGames && validDepth
  const activeGames = match.configuration?.games ?? games
  const elapsed = match.startedAt
    ? Math.max(0, Math.round(((match.finishedAt ? Date.parse(match.finishedAt) : clock) - Date.parse(match.startedAt)) / 1000))
    : 0
  const remaining = match.isRunning && match.games.length > 0
    ? Math.max(0, Math.round((elapsed / match.games.length) * (activeGames - match.games.length)))
    : null

  return <main className="app compare-app">
    <header>
      <div className="brand"><span className="brand__mark" aria-hidden="true">♞</span><div><h1>Mojo Compare</h1><p>Commit self-play laboratory</p></div></div>
      <div className="header-actions"><a className="nav-button" href="#/">Play</a><div className="status" role="status" aria-live="polite"><i className={match.isRunning ? 'ready' : ''} aria-hidden="true" />{match.isRunning ? `${match.games.length} / ${activeGames} games` : match.status === 'completed' ? 'Match complete' : match.status === 'cancelled' ? 'Match cancelled' : match.status === 'error' ? 'Match failed' : 'Ready'}</div></div>
    </header>

    <section className="compare-layout">
      <form className="panel compare-setup" onSubmit={(event) => { event.preventDefault(); if (valid && configuration) match.start(configuration, openings) }}>
        <div className="panel__heading">Match setup <small>Baseline Wasm · fixed depth</small></div>
        <div className="compare-form">
          {loadError && <p className="error-message" role="alert">{loadError}</p>}
          <label>Baseline commit<select aria-label="Baseline commit" value={baselineSha} disabled={match.isRunning || !manifest} onChange={(event) => setBaselineSha(event.target.value)}>{manifest?.versions.map((version) => <option key={version.sha} value={version.sha}>{versionLabel(version)}</option>)}</select></label>
          <label>Candidate commit<select aria-label="Candidate commit" value={candidateSha} disabled={match.isRunning || !manifest} onChange={(event) => setCandidateSha(event.target.value)}>{manifest?.versions.map((version) => <option key={version.sha} value={version.sha}>{versionLabel(version)}</option>)}</select></label>
          <div className="compare-fields">
            <label>Total games<input aria-label="Total games" type="number" min="2" max="1000" step="2" value={games} disabled={match.isRunning} onChange={(event) => setGames(Number(event.target.value))} /></label>
            <label>Search depth<input aria-label="Search depth" type="number" min="1" max="6" value={depth} disabled={match.isRunning} onChange={(event) => setDepth(Number(event.target.value))} /></label>
          </div>
          {baselineSha === candidateSha && <p className="field-error">Choose two different commits.</p>}
          {!validGames && <p className="field-error">Choose an even number from 2 to 1000 games.</p>}
          {!validDepth && <p className="field-error">Choose a search depth from 1 to 6.</p>}
          <p className="setting-note">Each opening is played twice with colors swapped. Runs stay local and stop when this page closes.</p>
          <div className="compare-actions">{match.isRunning ? <button type="button" className="danger" onClick={match.cancel}>Cancel match</button> : <button type="submit" className="primary" disabled={!valid || openings.length === 0}>Start match</button>}</div>
        </div>
      </form>

      <section className="panel compare-results" aria-label="Match results">
        <div className="panel__heading">Results <small>{elapsed}s elapsed{remaining === null ? '' : ` · ~${remaining}s remaining`}</small></div>
        <div className="progress-track" role="progressbar" aria-valuemin={0} aria-valuemax={activeGames} aria-valuenow={match.games.length} aria-label={`${match.games.length} of ${activeGames} games complete`}><span style={{ width: `${activeGames ? Math.min(100, (match.games.length / activeGames) * 100) : 0}%` }} /></div>
        <div className="summary-grid">
          <div><small>Candidate W/D/L</small><strong>{match.summary.wins} / {match.summary.draws} / {match.summary.losses}</strong></div>
          <div><small>Score</small><strong>{(match.summary.score * 100).toFixed(1)}%</strong></div>
          <div><small>Complete pairs</small><strong>{match.summary.completedPairs}</strong></div>
          <div><small>SPRT</small><strong>{match.summary.decision}</strong><span>LLR {match.summary.llr.toFixed(3)} · [{match.summary.lower.toFixed(3)}, {match.summary.upper.toFixed(3)}]</span></div>
        </div>
        {match.error && <p className="error-message" role="alert">{match.error}</p>}
        <div className="export-actions"><button type="button" disabled={!match.exportData || match.games.length === 0} onClick={() => match.exportData && exportPgn(match.exportData)}>Export PGN</button><button type="button" disabled={!match.exportData || match.games.length === 0} onClick={() => match.exportData && exportJson(match.exportData)}>Export JSON</button></div>
      </section>
    </section>

    <section className="panel game-log">
      <div className="panel__heading">Game log <small>{match.games.length} completed</small></div>
      <div className="game-table-wrap"><table><thead><tr><th>#</th><th>Opening</th><th>Candidate</th><th>Result</th><th>Reason</th><th>Plies</th></tr></thead><tbody>{match.games.map((game) => <tr key={game.gameIndex}><td>{game.gameIndex + 1}</td><td>{game.opening}</td><td>{game.candidateColor}</td><td>{game.result}</td><td>{game.reason}</td><td>{game.plies}</td></tr>)}</tbody></table>{match.games.length === 0 && <p className="empty">Configure and start a match to see individual games.</p>}</div>
    </section>
  </main>
}
