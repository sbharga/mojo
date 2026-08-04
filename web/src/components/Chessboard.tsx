import type { CSSProperties } from 'react'
import { type Square } from 'chess.js'
import { Chessboard as ReactChessboard, type ChessboardOptions } from 'react-chessboard'
import type { Side } from '../engine/types'

type LegacyChessboardProps = {
  id: string
  position: string
  boardOrientation: Side
  onPieceDrop: (from: string, to: string, piece?: string) => boolean
  onPieceClick: (piece: unknown, square: string) => void
  onPieceDragBegin: (piece: unknown, square: string) => void
  onSquareClick: (square: string) => void
  arePiecesDraggable: boolean
  autoPromoteToQueen: boolean
  customArrows: Array<[Square, Square, string]>
  boardWidth: number
  customDarkSquareStyle: CSSProperties
  customLightSquareStyle: CSSProperties
  customSquareStyles: Record<string, CSSProperties>
}

// react-chessboard v5 moves its configuration into a single `options` prop.
// Keep the app's existing board interface while translating to that API.
export function Chessboard({
  id,
  position,
  boardOrientation: orientation,
  onPieceDrop,
  onPieceClick,
  onPieceDragBegin,
  onSquareClick,
  arePiecesDraggable,
  customArrows,
  boardWidth,
  customDarkSquareStyle,
  customLightSquareStyle,
  customSquareStyles,
}: LegacyChessboardProps) {
  const options: ChessboardOptions = {
    id,
    position,
    boardOrientation: orientation,
    onPieceDrop: ({ piece, sourceSquare, targetSquare }) =>
      targetSquare !== null && onPieceDrop(sourceSquare, targetSquare, piece.pieceType),
    onPieceClick: ({ piece, square }) => onPieceClick(piece, square ?? ''),
    onPieceDrag: ({ piece, square }) => onPieceDragBegin(piece, square ?? ''),
    onSquareClick: ({ square }) => onSquareClick(square),
    canDragPiece: () => arePiecesDraggable,
    arrows: customArrows.map(([startSquare, endSquare, color]) => ({
      startSquare,
      endSquare,
      color,
    })),
    boardStyle: { width: boardWidth },
    darkSquareStyle: customDarkSquareStyle,
    lightSquareStyle: customLightSquareStyle,
    squareStyles: customSquareStyles,
  }
  return <ReactChessboard options={options} />
}
