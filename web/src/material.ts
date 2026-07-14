import type { Move, PieceSymbol } from 'chess.js'

const PIECE_VALUES: Record<PieceSymbol, number> = { p: 1, n: 3, b: 3, r: 5, q: 9, k: 0 }

export interface Material {
  /** Black pieces White has captured, sorted most to least valuable. */
  capturedByWhite: PieceSymbol[]
  /** White pieces Black has captured, sorted most to least valuable. */
  capturedByBlack: PieceSymbol[]
  /** Positive favors White, negative favors Black. */
  advantage: number
}

const byValueDesc = (a: PieceSymbol, b: PieceSymbol) => PIECE_VALUES[b] - PIECE_VALUES[a]

/** Derives captured material from verbose chess.js history up to (not including) `ply`. */
export function computeMaterial(history: Move[], ply: number): Material {
  const capturedByWhite: PieceSymbol[] = []
  const capturedByBlack: PieceSymbol[] = []
  history.slice(0, ply).forEach((move) => {
    if (!move.captured) return
    if (move.color === 'w') capturedByWhite.push(move.captured)
    else capturedByBlack.push(move.captured)
  })
  const advantage =
    capturedByWhite.reduce((sum, piece) => sum + PIECE_VALUES[piece], 0) -
    capturedByBlack.reduce((sum, piece) => sum + PIECE_VALUES[piece], 0)
  capturedByWhite.sort(byValueDesc)
  capturedByBlack.sort(byValueDesc)
  return { capturedByWhite, capturedByBlack, advantage }
}
