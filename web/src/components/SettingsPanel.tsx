import type { EngineMode, Side } from '../engine/types'

interface Props { mode: EngineMode; humanSide: Side; thinkTime: number; running: boolean; showBestMove: boolean; onMode: (value: EngineMode) => void; onSide: (value: Side) => void; onTime: (value: number) => void; onToggle: () => void; onFlip: () => void; onShowBestMove: (value: boolean) => void; onReset: () => void; onFen: () => void; onPgn: () => void; onExport: () => void }

const MIN_THINK_TIME_MS = 100
const MAX_THINK_TIME_MS = 10_000
const THINK_TIME_STEP_MS = 100

function formatThinkTime(ms: number) {
  return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${ms} ms`
}

export function SettingsPanel(props: Props) {
  return <div className="settings-panel"><label>Mode<select value={props.mode} onChange={(event) => props.onMode(event.target.value as EngineMode)}><option value="human-engine">You vs Mojo</option><option value="engine-engine">Mojo vs Mojo</option><option value="human-human">You vs You</option></select></label>{props.mode === 'human-engine' && <label>Your color<select value={props.humanSide} onChange={(event) => props.onSide(event.target.value as Side)}><option value="white">White</option><option value="black">Black</option></select></label>}<label>Engine time · {formatThinkTime(props.thinkTime)}<input type="range" min={MIN_THINK_TIME_MS} max={MAX_THINK_TIME_MS} step={THINK_TIME_STEP_MS} value={props.thinkTime} onChange={(event) => props.onTime(Number(event.target.value))} /></label><label className="toggle-control"><input type="checkbox" checked={props.showBestMove} onChange={(event) => props.onShowBestMove(event.target.checked)} /><span>Show best-move arrow</span></label><div className="control-grid">{props.mode === 'engine-engine' && <button type="button" className="primary" onClick={props.onToggle}>{props.running ? 'Pause game' : 'Start game'}</button>}<button type="button" onClick={props.onFlip}>Flip board</button><button type="button" onClick={props.onFen}>Load FEN</button><button type="button" onClick={props.onPgn}>Load PGN</button><button type="button" onClick={props.onExport}>Export PGN</button><button type="button" className="reset-game" onClick={props.onReset}>Reset game</button></div></div>
}
