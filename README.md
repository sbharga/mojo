# Mojo

Mojo is a fast, browser-native chess engine and analysis board. The Rust engine
compiles to WebAssembly and runs in a Web Worker, so analysis never blocks the
React UI and no chess positions leave the browser.

Alongside games against Mojo, the browser app can play against a locally
bundled Stockfish 18 Lite opponent with configurable target Elo and move time,
or run Mojo-versus-Stockfish games. Mojo continues to power the analysis panel.

## Requirements

- Rust stable with `wasm32-unknown-unknown` (`rustup target add wasm32-unknown-unknown`)
- [`wasm-pack`](https://rustwasm.github.io/docs/wasm-pack/installer/)
- Node.js 22+

## Development

```bash
npm --prefix web install
npm --prefix web run dev
```

`dev` builds the Wasm package before starting Vite. Run `npm --prefix web run build`
for the deployable static site. `npm --prefix web run check` performs linting,
type checking, unit tests, ECO opening validation, tactical regression, and a
self-play smoke test; it does not rebuild Wasm first. Rust checks are available
through `cargo test --manifest-path engine/Cargo.toml`. Run
`npm --prefix web run bench:engine` to measure completed depth, deadline
behavior, Wasm size, and memory across the engine-time presets. Each benchmark
sample uses a fresh engine instance so transposition-table warming does not
skew comparisons between time budgets or MultiPV settings.

`npm --prefix web run test:strength` runs a deterministic tactical regression
suite against the generated Wasm engine. It uses fixed-depth, hand-verifiable
positions so search changes cannot silently lose basic tactical correctness.

## Strength testing

`engine/selfplay.mjs` compares two Wasm builds at a fixed iterative-search
depth or equal time per move. Every opening is played twice with candidate
colors swapped, and game history is passed back to each engine so repetition
and 50-move behavior match the browser. Results include candidate W/D/L,
paired scores, and a sequential probability ratio test (SPRT) for configurable
Elo hypotheses. Use fixed depth for deterministic evaluation changes and
`--move-time-ms` when a change affects search speed, matching browser play.

Preserve a baseline before rebuilding a candidate, then run:

```bash
cp engine/pkg/mojo_engine_bg.wasm /tmp/mojo-baseline.wasm
npm --prefix web run build:engine
npm --prefix web run selfplay -- --baseline /tmp/mojo-baseline.wasm --move-time-ms 100
```

The bundled ECO suite contains all 2,014 records converted from the
`chess-eco-codes` PGN: 2,010 unique complete FENs after transpositions are
deduplicated for self-play (see `engine/OPENINGS_LICENSE`). `--openings N` selects N
deterministic positions evenly across the ECO-sorted corpus for a broad short
sample. You can also pass another non-repeating JSON suite with `--openings-file`;
entries may use `{"name":"Position","fen":"..."}` or
`{"name":"Line","moves":["e2e4","e7e5"]}`. Paths are resolved from the
repository root. Choose the assumed `--draw-rate` from earlier matches and
supply enough independent opening pairs for the reported SPRT to reach a
boundary. A `continue testing` result is inconclusive. The baseline and
candidate must share the current generated JS ABI.

## Architecture

- `engine/` is the Rust search engine and Wasm boundary.
- `web/` owns rules/history through `chess.js`, the UI, persistence, and the
  worker protocol.
- `web/src/engine/worker.ts` owns one engine instance and cancels stale
  analysis by request id.
- Stockfish Lite runs as a separate UCI worker only in Stockfish game modes;
  its GPLv3 attribution and source information are in `THIRD_PARTY_NOTICES.md`.

Mojo is standard chess only in this release. It supports FEN and main-line PGN
import/export, local human games, and engine games. The current game and UI
settings are restored from browser storage when the page is reopened. Mojo is
not an online chess service or a chess.com clone.
