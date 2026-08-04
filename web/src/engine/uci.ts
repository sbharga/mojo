import type { Move, Square } from 'chess.js'

export interface UciMove {
  from: Square
  to: Square
  promotion?: 'q' | 'r' | 'b' | 'n'
}

export function parseUci(uci: string): UciMove {
  return {
    from: uci.slice(0, 2) as Square,
    to: uci.slice(2, 4) as Square,
    promotion: uci[4] as 'q' | 'r' | 'b' | 'n' | undefined,
  }
}

export function formatUci(move: Move): string {
  return `${move.from}${move.to}${move.promotion ?? ''}`
}
