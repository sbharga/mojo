# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Mojo is a browser-native chess engine and analysis board. A Rust search engine
compiles to WebAssembly and runs inside a Web Worker, so analysis never blocks
the React UI and no chess positions leave the browser. Standard chess only —
no variants, no online play, not a chess.com clone.

The engine's design goal is to be extremely lightweight and lightning fast
while staying as accurate as possible — it has to search deeply inside a
browser tab's CPU and memory budget, not a server's. Prefer changes that keep
the Wasm binary small and the search fast per node over changes that add
weight for marginal accuracy; when a change trades one of these for another
(e.g. a pruning heuristic that risks tactical blindness, or a new dependency),
justify the trade explicitly and verify it with `test:strength` and
`selfplay` rather than intuition. Always follow the project's enforced best
practices (`clippy -D warnings`, `cargo fmt`, `forbid`-lint `unsafe_code`,
ESLint/TypeScript strictness) rather than working around them.

## Requirements

- Rust stable with the `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`)
- [`wasm-pack`](https://rustwasm.github.io/docs/wasm-pack/installer/)
- Node.js 22+

## Commands

All frontend commands run from `web/` (or via `--prefix web` from the repo root).

- `npm --prefix web run dev` — builds the Wasm package, then starts Vite.
- `npm --prefix web run build` — builds the Wasm package, type-checks (`tsc -b`), and produces the deployable static site.
- `npm --prefix web run check` — lint + typecheck + unit tests + all generated ECO FEN validations + deterministic Wasm tactical regressions + a neutral self-play smoke test. Run this before considering frontend or engine integration work done. It does not rebuild Wasm first.
- `npm --prefix web run test` — `vitest run` only.
- `npm --prefix web run lint` — `eslint .` only.
- `npm --prefix web run build:engine` — rebuilds `engine/pkg` from the Rust crate (`wasm-pack build --target web --out-dir pkg --release`). Re-run this after any change under `engine/src`; the web app imports the checked-in `engine/pkg` output, not the crate source, directly.
- `npm --prefix web run bench:engine` — rebuilds the engine, then runs `engine/bench.mjs`, which reports completed depth, deadline behavior, Wasm size, and memory across engine-time presets for a fixed set of positions.
- `npm --prefix web run test:strength` — runs `engine/accuracy.mjs` against the generated Wasm package. The fixed-depth positions cover basic mates, material wins, forks, and promotions; rebuild the engine first after Rust changes.
- `npm --prefix web run selfplay -- --baseline <path>` — compares the current candidate Wasm with an ABI-compatible baseline using paired, color-swapped games and reports W/D/L plus a paired-score SPRT. Fixed depth is deterministic; use `--move-time-ms` for performance-affecting changes so the match reflects browser play. By default it uses the 2,010 unique FENs generated from all 2,014 records in `engine/openings.json`; `--openings N` deterministically samples across the full ECO-sorted corpus, while `--openings-file` supplies another suite. Relative paths are resolved from the repository root.
- Add `--training-output <path>` to self-play to export white-result/FEN records for Texel tuning. `npm --prefix web run tune:eval -- <records.tsv>` filters quiet positions, deduplicates them, fits all evaluation weights, and emits the checked-in delta table. Validate the exact extractor with `cargo test --manifest-path engine/Cargo.toml --features tuning`.
- `node engine/generate-openings.mjs <eco.pgn> engine/openings.json` — reproducibly converts every PGN record to a full FEN with ECO metadata. `npm --prefix web run test:openings` validates all generated positions with both `chess.js` and the Wasm engine. Source and license: `engine/OPENINGS_LICENSE`.

Rust:

- `cargo test --manifest-path engine/Cargo.toml` — engine unit tests (perft, eval symmetry, search correctness, mate detection, repetition, TT/timeout behavior — see `engine/src/lib.rs` test module).
- `cargo clippy --manifest-path engine/Cargo.toml --all-targets -- -D warnings` and `cargo fmt --check --manifest-path engine/Cargo.toml` — both run in CI and must pass. `unsafe_code` is `forbid`-lint at the crate level.
- To run a single Rust test: `cargo test --manifest-path engine/Cargo.toml <test_name>`.

CI (`.github/workflows/ci.yml`) runs, in order: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `npm ci`, `wasm-pack` install, `npm run build`, `npm run check`. Match this sequence locally before pushing.

## Architecture

### Engine ↔ Web boundary

- `engine/` is the Rust search engine. `engine/src/lib.rs` is the `wasm-bindgen`
  surface: it exposes a reusable `Engine` struct (`set_position`,
  `analyze_depth`, `fallback_move`) plus stateless `analyze_step` /
  `fallback_move` free functions. All engine I/O is plain data (FENs, UCI move
  strings, plain result structs) — the JS/worker side never depends on Rust
  internals, and the Wasm boundary only understands serializable values.
- `engine/src/search.rs` holds `SearchCore`: iterative-deepening alpha-beta
  search with a fixed-size transposition table that survives across depths
  and adjacent positions (reused between `analyze_depth` calls on the same
  `Engine` instance), MultiPV support, late-move reduction, static exchange
  evaluation, repetition/50-move detection via position history, and a node
  limit for deterministic timeout testing.
- `engine/src/eval.rs` is the static evaluation function (material + terms)
  used by search and exposed for tests (e.g. color/turn symmetry).
  `engine/src/search/pawn_cache.rs` keeps a 128 KiB pawn-Zobrist cache of
  structure scores and passer bitboards; king-distance and rook-file terms
  remain position-dependent and are evaluated outside the cache.
- `engine/pkg/` is the **generated** `wasm-pack` output (JS glue + `.wasm`
  binary) that `web/` imports directly — it is committed and must be
  regenerated (`npm run build:engine`) after any `engine/src` change; it is
  not auto-rebuilt by `npm run check` or `npm test`.
- `engine/Cargo.toml`'s `[profile.release]` (`codegen-units = 1`, `lto =
  "fat"`, `panic = "abort"`, `strip = true`) plus `wasm-opt -O
  --enable-bulk-memory` on the `wasm-pack` output are what keep the binary
  small and fast in-browser; `bench:engine` reports the resulting Wasm size
  alongside search speed so regressions in either are visible together.

### Worker protocol (web/src/engine/)

- `web/src/engine/worker.ts` owns exactly one `Engine` Wasm instance inside a
  Web Worker. It initializes lazily on first `analyze` (or eagerly on an
  `initialize` message), then runs iterative deepening itself by calling
  `analyze_depth` in a loop up to depth 32, posting an `analysis` message per
  completed, non-timed-out iteration and a final `complete` message.
- Requests carry a monotonically increasing `requestId`; a `cancel` message
  records the highest cancelled id so in-flight/late results from stale
  requests are dropped without needing to kill the loop early. `useEngine.ts`
  (`web/src/engine/useEngine.ts`) bumps `requestId` and posts `cancel` for the
  previous one before posting each new `analyze`.
- `purpose` on a request is `'analysis'` (MultiPV 3, panel display) or
  `'move'` (MultiPV 1; on failure to find any completed line, falls back to
  `engine.fallback_move()` so the app never stalls with no move).
- `web/src/engine/types.ts` is the single source of truth for the message
  shapes (`WorkerRequest`/`WorkerMessage`/`Analysis`) shared between the main
  thread and the worker — keep both sides in sync through this file rather
  than duplicating shapes.
- `web/src/engine/analysis.ts` has small pure helpers (`isCurrentAnalysis`,
  `bestMoveForPosition`, `formatAnalysisScore`) used by the UI to avoid
  rendering analysis that no longer matches the current position (`root_fen`
  must match the live FEN).

### UI (web/src/)

- `web/src/App.tsx` is the single stateful component: it owns the `chess.js`
  `Chess` game instance (rules/history/PGN/FEN), drives `useEngine`, and wires
  up the board (`react-chessboard`), move history, analysis panel, and
  settings. Engine/human turn logic, preview-mode (reviewing past plies
  without mutating the live game), and localStorage persistence
  (`mojo-settings`, `mojo-game`) all live here.
- Components under `web/src/components/` (`AnalysisPanel`, `EvaluationBar`,
  `MoveHistory`, `SettingsPanel`, `SetupDialog`) are presentational — they
  receive state and callbacks from `App.tsx` rather than owning engine or
  game state themselves.
