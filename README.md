# Mojo

A chess engine written in Rust, compiled to WebAssembly, with a React web UI for
playing against it in the browser. The engine runs off the main thread in Web
Workers, and the app can also load a bundled Stockfish build as an opponent or
analysis reference.

**Play it:** https://sbharga.github.io/mojo/

## Features

- Iterative-deepening alpha-beta search with quiescence, a transposition table,
  aspiration windows, null-move pruning, and late-move reductions.
- Handcrafted tapered evaluation: packed midgame/endgame scores, pawn-structure
  cache, king-attack and mobility/threat terms, a KPK bitbase, and fifty-move
  damping.
- Feature-detected SIMD: ships both a baseline and a `+simd128` Wasm build and
  picks the faster one the browser supports at runtime.
- Separate `move` and `analysis` engine instances so background analysis never
  eats into a move's time budget.
- Five play modes: human vs. engine, human vs. Stockfish, engine vs. engine,
  Mojo vs. Stockfish, and human vs. human.
- Offline tooling for Texel evaluation tuning, SPSA search-parameter tuning,
  opening-book generation, and self-play/accuracy measurement.

`unsafe_code` is forbidden crate-wide (Rust edition 2024).

## Repository layout

- `engine/` — Rust crate `mojo-engine`. The search + evaluation core, exposed to
  JavaScript via `wasm-bindgen`, plus offline tuning/analysis scripts and
  feature-gated binaries.
- `web/` — React 19 + TypeScript + Vite single-page app. Loads the engine Wasm
  in Web Workers and deploys to GitHub Pages under the base path `/mojo/`.

## Getting started

Prerequisites: a Rust toolchain with the `wasm32-unknown-unknown` target,
[`wasm-pack`](https://rustwasm.github.io/wasm-pack/) on your `PATH`, and Node.js
24 (the version used by CI).

All web `npm` scripts run from `web/`. Use `npm ci` for a reproducible install;
use `npm install` only when intentionally updating dependencies.

```sh
cd web
npm ci
npm run dev      # builds the engine Wasm, then starts the Vite dev server
```

To produce a production build:

```sh
npm run build    # build:engine, tsc -b, vite build → web/dist
```

## Development

Rust checks run from the repo root with the manifest path:

```sh
cargo test --manifest-path engine/Cargo.toml
cargo fmt --check --manifest-path engine/Cargo.toml
cargo clippy --manifest-path engine/Cargo.toml --all-targets -- -D warnings
```

The full validation gate (mirrors CI) runs from `web/`:

```sh
npm run check    # lint + tsc + vitest + openings + strength + cancellation + seeding + size + selfplay
```

The app uses `SharedArrayBuffer` for prompt search cancellation. The Vite
configuration supplies the required cross-origin isolation headers locally;
deployments must preserve those headers to enable this fast cancellation path.
Without them, the engine remains functional and falls back to message-based
cancellation between iterative-deepening passes.

Offline pipelines (not part of `check`, since they alter tuned constants or
measure strength):

```sh
npm run tune:eval      # Texel evaluation tuning
npm run tune:search    # SPSA search-parameter tuning
npm run bench:engine   # benchmark
npm run selfplay       # self-play harness
```

See [CLAUDE.md](CLAUDE.md) for a deeper tour of the architecture.

## License

Mojo's own source is licensed under the [MIT License](LICENSE).

The app bundles the single-threaded Lite build of
[Stockfish.js](https://github.com/nmrugg/stockfish.js) (GPLv3) as an opponent and
reference; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). The opening data
in `engine/openings.json` is derived from a third-party dataset under its own
license (`engine/OPENINGS_LICENSE`).
