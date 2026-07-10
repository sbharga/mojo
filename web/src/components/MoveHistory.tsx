import { useEffect, useRef } from 'react'
import type { Move } from 'chess.js'

interface Props { history: Move[]; onReset: () => void; onUndo: () => void; onReview: (ply: number) => void; disabled: boolean }

export function MoveHistory({ history, onReset, onUndo, onReview, disabled }: Props) {
  const movesElement = useRef<HTMLDivElement>(null)
  const rows = [] as Array<{ number: number; white?: Move; black?: Move }>
  for (let index = 0; index < history.length; index += 2) rows.push({ number: index / 2 + 1, white: history[index], black: history[index + 1] })
  useEffect(() => {
    const element = movesElement.current
    if (element) element.scrollTop = element.scrollHeight
  }, [history.length])
  return <section className="panel move-panel"><div className="panel__heading"><span>Move history</span><div className="panel__actions"><button onClick={onUndo} disabled={disabled || history.length === 0} title="Undo last move">↶</button><button onClick={onReset} disabled={disabled} title="New game">↻</button></div></div><div className="moves" ref={movesElement}>{rows.length ? rows.map((row) => <div className="move-row" key={row.number}><span>{row.number}.</span>{row.white && <button onClick={() => onReview((row.number - 1) * 2 + 1)}>{row.white.san}</button>}{row.black && <button onClick={() => onReview(row.number * 2)}>{row.black.san}</button>}</div>) : <div className="empty">The game starts here.</div>}</div></section>
}
