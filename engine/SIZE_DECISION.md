# Wasm size optimization measurements

Measured 2026-07-14 after roadmap item 27. All byte counts are release Wasm;
Brotli uses quality 11.

| Build | Baseline raw | Baseline gzip | Baseline Brotli | SIMD raw | SIMD gzip | SIMD Brotli |
|---|---:|---:|---:|---:|---:|---:|
| Reference Rust `-O` / wasm-opt `-O` | 1,453,959 | 219,858 | 129,411 | 1,455,071 | 220,010 | 129,730 |
| Rust `opt-level=z` | 1,427,392 | 217,465 | 124,351 | 1,427,210 | 217,363 | 124,134 |
| Rust `opt-level=s` | 1,431,385 | 218,828 | 125,703 | 1,431,234 | 218,809 | 125,659 |
| Accepted `-O`, stripped metadata, unused exports removed | 1,450,608 | 219,280 | 128,908 | 1,451,690 | 219,429 | 129,175 |

Rust `z` was rejected: its 1.8% raw / 1.1% gzip reduction repeatedly lost a
completed depth in the 100–1,000 ms benchmark and reduced measured throughput
roughly 20–35%. Rust `s` saved still less (1.6% raw / 0.5% gzip) while also
regressing completed depth in multiple cases. A direct wasm-opt `-Oz` build on
normal Rust codegen increased raw and gzip size, saving only 65–131 Brotli
bytes, so wasm-opt remains `-O`.

The accepted change removes repository-unused stateless `analyze_step`,
`fallback_move`, and `engine_name` exports, strips producer/target-feature
metadata after optimization, and keeps the speed-oriented Rust profile. Both
artifacts pass their tactical suites; a 20 ms paired smoke match against the
reference scored 1–4–1. The benchmark now reports Brotli, CI enforces a 225 KB
gzip ceiling per artifact, and CI uploads `twiggy top` output for comparisons.
