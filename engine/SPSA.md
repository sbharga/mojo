# Search-parameter SPSA tuning

SPSA is offline tooling. The normal baseline and SIMD artifacts contain no
mutable search-parameter record or setter. Build the dedicated ABI first:

```sh
npm --prefix web run build:engine:spsa
```

Run a short smoke experiment:

```sh
npm --prefix web run tune:search -- --iterations 1 --openings 2 --depth 3
```

For a real run, increase iterations and paired openings substantially. The
driver deterministically perturbs all parameters at once, runs color-swapped
plus-versus-minus games through the existing self-play harness, applies a
decaying SPSA update, and checkpoints the latest JSON values after each
iteration. Options are `--iterations`, `--openings`, `--depth`, `--seed`,
`--wasm`, `--glue`, and `--output`.

| Parameter | Default | Min | Max |
|---|---:|---:|---:|
| `aspiration_initial_delta` | 20 | 5 | 100 |
| `rfp_margin_per_ply` | 120 | 40 | 240 |
| `futility_margin_base` | 100 | 20 | 240 |
| `futility_margin_per_ply` | 100 | 20 | 240 |
| `probcut_margin` | 180 | 60 | 320 |
| `delta_pruning_margin` | 120 | 40 | 240 |

The engine rejects unknown fields and out-of-range values before a match.
`selfplay.mjs` also accepts independent `--baseline-params` and
`--candidate-params`, a tuning `--glue` path, and `--json-output`.

SPSA output is a proposal, not an accepted engine change. Rebuild normal
artifacts with selected defaults and confirm the complete bundle using a
normal equal-time SPRT at the real 100–1,000 ms controls before merging it.
