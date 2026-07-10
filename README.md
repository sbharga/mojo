# Mojo

Mojo is a fast, browser-native chess engine and analysis board. The Rust engine
compiles to WebAssembly and runs in a Web Worker, so analysis never blocks the
React UI and no chess positions leave the browser.

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
type checking, and unit tests. Rust checks are available through
`cargo test --manifest-path engine/Cargo.toml`. Run
`npm --prefix web run bench:engine` to measure completed depth, deadline
behavior, Wasm size, and memory across the engine-time presets.

## Architecture

- `engine/` is the Rust search engine and Wasm boundary.
- `web/` owns rules/history through `chess.js`, the UI, persistence, and the
  worker protocol.
- `web/src/engine/worker.ts` owns one engine instance and cancels stale
  analysis by request id.

Mojo is standard chess only in this release. It supports FEN and main-line PGN
import/export, local human games, and engine games. It is not an online chess
service or a chess.com clone.
