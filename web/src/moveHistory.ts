import type { Move } from 'chess.js'

export interface MoveRow {
  number: number
  white?: Move
  black?: Move
  whitePly?: number
  blackPly?: number
}

/** Groups verbose chess.js history without assuming the game began with White. */
export function groupMoveHistory(history: Move[]): MoveRow[] {
  const rows = new Map<number, MoveRow>()
  history.forEach((move, index) => {
    const number = Number(move.before.split(' ')[5])
    const row = rows.get(number) ?? { number }
    if (move.color === 'w') {
      row.white = move
      row.whitePly = index + 1
    } else {
      row.black = move
      row.blackPly = index + 1
    }
    rows.set(number, row)
  })
  return [...rows.values()]
}
