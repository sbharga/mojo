interface Props { scoreCp: number | null; mateIn?: number | null }

// A mate score fills the bar toward the mating side rather than trying to
// place a bounded numeric value on an effectively infinite advantage.
const MATE_WHITE_SHARE = 96
const MATE_BLACK_SHARE = 4

export function EvaluationBar({ scoreCp, mateIn }: Props) {
  const hasMate = mateIn !== null && mateIn !== undefined
  const whiteAhead = hasMate ? mateIn > 0 : (scoreCp ?? 0) >= 0
  const whiteShare = hasMate
    ? (whiteAhead ? MATE_WHITE_SHARE : MATE_BLACK_SHARE)
    : scoreCp === null
      ? 50
      : 50 + Math.max(-46, Math.min(46, scoreCp / 18))
  const label = hasMate
    ? `M${Math.abs(mateIn)}`
    : scoreCp === null
      ? '—'
      : `${scoreCp > 0 ? '+' : ''}${(scoreCp / 100).toFixed(1)}`
  const unavailable = !hasMate && scoreCp === null

  return <div className="evaluation" aria-label={unavailable ? 'Evaluation unavailable' : `White evaluation ${label}`}><div className="evaluation__black" style={{ height: `${100 - whiteShare}%` }} /><div className={`evaluation__label evaluation__label--${whiteAhead ? 'white' : 'black'}`}>{label}</div></div>
}
