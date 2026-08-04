import { Chess } from 'chess.js'
import type { Analysis, SearchSeed } from './types'
import { parseUci } from './uci'

export function ponderSeed(currentFen: string, predecessor: Analysis | null): SearchSeed | null {
  const line = predecessor?.lines[0]
  const expectedMove = line?.moves[0]
  if (!predecessor || !line || !expectedMove || line.moves.length < 2) return null
  try {
    const position = new Chess(predecessor.root_fen)
    position.move(parseUci(expectedMove))
    if (position.fen() !== currentFen) return null
  } catch {
    return null
  }
  if (line.score_cp === undefined && line.mate_in === undefined) return null
  return {
    moves: line.moves.slice(1),
    depth: Math.max(1, predecessor.depth - 1),
    ...(line.score_cp === undefined ? {} : { score_cp: -line.score_cp }),
    ...(line.mate_in === undefined ? {} : { mate_in: -line.mate_in }),
  }
}
