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

Search uses a hard per-move deadline inside Rust and a dynamic soft deadline
between completed iterations. Stable best moves and concentrated root effort
can return early for responsiveness; best-move changes, score drops, and
scattered effort extend thinking toward the hard limit. Benchmark and timed
self-play loops consume the same Rust-provided policy as the browser worker.
The same callers use a smoothed effective branching factor to avoid starting a
deeper iteration predicted not to finish, with extra allowance for MultiPV and
an override when the best move or score is unstable.
After each completed depth, Rust also calibrates wall-clock polling from the
measured node rate to target roughly 1.5 ms between checks, bounded to
64–4,096 nodes; deterministic node-limited tests still check every node.

`npm --prefix web run test:strength` runs a deterministic tactical regression
suite against the generated Wasm engine. It uses fixed-depth, hand-verifiable
positions so search changes cannot silently lose basic tactical correctness.

The engine build emits baseline and `simd128` Wasm artifacts. The worker uses a
small `WebAssembly.validate` probe and loads SIMD only on supporting browsers;
the benchmark reports raw and gzip sizes for both builds.

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

For reproducible Texel tuning, add `--training-output /tmp/mojo-positions.tsv`
to a representative self-play match, then fit the evaluator's 822 linear
weights and emit a generated delta table:

```bash
npm --prefix web run tune:eval -- /tmp/mojo-positions.tsv
cargo test --manifest-path engine/Cargo.toml --features tuning
```

The fitter rejects tactical positions, deduplicates positions by Zobrist hash,
averages conflicting game results, reserves a deterministic validation split,
and records the input hash in the generated source.

The micro-NNUE tradeoff and the measurable gates for revisiting it are recorded
in [`engine/NNUE_DECISION.md`](engine/NNUE_DECISION.md). The production build
continues to use the HCE until a trained network clears those size, speed,
licensing, correctness, and fixed-time strength gates.

## Architecture

- `engine/` is the Rust search engine and Wasm boundary.
- `web/` owns rules/history through `chess.js`, the UI, persistence, and the
  worker protocol.
- `web/src/engine/worker.ts` owns one engine instance and cancels stale
  analysis by request id. On a cross-origin-isolated page, each worker also
  receives a SharedArrayBuffer cancellation watermark that Rust polls at its
  adaptive clock interval; other deployments retain between-depth fallback.
- When the analysis worker predicted the human's move, its completed primary
  PV is forwarded as a ponder seed for the resulting position. Rust validates
  the suffix and installs exact, decreasing-depth TT entries before the move
  worker searches, while a missed prediction is ignored.
- Stockfish Lite runs as a separate UCI worker only in Stockfish game modes;
  its GPLv3 attribution and source information are in `THIRD_PARTY_NOTICES.md`.

Mojo is standard chess only in this release. It supports FEN and main-line PGN
import/export, local human games, and engine games. The current game and UI
settings are restored from browser storage when the page is reopened. Mojo is
not an online chess service or a chess.com clone.

For prompt mid-search cancellation, production hosting must send
`Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: require-corp`. Vite development and preview
servers set both. GitHub Pages does not apply repository-defined response
headers, so the current Pages deployment intentionally uses the safe fallback.
