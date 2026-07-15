# Micro-NNUE decision

Status: **defer production NNUE; keep the tuned HCE as Mojo's evaluator.**

Decision date: 2026-07-14. Baseline: `36f8295` after evaluation roadmap
items 14–20.

## Why

The current evaluator now has a reproducible Texel pipeline, a pawn cache,
safe mobility and threats, nonlinear king safety, packed tapered scores,
50-move damping, and exact KPK knowledge. It remains deterministic,
explainable, and requires no separately trained artifact. Adding an untrained
or weakly trained network would increase size and maintenance cost without an
evidence-based strength benefit.

The measured release baseline from `node engine/bench.mjs` is:

| Metric | Current HCE build |
|---|---:|
| Raw Wasm | 1,449,073 bytes |
| Gzip Wasm | 217,739 bytes |
| Initial linear memory | 2,686,976 bytes |
| Peak benchmark memory | 5,439,488 bytes |

A dual-perspective i16 feature transformer adds approximately the following
high-entropy weight payload before biases and output weights:

| Candidate | Weight payload | Raw-build increase |
|---|---:|---:|
| `(768 → 32) × 2 → 1` | 49 KiB | 3.5% |
| `(768 → 64) × 2 → 1` | 97 KiB | 6.9% |
| `(768 → 128) × 2 → 1` | 194 KiB | 13.7% |

The compressed-size impact is proportionally more important because trained
weights compress poorly. Mojo should pay that permanent download cost only
after a representative net proves a material fixed-time gain.

## Adoption gates

Revisit this decision when all of these are available:

1. At least 500,000 deduplicated quiet positions from the reproducible
   self-play export, with source hashes and train/validation separation.
2. A checked-in training manifest containing architecture, feature mapping,
   quantization scales, dataset hashes, trainer version, and validation loss.
3. Incremental accumulator tests covering every move kind: normal moves,
   captures, en passant, promotion, and castling.
4. A Wasm benchmark showing no more than 10% node-rate regression at the
   100 ms browser preset and no deadline regression.
5. A paired fixed-time SPRT that accepts at least a +20 Elo alternative over
   the then-current HCE, plus unchanged tactical, perft, and draw correctness.
6. No more than 100 KiB added to the gzip release and a documented license for
   every training-data source and generated network artifact.

The first experiment should use the smallest dual-perspective network that can
clear these gates (`N = 32`), with the HCE retained behind the default build
path until the candidate is proven. A learned residual on top of the HCE is a
secondary experiment if a full replacement cannot justify its size.

## Reproduction

```bash
npm --prefix web run bench:engine
npm --prefix web run selfplay -- --baseline <hce-baseline.wasm> --move-time-ms 100
```

Record raw/gzip bytes, memory, completed depths, node rate, tactical results,
and paired SPRT output together. A lower validation loss alone is not a ship
criterion.
