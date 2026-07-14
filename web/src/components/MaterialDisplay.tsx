import type { PieceSymbol } from 'chess.js'

const GLYPHS: Record<PieceSymbol, string> = { p: '♟', n: '♞', b: '♝', r: '♜', q: '♛', k: '♚' }

interface Props { pieces: PieceSymbol[]; color: 'white' | 'black'; advantage: number }

export function MaterialColumn({ pieces, color, advantage }: Props) {
  return (
    <div className="material-column" aria-label={`Captured by ${color === 'black' ? 'White' : 'Black'}`}>
      {advantage > 0 && <span className="material-advantage">+{advantage}</span>}
      <div className="material-pieces">
        {pieces.map((piece, index) => <span key={index} className={`material-piece--${color}`}>{GLYPHS[piece]}</span>)}
      </div>
    </div>
  )
}
