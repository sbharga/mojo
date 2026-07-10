import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Chess, type Move, type Square } from 'chess.js'
import { Chessboard } from 'react-chessboard'
import { AnalysisPanel } from './components/AnalysisPanel'
import { EvaluationBar } from './components/EvaluationBar'
import { MoveHistory } from './components/MoveHistory'
import { SettingsPanel } from './components/SettingsPanel'
import { SetupDialog } from './components/SetupDialog'
import { useEngine } from './engine/useEngine'
import { bestMoveForPosition } from './engine/analysis'
import type { EngineMode, Side } from './engine/types'
import './styles.css'

const initialFen = new Chess().fen()
type Dialog = 'fen' | 'pgn' | 'export' | 'settings' | null

function uciMove(uci: string) {
  return { from: uci.slice(0, 2) as Square, to: uci.slice(2, 4) as Square, promotion: uci[4] as 'q' | 'r' | 'b' | 'n' | undefined }
}

function App() {
  const game = useRef(new Chess())
  const [fen, setFen] = useState(game.current.fen())
  const [rootFen, setRootFen] = useState(initialFen)
  const [history, setHistory] = useState<Move[]>([])
  const [mode, setModeState] = useState<EngineMode>('human-engine')
  const [humanSide, setHumanSide] = useState<Side>('white')
  const [thinkTime, setThinkTime] = useState(500)
  const [flipped, setFlipped] = useState(false)
  const [running, setRunning] = useState(false)
  const [dialog, setDialog] = useState<Dialog>(null)
  const [previewFen, setPreviewFen] = useState<string | null>(null)
  const [selectedSquare, setSelectedSquare] = useState<Square | null>(null)
  const boardShell = useRef<HTMLDivElement>(null)
  const [boardWidth, setBoardWidth] = useState(720)
  const sync = useCallback(() => { setFen(game.current.fen()); setHistory(game.current.history({ verbose: true })); setSelectedSquare(null) }, [])
  const applyEngineMove = useCallback((uci: string) => {
    if (game.current.isGameOver()) return
    try { game.current.move(uciMove(uci)); sync() } catch { /* A stale move is safely ignored. */ }
  }, [sync])
  const { analysis, isReady, error, start, cancel } = useEngine(applyEngineMove)

  const turn = game.current.turn() === 'w' ? 'white' : 'black'
  const gameOver = game.current.isGameOver()
  const positionHistory = useMemo(() => history.map((move) => move.before), [history])
  const engineToMove = !gameOver && ((mode === 'human-engine' && turn !== humanSide) || (mode === 'engine-engine' && running))
  const humanCanMove = !previewFen && !gameOver && (mode === 'human-human' || (mode === 'human-engine' && turn === humanSide))
  const orientation = flipped ? (humanSide === 'white' ? 'black' : 'white') : mode === 'human-engine' ? humanSide : 'white'
  // The board is square, so its rendered height equals boardWidth; the
  // evaluation bar enforces a 540px floor via its own min-height CSS.
  const sidebarHeight = Math.max(boardWidth, 540)

  useEffect(() => {
    const shell = boardShell.current
    if (!shell) return
    const updateWidth = () => setBoardWidth(Math.max(280, Math.floor(shell.clientWidth)))
    updateWidth()
    const observer = new ResizeObserver(updateWidth)
    observer.observe(shell)
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    const saved = localStorage.getItem('mojo-settings')
    if (!saved) return
    try {
      const value = JSON.parse(saved) as { mode?: EngineMode; humanSide?: Side; thinkTime?: number; flipped?: boolean }
      if (value.mode) setModeState(value.mode)
      if (value.humanSide) setHumanSide(value.humanSide)
      if (value.thinkTime) setThinkTime(value.thinkTime)
      if (value.flipped) setFlipped(value.flipped)
    } catch { /* Corrupt local settings should not prevent play. */ }
  }, [])

  useEffect(() => { localStorage.setItem('mojo-settings', JSON.stringify({ mode, humanSide, thinkTime, flipped })); localStorage.setItem('mojo-game', game.current.pgn()) }, [mode, humanSide, thinkTime, flipped, fen])

  useEffect(() => {
    if (!isReady || previewFen) return
    start(fen, positionHistory, thinkTime, engineToMove ? 'move' : 'analysis')
    return () => cancel()
  }, [cancel, engineToMove, fen, isReady, positionHistory, previewFen, start, thinkTime])

  const newGame = useCallback(() => { cancel(); game.current = new Chess(); setRootFen(initialFen); setPreviewFen(null); setRunning(false); sync() }, [cancel, sync])
  const setMode = (value: EngineMode) => { setModeState(value); newGame() }
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
  const undo = () => { cancel(); game.current.undo(); setPreviewFen(null); sync() }
  const review = (ply: number) => {
    const reviewed = new Chess(rootFen)
    history.slice(0, ply).forEach((move) => reviewed.move({ from: move.from, to: move.to, promotion: move.promotion }))
    setPreviewFen(reviewed.fen())
  }
  const playAnalysisMove = (move: string) => {
    if (!analysis?.lines.some((line) => line.moves[0] === move) || analysis.root_fen !== fen || previewFen || game.current.isGameOver()) return
    try {
      cancel()
      game.current.move(uciMove(move))
      sync()
    } catch { /* A stale analysis line is safely ignored. */ }
  }
  const loadFen = (value: string) => { try { game.current = new Chess(value.trim()); setRootFen(game.current.fen()); setPreviewFen(null); sync(); setDialog(null) } catch { window.alert('That FEN is not a legal standard-chess position.') } }
  const loadPgn = (value: string) => { try { const loaded = new Chess(); loaded.loadPgn(value); game.current = loaded; setRootFen(loaded.getHeaders().FEN ?? initialFen); setPreviewFen(null); sync(); setDialog(null) } catch { window.alert('That PGN could not be loaded.') } }
  const selectSquare = (square: Square) => {
    if (!humanCanMove) return
    const piece = game.current.get(square)
    if (piece && piece.color === game.current.turn()) setSelectedSquare(square)
  }
  const legalSquareStyles = useMemo(() => {
    const styles: Record<string, Record<string, string>> = {}
    if (!selectedSquare || !humanCanMove) return styles
    styles[selectedSquare] = { backgroundColor: '#f6f669b0' }
    game.current.moves({ square: selectedSquare, verbose: true }).forEach((move) => {
      styles[move.to] = game.current.get(move.to) ? { boxShadow: 'inset 0 0 0 5px #f6f669b0' } : { background: 'radial-gradient(circle, #f6f669a8 0 19%, transparent 21%)' }
    })
    return styles
  }, [humanCanMove, selectedSquare])
  const clickSquare = (square: Square) => {
    if (!selectedSquare || !humanCanMove) return selectSquare(square)
    const legalMove = game.current.moves({ square: selectedSquare, verbose: true }).find((move) => move.to === square)
    if (legalMove && !legalMove.promotion) play(selectedSquare, square)
    else selectSquare(square)
  }
  const whiteAnalysis = useMemo(() => {
    if (!analysis || analysis.root_fen.split(' ')[1] === 'w') return analysis
    return { ...analysis, lines: analysis.lines.map((line) => ({ ...line, score_cp: line.score_cp === undefined ? undefined : -line.score_cp, mate_in: line.mate_in === undefined ? undefined : -line.mate_in })) }
  }, [analysis])
  const arrows = useMemo<Array<[Square, Square, string]>>(() => {
    const move = bestMoveForPosition(analysis, fen)
    return move ? [[move.slice(0, 2) as Square, move.slice(2, 4) as Square, '#f4bd2e']] : []
  }, [analysis, fen])
  const evalLine = whiteAnalysis?.lines[0]

  return <main className="app"><header><div className="brand"><span className="brand__mark">♞</span><div><h1>Mojo</h1><p>Browser chess engine</p></div></div><div className="header-actions"><div className="status"><i className={isReady ? 'ready' : ''} />{error ?? (gameOver ? 'Game over' : engineToMove ? 'Mojo is thinking' : `${turn} to move`)}</div><button className="icon-button" onClick={() => setDialog('settings')} title="Settings" aria-label="Settings">⚙</button></div></header><div className="workspace"><div className="board-area"><EvaluationBar scoreCp={evalLine?.score_cp ?? null} mateIn={evalLine?.mate_in ?? null} /><div className="board-shell" ref={boardShell}>{previewFen && <button className="preview-banner" onClick={() => setPreviewFen(null)}>Viewing variation — return to live game</button>}<Chessboard id="mojo-board" position={previewFen ?? fen} boardOrientation={orientation} onPieceDrop={play} onPieceClick={(_, square) => selectSquare(square as Square)} onPieceDragBegin={(_, square) => selectSquare(square as Square)} onSquareClick={(square) => clickSquare(square as Square)} arePiecesDraggable={humanCanMove} autoPromoteToQueen={false} customArrows={arrows} boardWidth={boardWidth} customDarkSquareStyle={{ backgroundColor: '#779556' }} customLightSquareStyle={{ backgroundColor: '#ebecd0' }} customSquareStyles={legalSquareStyles as never} /></div></div><aside style={{ height: sidebarHeight }}><AnalysisPanel analysis={analysis} onSelectMove={playAnalysisMove} /><MoveHistory history={history} onReset={newGame} onUndo={undo} onReview={review} disabled={Boolean(previewFen)} /></aside></div>{dialog === 'settings' && <div className="modal-backdrop" role="presentation" onClick={() => setDialog(null)}><div className="modal modal--settings" onClick={(event) => event.stopPropagation()}><div className="modal__heading"><h2>Settings</h2><button type="button" onClick={() => setDialog(null)}>×</button></div><SettingsPanel mode={mode} humanSide={humanSide} thinkTime={thinkTime} running={running} onMode={setMode} onSide={(side) => { setHumanSide(side); newGame() }} onTime={setThinkTime} onToggle={() => setRunning((value) => !value)} onFlip={() => setFlipped((value) => !value)} onFen={() => setDialog('fen')} onPgn={() => setDialog('pgn')} onExport={() => setDialog('export')} /></div></div>}{(dialog === 'fen' || dialog === 'pgn' || dialog === 'export') && <SetupDialog title={dialog === 'fen' ? 'Load FEN position' : dialog === 'pgn' ? 'Load PGN game' : 'Export PGN'} initialValue={dialog === 'fen' ? fen : game.current.pgn()} onClose={() => setDialog(null)} onSubmit={dialog === 'fen' ? loadFen : dialog === 'pgn' ? loadPgn : () => setDialog(null)} />}</main>
}

export default App
