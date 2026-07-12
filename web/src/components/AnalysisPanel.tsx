import type { Analysis } from '../engine/types'
import { formatAnalysisScore } from '../engine/analysis'

interface Props { analysis: Analysis | null; onSelectMove: (move: string) => void }

export function AnalysisPanel({ analysis, onSelectMove }: Props) {
  const lines = analysis?.lines.slice(0, 3) ?? []
  // Keyed by rank, not content: each slot is "the Nth-best line" and its
  // suggested move can change every depth iteration without the list
  // reordering, so keying by rank (not the move) keeps a focused button
  // stable across those updates instead of remounting it.
  return <section className="panel analysis-panel" aria-labelledby="analysis-heading"><div className="panel__heading"><span id="analysis-heading">Mojo analysis</span><small>{analysis ? `depth ${analysis.depth} · ${Math.round(analysis.nodes / 1000)}k nodes` : 'warming up'}</small></div><div className="analysis-lines">{lines.length ? lines.map((line, rank) => <button type="button" className="analysis-line" key={rank} disabled={!line.moves[0]} onClick={() => line.moves[0] && onSelectMove(line.moves[0])} title={line.moves[0] ? `Play ${line.moves[0]}` : undefined}><strong>{formatAnalysisScore(line)}</strong><span>{line.moves.join(' ')}</span></button>) : <div className="empty">Analysis will appear here.</div>}</div></section>
}
