import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { CSSProperties } from 'react'
import { Chess, type Move, type Square } from 'chess.js'
import { Chessboard as ReactChessboard, type ChessboardOptions } from 'react-chessboard'
import { AnalysisPanel } from './components/AnalysisPanel'
import { EvaluationBar } from './components/EvaluationBar'
import { MoveHistory } from './components/MoveHistory'
import { SettingsPanel } from './components/SettingsPanel'
import { SetupDialog } from './components/SetupDialog'
import { useEngine } from './engine/useEngine'
import { useStockfish } from './engine/useStockfish'
import { bestMoveForPosition } from './engine/analysis'
import type { EngineMode, Side } from './engine/types'
import { boardOrientation, loadGame, loadSettings, rootFenForGame, saveSession } from './appState'
import './styles.css'

const initialFen = new Chess().fen()
type Dialog = 'fen' | 'pgn' | 'export' | 'settings' | null

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
function Chessboard({
  id, position, boardOrientation: orientation, onPieceDrop, onPieceClick,
  onPieceDragBegin, onSquareClick, arePiecesDraggable, customArrows,
  boardWidth, customDarkSquareStyle, customLightSquareStyle, customSquareStyles,
}: LegacyChessboardProps) {
  const options: ChessboardOptions = {
    id,
    position,
    boardOrientation: orientation,
    onPieceDrop: ({ piece, sourceSquare, targetSquare }) => targetSquare !== null && onPieceDrop(sourceSquare, targetSquare, piece.pieceType),
    onPieceClick: ({ piece, square }) => onPieceClick(piece, square ?? ''),
    onPieceDrag: ({ piece, square }) => onPieceDragBegin(piece, square ?? ''),
    onSquareClick: ({ square }) => onSquareClick(square),
    canDragPiece: () => arePiecesDraggable,
    arrows: customArrows.map(([startSquare, endSquare, color]) => ({ startSquare, endSquare, color })),
    boardStyle: { width: boardWidth },
    darkSquareStyle: customDarkSquareStyle,
    lightSquareStyle: customLightSquareStyle,
    squareStyles: customSquareStyles,
  }
  return <ReactChessboard options={options} />
}

function uciMove(uci: string) {
  return { from: uci.slice(0, 2) as Square, to: uci.slice(2, 4) as Square, promotion: uci[4] as 'q' | 'r' | 'b' | 'n' | undefined }
}

function moveToUci(move: Move) {
  return `${move.from}${move.to}${move.promotion ?? ''}`
}

