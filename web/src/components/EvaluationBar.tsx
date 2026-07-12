interface Props { scoreCp: number | null; mateIn?: number | null; result?: 'white' | 'black' | 'draw' | null }

// A mate score fills the bar toward the mating side rather than trying to
// place a bounded numeric value on an effectively infinite advantage.
const MATE_WHITE_SHARE = 100
const MATE_BLACK_SHARE = 0

export function EvaluationBar({ scoreCp, mateIn, result = null }: Props) {
  const finished = result !== null
  const hasMate = mateIn !== null && mateIn !== undefined
  const whiteAhead = finished && result !== 'draw' ? result === 'white' : hasMate ? mateIn > 0 : (scoreCp ?? 0) >= 0
  const whiteShare = finished
    ? result === 'white' ? MATE_WHITE_SHARE : result === 'black' ? MATE_BLACK_SHARE : 50
    : hasMate
    ? (whiteAhead ? MATE_WHITE_SHARE : MATE_BLACK_SHARE)
    : scoreCp === null
      ? 50
      : 50 + Math.max(-46, Math.min(46, scoreCp / 18))
  const label = finished
    ? result === 'draw' ? '' : result === 'white' ? 'White' : 'Black'
    : hasMate
    ? `M${Math.abs(mateIn)}`
    : scoreCp === null
      ? '—'
      : `${scoreCp > 0 ? '+' : ''}${(scoreCp / 100).toFixed(1)}`
  const unavailable = !finished && !hasMate && scoreCp === null
  const accessibleLabel = finished
    ? result === 'draw' ? 'Game drawn' : `${result === 'white' ? 'White' : 'Black'} won`
    : unavailable
    ? 'Evaluation unavailable'
    : hasMate
      ? `${mateIn > 0 ? 'White' : 'Black'} mates in ${Math.abs(mateIn)}`
      : `White evaluation ${label}`

  return <div className="evaluation" role="img" aria-label={accessibleLabel}><div className="evaluation__black" style={{ height: `${100 - whiteShare}%` }} />{label && <div className={`evaluation__label evaluation__label--${whiteAhead ? 'white' : 'black'}`}>{label}</div>}</div>
}
