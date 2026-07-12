import { useEffect, useRef } from 'react'
import type { Move } from 'chess.js'
import { groupMoveHistory } from '../moveHistory'

interface Props { history: Move[]; currentPly: number; onNavigate: (ply: number) => void }

export function MoveHistory({ history, currentPly, onNavigate }: Props) {
  const movesElement = useRef<HTMLDivElement>(null)
  const rows = groupMoveHistory(history)
  useEffect(() => {
    const element = movesElement.current
    if (element) element.scrollTop = element.scrollHeight
  }, [history.length])
  return <section className="panel move-panel" aria-labelledby="move-history-heading"><div className="panel__heading"><span id="move-history-heading">Move history</span><div className="history-controls"><button type="button" onClick={() => onNavigate(0)} disabled={currentPly === 0} title="Go to start" aria-label="Go to start">⏮</button><button type="button" onClick={() => onNavigate(currentPly - 1)} disabled={currentPly === 0} title="Previous move" aria-label="Previous move">◀</button><button type="button" onClick={() => onNavigate(currentPly + 1)} disabled={currentPly === history.length} title="Next move" aria-label="Next move">▶</button><button type="button" onClick={() => onNavigate(history.length)} disabled={currentPly === history.length} title="Go to latest" aria-label="Go to latest">⏭</button></div></div><div className="moves" ref={movesElement}>{rows.length ? rows.map((row) => <div className="move-row" key={row.number}><span>{row.number}.</span>{row.white && row.whitePly !== undefined && <button type="button" className={currentPly === row.whitePly ? 'active' : ''} onClick={() => onNavigate(row.whitePly!)} aria-label={`Go to ${row.number}. ${row.white.san}`}>{row.white.san}</button>}{row.black && row.blackPly !== undefined && <button type="button" className={currentPly === row.blackPly ? 'active' : ''} onClick={() => onNavigate(row.blackPly!)} aria-label={`Go to ${row.number}... ${row.black.san}`}>{row.black.san}</button>}</div>) : <div className="empty">The game starts here.</div>}</div></section>
}
