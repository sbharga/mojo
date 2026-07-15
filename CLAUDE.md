# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Mojo is a Rust chess engine compiled to WebAssembly, plus a React web UI to play against it. Two top-level packages:

- `engine/` — Rust crate `mojo-engine`. Iterative-deepening alpha-beta search + handcrafted tapered evaluation, exposed to JS via `wasm-bindgen`. Also hosts offline tooling: Texel eval tuning, SPSA search tuning, opening-book generation, self-play/accuracy harnesses (all `.mjs` scripts + feature-gated Rust binaries).
- `web/` — React 19 + TypeScript + Vite SPA. Loads the engine Wasm in Web Workers and can also run bundled Stockfish as an opponent/reference. Deploys to GitHub Pages under base path `/mojo/`.

`no unsafe`: `unsafe_code` is `forbid`den crate-wide. Rust edition 2024.

## Commands

All `npm` scripts live in `web/package.json` and run from `web/` (CI uses `--prefix web`). Building the Wasm requires `wasm-pack` on PATH.

**Engine (Rust)** — from repo root, always pass the manifest path:
```
cargo test --manifest-path engine/Cargo.toml                 # unit tests
cargo test --manifest-path engine/Cargo.toml <name>          # single test by substring
cargo fmt --check --manifest-path engine/Cargo.toml
cargo clippy --manifest-path engine/Cargo.toml --all-targets -- -D warnings
```

**Web / build (from `web/`):**
```
npm run build:engine        # wasm-pack build → engine/pkg; also emits a +simd128 variant (build-wasm.mjs)
npm run dev                 # rebuild engine, then vite dev server
npm run build               # build:engine, tsc -b, vite build
npm test                    # vitest run (append a path/-t pattern for one test)
npm run lint                # eslint
```

**Full validation gate** (mirrors CI; run before finishing engine/UI changes):
```
npm run check   # lint + tsc + vitest + openings + strength + cancellation + seeding + size + selfplay
```

CI (`.github/workflows/ci.yml`) additionally runs `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `npm run build`, and a `twiggy` Wasm size profile. Push to `main` deploys to Pages via `deploy.yml`.

**Offline tuning/analysis pipelines** (not part of `check`, they alter tuned constants or measure strength):
```
npm run tune:eval       # Texel tuning → texel bin (feature "tuning")
npm run tune:search     # SPSA search-param tuning (spsa.mjs, feature "spsa")
npm run bench:engine    # bench.mjs
npm run selfplay        # selfplay.mjs
```

Cargo features gate the offline tooling and optional book: `tuning` (texel bin), `spsa`, `bookgen` (bookgen bin), `book` (bundle `book.bin` into the Wasm).

## Architecture

### Engine (`engine/src/`)
- `lib.rs` — the only Wasm surface. `Engine` struct wraps a persistent `SearchCore` (transposition table + heuristics survive across iterative depths and adjacent positions). Key methods: `set_position(fen, prior_fens)`, `analyze_depth`, stop-flag/stop-request wiring for cancellation. All inputs/outputs are plain data so the worker protocol stays independent of Rust internals.
- `search/mod.rs` — negamax + quiescence, TT lifecycle, and the search-tuning constants (aspiration windows, null-move, LMR, adaptive time-check interval). **Tuning constants here are intentionally frozen during refactors** — retuning is a strength decision that needs the self-play/SPRT-style harnesses, not a cleanup.
- `search/` submodules: `ordering` (move ordering / pickers), `see` (static exchange eval), `moves` (move/position utils, encode/decode, repetition keys), `tt`, `pawn_cache`, `correction`.
- `eval.rs` — handcrafted tapered evaluation (packed mid/endgame scores, pawn-structure cache, king-attack, mobility/threats, KPK bitbase from `kpk.rs`, fifty-move damping). `eval_tuned.rs` holds tuned parameter tables. `book.rs` is the optional opening book (feature `book`).
- `cozy-chess` provides board representation and legal move generation.

### Web engine integration (`web/src/engine/`)
- `worker.ts` — runs inside a Web Worker; `init`s the Wasm and drives one `Engine` instance. **Selects the `+simd128` Wasm variant when the browser passes a SIMD validation probe** (`wasmFeatures.ts`), else the baseline. Normalizes scores to White's perspective.
- `useEngine.ts` — spawns **two separate Workers/Engines, `move` and `analysis`**. They must not share one Engine: a single `analyze_depth` call is synchronous/uninterruptible, so a background analysis search could consume the whole time budget ahead of a queued move search. Separate threads keep "engine time is a per-move maximum" true.
- `stopSignal.ts` — `SharedArrayBuffer`-backed cancellation flag read mid-search (requires COOP/COEP headers, set in `vite.config.ts`).
- `analysisCache.ts` / `repetitionFingerprint.ts` — cache analysis keyed by position **including repetition history** (repetition changes the correct evaluation). `pvSeed.ts` seeds a move search from prior analysis PVs.
- `useStockfish.ts` / `stockfishClient.ts` — bundled Stockfish opponent; `scripts/prepare-stockfish.mjs` copies its assets into `public/` before dev/build.

### UI (`web/src/`)
`App.tsx` orchestrates game state (`chess.js`) and drives moves across five modes (human-engine, human-stockfish, engine-engine, mojo-stockfish, human-human). `appState.ts` reads/writes settings + session from `localStorage` **defensively** (clamps/validates every field so hand-edited storage can't break the UI). Components in `components/`.