function App() {
  const [initialGame] = useState(() => loadGame(localStorage))
  const [initialSettings] = useState(() => loadSettings(localStorage))
  const game = useRef(initialGame)
  const [fen, setFen] = useState(() => initialGame.fen())
  const [rootFen, setRootFen] = useState(() => rootFenForGame(initialGame, initialFen))
  const [history, setHistory] = useState<Move[]>(() => initialGame.history({ verbose: true }))
  const [mode, setModeState] = useState<EngineMode>(initialSettings.mode)
  const [humanSide, setHumanSide] = useState<Side>(initialSettings.humanSide)
  const [thinkTime, setThinkTime] = useState(initialSettings.thinkTime)
  const [stockfishElo, setStockfishElo] = useState(initialSettings.stockfishElo)
  const [stockfishThinkTime, setStockfishThinkTime] = useState(initialSettings.stockfishThinkTime)
  const [stockfishSide, setStockfishSide] = useState<Side>(initialSettings.stockfishSide)
  const [flipped, setFlipped] = useState(initialSettings.flipped)
  const [showBestMove, setShowBestMove] = useState(initialSettings.showBestMove)
  const [running, setRunning] = useState(false)
  const [dialog, setDialog] = useState<Dialog>(null)
  const [viewPly, setViewPly] = useState(() => initialGame.history().length)
  const [selectedSquare, setSelectedSquare] = useState<Square | null>(null)
  const boardShell = useRef<HTMLDivElement>(null)
  const [boardWidth, setBoardWidth] = useState(720)
  const settingsDialogRef = useRef<HTMLDialogElement>(null)
  const closeSettings = () => settingsDialogRef.current?.close()
  const sync = useCallback(() => {
    const nextHistory = game.current.history({ verbose: true })
    setFen(game.current.fen())
    setHistory(nextHistory)
    setViewPly(nextHistory.length)
    setSelectedSquare(null)
  }, [])
  const applyEngineMove = useCallback((uci: string) => {
    if (game.current.isGameOver()) return
    try { game.current.move(uciMove(uci)); sync() } catch { /* A stale move is safely ignored. */ }
  }, [sync])
  const { analysis, isReady: isMojoReady, error: mojoError, start, cancel } = useEngine(applyEngineMove)
  const stockfishEnabled = mode === 'human-stockfish' || mode === 'mojo-stockfish'
  const { isReady: isStockfishReady, isThinking: stockfishThinking, error: stockfishError, start: startStockfish, cancel: cancelStockfish } = useStockfish(stockfishEnabled, applyEngineMove)

  const turn = game.current.turn() === 'w' ? 'white' : 'black'
  const gameOver = game.current.isGameOver()
  const gameResult: 'white' | 'black' | 'draw' | null = gameOver
    ? game.current.isCheckmate() ? (turn === 'white' ? 'black' : 'white') : 'draw'
    : null
  const atLatest = viewPly === history.length
  const positionHistory = useMemo(() => history.slice(0, viewPly).map((move) => move.before), [history, viewPly])
  const stockfishMoves = useMemo(() => history.map(moveToUci), [history])
  const mojoToMove = atLatest && !gameOver && ((mode === 'human-engine' && turn !== humanSide) || (mode === 'engine-engine' && running) || (mode === 'mojo-stockfish' && running && turn !== stockfishSide))
  const stockfishToMove = atLatest && !gameOver && ((mode === 'human-stockfish' && turn !== humanSide) || (mode === 'mojo-stockfish' && running && turn === stockfishSide))
  const humanCanMove = !gameOver && (!atLatest || mode === 'human-human' || ((mode === 'human-engine' || mode === 'human-stockfish') && turn === humanSide))
  const isReady = isMojoReady && (!stockfishEnabled || isStockfishReady)
  const error = stockfishEnabled ? stockfishError ?? mojoError : mojoError
  const orientation = boardOrientation(mode, humanSide, flipped)
  // Keep the desktop sidebar aligned with the square board. Responsive CSS
  // releases this fixed height once the panels stack below the board.
  const sidebarHeight = Math.max(boardWidth, 540)

  useEffect(() => {
    const shell = boardShell.current
    if (!shell) return
    const updateWidth = () => setBoardWidth(Math.max(220, Math.floor(shell.clientWidth)))
    updateWidth()
    const observer = new ResizeObserver(updateWidth)
    observer.observe(shell)
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    const savedGame = new Chess(rootFen)
    history.forEach((move) => savedGame.move({ from: move.from, to: move.to, promotion: move.promotion }))
    saveSession(localStorage, { mode, humanSide, thinkTime, stockfishElo, stockfishThinkTime, stockfishSide, flipped, showBestMove }, savedGame)
  }, [mode, humanSide, thinkTime, stockfishElo, stockfishThinkTime, stockfishSide, flipped, showBestMove, history, rootFen])

  useEffect(() => {
    if (dialog === 'settings') settingsDialogRef.current?.showModal()
  }, [dialog])

  useEffect(() => {
    // A terminal position has no useful engine continuation. Cancelling here
    // also prevents a final completed move from starting a redundant analysis.
    if (!isMojoReady) return
    if (gameOver) {
      cancel()
      return
    }
    start(fen, positionHistory, thinkTime, mojoToMove ? 'move' : 'analysis')
    return () => cancel()
  }, [cancel, fen, gameOver, isMojoReady, mojoToMove, positionHistory, start, thinkTime])

  useEffect(() => {
    if (!isStockfishReady) return
    if (stockfishToMove) startStockfish({ rootFen, moves: stockfishMoves, elo: stockfishElo, thinkTimeMs: stockfishThinkTime })
    else cancelStockfish()
    return () => cancelStockfish()
  }, [cancelStockfish, isStockfishReady, rootFen, startStockfish, stockfishElo, stockfishMoves, stockfishThinkTime, stockfishToMove])

  const newGame = useCallback(() => { cancel(); cancelStockfish(); game.current = new Chess(); setRootFen(initialFen); setRunning(false); sync() }, [cancel, cancelStockfish, sync])
  const setMode = (value: EngineMode) => {
    cancel()
    cancelStockfish()
    setRunning(false)
    setModeState(value)
  }
  const play = (from: string, to: string, piece?: string) => {
    if (!humanCanMove) return false
    try {
      const pawn = game.current.get(from as Square)
      const promotion = pawn?.type === 'p' && (to.endsWith('1') || to.endsWith('8')) ? piece?.[1]?.toLowerCase() : undefined
      if (pawn?.type === 'p' && (to.endsWith('1') || to.endsWith('8')) && !promotion) return false
      game.current.move({ from, to, promotion: promotion as 'q' | 'r' | 'b' | 'n' | undefined })
      sync()
      return true
    } catch { return false }
  }
  const navigate = (ply: number) => {
    cancel()
    cancelStockfish()
    setRunning(false)
    const targetPly = Math.max(0, Math.min(history.length, ply))
    const position = new Chess(rootFen)
    history.slice(0, targetPly).forEach((move) => position.move({ from: move.from, to: move.to, promotion: move.promotion }))
    game.current = position
    setFen(position.fen())
    setViewPly(targetPly)
    setSelectedSquare(null)
  }
  const playAnalysisMove = (move: string) => {
    if (!analysis?.lines.some((line) => line.moves[0] === move) || analysis.root_fen !== fen || game.current.isGameOver()) return
    try {
      cancel()
      cancelStockfish()
      game.current.move(uciMove(move))
      sync()
    } catch { /* A stale analysis line is safely ignored. */ }
  }
  const loadFen = (value: string) => { try { game.current = new Chess(value.trim()); setRootFen(game.current.fen()); sync(); setDialog(null) } catch { window.alert('That FEN is not a legal standard-chess position.') } }
  const loadPgn = (value: string) => { try { const loaded = new Chess(); loaded.loadPgn(value); game.current = loaded; setRootFen(loaded.getHeaders().FEN ?? initialFen); sync(); setDialog(null) } catch { window.alert('That PGN could not be loaded.') } }
  const selectSquare = (square: Square) => {
    if (!humanCanMove) return
    const piece = game.current.get(square)
    if (piece && piece.color === game.current.turn()) setSelectedSquare(square)
  }
  const lastMove = viewPly > 0 ? history[viewPly - 1] : null
  const legalSquareStyles = useMemo(() => {
    const styles: Record<string, Record<string, string>> = {}
    if (lastMove) {
      styles[lastMove.from] = { backgroundColor: '#f6f66980' }
      styles[lastMove.to] = { backgroundColor: '#f6f66980' }
    }
    if (!selectedSquare || !humanCanMove) return styles
    styles[selectedSquare] = { backgroundColor: '#f6f669b0' }
    game.current.moves({ square: selectedSquare, verbose: true }).forEach((move) => {
      styles[move.to] = game.current.get(move.to) ? { boxShadow: 'inset 0 0 0 5px #f6f669b0' } : { background: 'radial-gradient(circle, #f6f669a8 0 19%, transparent 21%)' }
    })
    return styles
  }, [humanCanMove, lastMove, selectedSquare])
  const clickSquare = (square: Square) => {
    if (!selectedSquare || !humanCanMove) return selectSquare(square)
    const legalMove = game.current.moves({ square: selectedSquare, verbose: true }).find((move) => move.to === square)
    // Click-to-move has no promotion picker, so match the board library's
    // conventional default and promote to a queen.
    if (legalMove) play(selectedSquare, square, `${game.current.turn()}Q`)
    else selectSquare(square)
  }
  const arrows = useMemo<Array<[Square, Square, string]>>(() => {
    const move = bestMoveForPosition(analysis, fen)
    return showBestMove && move ? [[move.slice(0, 2) as Square, move.slice(2, 4) as Square, '#f4bd2e']] : []
  }, [analysis, fen, showBestMove])
  // Worker results are already normalized to White's perspective.
  const evalLine = analysis?.lines[0]

  return <main className="app"><header><div className="brand"><span className="brand__mark" aria-hidden="true">♞</span><div><h1>Mojo</h1><p>Browser chess engine</p></div></div><div className="header-actions"><div className="status" role="status" aria-live="polite"><i className={isReady && !error ? 'ready' : ''} aria-hidden="true" />{error ?? (gameOver ? 'Game over' : stockfishToMove || stockfishThinking ? 'Stockfish is thinking' : mojoToMove ? 'Mojo is thinking' : `${turn} to move`)}</div><button type="button" className="icon-button" onClick={() => setDialog('settings')} title="Settings" aria-label="Settings">⚙</button></div></header><div className="workspace"><div className="board-area"><EvaluationBar scoreCp={evalLine?.score_cp ?? null} mateIn={evalLine?.mate_in ?? null} result={gameResult} /><div className="board-shell" ref={boardShell}><Chessboard id="mojo-board" position={fen} boardOrientation={orientation} onPieceDrop={play} onPieceClick={(_, square) => selectSquare(square as Square)} onPieceDragBegin={(_, square) => selectSquare(square as Square)} onSquareClick={(square) => clickSquare(square as Square)} arePiecesDraggable={humanCanMove} autoPromoteToQueen={false} customArrows={arrows} boardWidth={boardWidth} customDarkSquareStyle={{ backgroundColor: '#779556' }} customLightSquareStyle={{ backgroundColor: '#ebecd0' }} customSquareStyles={legalSquareStyles as never} /></div></div><aside style={{ '--sidebar-height': `${sidebarHeight}px` } as CSSProperties}><AnalysisPanel analysis={analysis} onSelectMove={playAnalysisMove} /><MoveHistory history={history} currentPly={viewPly} onNavigate={navigate} /></aside></div>{dialog === 'settings' && <dialog ref={settingsDialogRef} className="modal modal--settings" aria-labelledby="settings-heading" onClose={() => setDialog(null)} onClick={(event) => { if (event.target === settingsDialogRef.current) closeSettings() }}><div className="modal__heading"><h2 id="settings-heading">Settings</h2><button type="button" onClick={closeSettings} aria-label="Close settings">×</button></div><SettingsPanel mode={mode} humanSide={humanSide} thinkTime={thinkTime} stockfishElo={stockfishElo} stockfishThinkTime={stockfishThinkTime} stockfishSide={stockfishSide} running={running} showBestMove={showBestMove} onMode={setMode} onSide={setHumanSide} onTime={setThinkTime} onStockfishElo={setStockfishElo} onStockfishTime={setStockfishThinkTime} onStockfishSide={setStockfishSide} onToggle={() => setRunning((value) => !value)} onFlip={() => setFlipped((value) => !value)} onShowBestMove={setShowBestMove} onReset={() => { newGame(); closeSettings() }} onFen={() => setDialog('fen')} onPgn={() => setDialog('pgn')} onExport={() => setDialog('export')} /></dialog>}{(dialog === 'fen' || dialog === 'pgn' || dialog === 'export') && <SetupDialog title={dialog === 'fen' ? 'Load FEN position' : dialog === 'pgn' ? 'Load PGN game' : 'Export PGN'} initialValue={dialog === 'fen' ? fen : game.current.pgn()} onClose={() => setDialog(null)} onSubmit={dialog === 'fen' ? loadFen : dialog === 'pgn' ? loadPgn : () => setDialog(null)} submitLabel={dialog === 'export' ? 'Close' : 'Load'} readOnly={dialog === 'export'} />}</main>
}

export default App
