# Mojo Engine — Features

A thorough, feature-by-feature description of everything in `engine/`.

Mojo's engine is a compact, deterministic chess engine written in Rust
(edition 2024, `unsafe_code = forbid`), built as the crate `mojo-engine`. It
compiles to both a native `rlib` (for tests and offline tooling) and a
WebAssembly `cdylib` exposed to JavaScript through `wasm-bindgen`. The design
target is a three-way balance: **lightweight** (small Wasm binary),
**fast** (strong play in a tight per-move time budget), and **accurate**
(strength never silently regresses).

## Design goals

The engine should be **as lightweight and as fast as possible while staying
accurate**. These three goals pull against each other, and every change is
judged by that balance:

- **Lightweight** — small Wasm binary. CI enforces a `twiggy` size profile and
  `npm run check` includes a size gate. Prefer solutions that don't grow the
  binary; the opening book and SIMD variant are optional/feature-gated for this
  reason.
- **Fast** — strong play within a tight per-move time budget. Search efficiency
  (move ordering, pruning, TT reuse across positions) matters more than raw
  feature count.
- **Accurate** — strength must not regress. Evaluation/search-tuning changes
  are validated with the self-play / SPRT-style harnesses, never by eyeballing.

When a change trades one of these against another, that trade-off is meant to
be called out explicitly rather than silently favoring one axis.

## Release profile

The release profile is tuned for size and determinism: `codegen-units = 1`,
`lto = "fat"`, `panic = "abort"`, `strip = true`, plus a `wasm-opt` pass
(`-O`, bulk-memory, nontrapping float-to-int, SIMD, strip producers/target
features).

Cargo features gate all optional/offline surface area:

| Feature | Effect |
|---------|--------|
| *(default)* | Search + eval only; smallest binary |
| `book` | Bundles `book.bin` and enables `Engine::book_move` |
| `tuning` | Exposes `eval::tuning` module + builds the `texel` binary |
| `spsa` | Enables runtime-settable search parameters + `set_search_parameters` |
| `bookgen` | Builds the `bookgen` binary |

### Dependencies
`cozy-chess` 0.3.4 (board representation + legal move generation — provides
magic-bitboard attack tables and Zobrist hashing), `arrayvec` (stack move
lists), `wasm-bindgen` / `js-sys` / `serde` / `serde-wasm-bindgen` (JS
boundary). Dev-only: `pretty_assertions`. No search/eval logic is delegated to
a third-party engine library — everything below is first-party.

> A reference section at the end lists concrete metrics, the current frozen
> constant values, what currently exists and what does not, and the invariants
> the codebase maintains.

---

## 1. Wasm surface (`src/lib.rs`)

The only public API. Everything crosses the JS boundary as plain data so the
Web Worker protocol stays independent of Rust internals.

### `Engine` struct
Wraps a persistent `SearchCore` (transposition table + all heuristics survive
across iterative depths **and** adjacent positions) plus an optional current
`Board`.

Methods exposed to JS:

- **`new()` / `Default`** — constructs an engine; eagerly initializes the KPK
  bitbase and debug-asserts the TT memory layout (16-byte entries, 64-byte
  buckets, 2 MiB total).
- **`set_position(fen, prior_fens)`** — parses the root FEN and a JS array of
  prior game FENs (for repetition detection), forwarding both to the search
  core. Returns a JS error on any invalid FEN.
- **`analyze_depth(depth, multi_pv, time_limit_ms)`** — runs **one**
  iterative-deepening step (depth clamped to ≥ 1) while retaining earlier
  search state. Returns a serialized `AnalysisResult`. A single call is
  synchronous and uninterruptible except via the shared stop flag.
- **`set_stop_flag(Int32Array)`** *(wasm only)* — installs a
  `SharedArrayBuffer`-backed cancellation watermark polled mid-search.
- **`set_stop_request(request_id)`** *(wasm only)* — identifies which request
  the current search belongs to, so a stale watermark can't cancel a newer
  search.
- **`seed_pv(moves, depth, score_cp, mate_in)`** — seeds this instance's TT
  from another worker's principal variation. Validates that exactly one of
  centipawn/mate score is given, and that every PV move is legal; returns the
  count of entries installed.
- **`set_search_parameters(params)`** *(feature `spsa`)* — applies a bounded
  search-parameter record; rejects out-of-range values.
- **`fallback_move()`** — returns the best static one-ply move (used as a
  safety net when no search result is available).
- **`book_move(seed)`** *(feature `book`)* — returns a weighted random opening
  reply from the embedded book.

### `AnalysisResult` / `PrincipalVariation`
Serialized output includes: `depth`, `nodes`, `root_node_fraction`,
`soft_time_fraction`, `predicted_next_ms`, `ebf_gate_override`,
`clock_check_interval`, `elapsed_ms`, `timed_out`, and the `lines` (each a PV
with either a centipawn score or a `mate_in` count, plus the UCI move list).
These time-management fields let the JS driver decide whether to spend another
iteration.

### Helper functions
- **`uci_move`** — formats a move as UCI, translating cozy-chess's internal
  king-to-rook castling encoding into the standard king-destination form
  (e1g1 / e1c1).
- **`score_to_line`** — converts an internal score into either a centipawn
  value or a signed mate distance, and materializes the PV move list.
- **`seed_score`** — reconstructs an internal score from an external
  centipawn or mate-in value, with mate-distance encoding.
- **`now_ms`** — `performance.now()` on wasm, a monotonic `Instant`-based
  clock natively.

Extensive unit tests cover perft correctness (start/kiwipete/endgame),
evaluation symmetry, mate finding, fifty-move and threefold-repetition draws,
MultiPV distinctness, PV legality, timeout honesty, and UCI castling output.

---

## 2. Search core (`src/search/mod.rs`)

An iterative-deepening alpha-beta (negamax) search with a large, modern
pruning/reduction toolbox. All search-tuning constants live at the top of this
file and are **intentionally frozen during refactors** — changing them is a
strength decision that must be validated with the self-play/SPRT harness.

### Persistent state (`SearchCore`)
Survives across depths and adjacent positions:
- **Transposition table** — `Box<[TTBucket]>`, 2 MiB, 4-way buckets.
- **Killers** — two per ply.
- **History** — main `[64][64]` butterfly history.
- **Continuation history** — piece-to/piece-to indexed (`288 KiB`).
- **Capture history** — `12×64×6` indexed (`9 KiB`).
- **Correction history** — pawn + material tables (`128 KiB` combined).
- **Pawn cache** — direct-mapped pawn-structure cache (`128 KiB`).
- **Countermove** table, **PV** triangular table, **static-eval** stack,
  **root move statistics**, repetition **path**, prior-position keys, and a
  suite of time-management/EBF smoothing accumulators.

On a position change (`set_position`), if the root key changed, all history
tables are **halved** (aged, not cleared), killers/countermoves/root-stats are
reset, and the TT generation counter is bumped so old entries can be evicted.

### `analyze_depth` — one iterative step
- Resets node counters and the timeout flag; sets the deadline.
- Computes and stores a corrected static eval at the root.
- Returns immediately with empty lines if the root is a draw.
- Runs up to 5 MultiPV lines, each inside an **aspiration window** around the
  previous iteration's score, widening only the failed side on fail-high/low
  (up to 4 retries before falling back to a full `(-INF, INF)` window).
- After the search, updates **time management** (soft time fraction from
  best-move stability, root-node concentration, best-move changes, and score
  drops) and predicts the next iteration's cost via a **smoothed effective
  branching factor**.

### `search_root`
Drives root move ordering via `RootMovePicker`, uses principal-variation
search (full window on move 0, null-window probe + re-search otherwise),
records per-move subtree node counts (for time management), and stores the
primary line's result in the TT. Only the *unexcluded* primary search updates
root stats/TT so MultiPV lines can't displace the principal move.

### `negamax` — the main recursion
A dense implementation featuring, in order:
- **Node/time checks**, ply ceiling, draw and KPK-loss short-circuits.
- **Check extensions** (up to `MAX_CHECK_EXTENSIONS = 2`).
- **Mate-distance pruning** on alpha/beta.
- **TT cutoff** (non-PV, sufficient depth) with exact/lower/upper handling.
- **Internal iterative reductions (IIR)** when no TT move exists at depth.
- **Null-move pruning** with depth-scaled reduction, halfmove-clock and
  non-pawn-material guards, a zugzwang **verification search** at high depth,
  and threat extraction from the null refutation.
- **Reverse futility / static null-move pruning** (RFP), with an
  "improving"-aware margin.
- **Razoring** — drop straight to quiescence when far below alpha at shallow
  depth.
- **ProbCut** — try winning captures at reduced depth against a raised beta.
- **Singular extensions** — verify the TT move is uniquely best; may extend,
  trigger multi-cut, or apply a negative extension.
- **Main move loop** with staged move picking, and per-move shallow pruning:
  SEE pruning of losing captures, late-move pruning (LMP), history pruning,
  and futility pruning of quiets. Late moves get **late-move reductions (LMR)**
  from a precomputed log-scaled table, adjusted by PV/cut-node status, checks,
  TT-move-capture, history score, and the improving flag.
- **Cutoff bookkeeping** — records killers/history/countermove for quiets and
  capture history for captures; updates **correction history** from the search
  result.

### `quiescence`
Capture/check search with a TT probe, stand-pat cutoff, SEE-based pruning of
losing captures, and **delta pruning** (skips captures that can't reach alpha
even with the captured value + margin — but correctly accounts for promotions).

### Supporting logic
- **`is_draw`** — halfmove ≥ 100, insufficient material, or threefold
  repetition. Repetition counts both real game history and the in-search path,
  but a **null-move boundary** excludes real history and pre-null path nodes
  from any repetition claim inside a synthetic null subtree.
- **`expired`** — node-count-gated polling of the deadline and the shared stop
  flag (comparing the atomic watermark against the current request id).
- **Clock-check calibration** — dynamically retunes the node interval between
  time checks to target ~1.5 ms of wall time per check, bounded to
  `[64, 4096]`.
- **`store`** — depth-preferring replacement within a bucket, falling back to
  the oldest/shallowest entry by generation age.
- **`build_lmr_table`** / **`log_scaled`** — `const fn` construction of the
  32×64 reduction table from a fixed-point natural-log approximation.

### `SearchParameters` (feature `spsa`)
A validated, bounded record of six tunable margins (aspiration delta, RFP,
futility base/per-ply, probcut, delta-pruning). When the feature is off, each
accessor returns the frozen constant; when on, it returns the runtime value.
This is how the SPSA pipeline perturbs parameters without recompiling.

The file carries a large in-module test suite covering LMR math, RFP/LMP
margins, singular/IIR/probcut/null-verification gating, aspiration widening,
the null-move repetition boundary, stalemate detection under pruning, time
management, EBF prediction, clock calibration, and PV seeding.

---

## 3. Move ordering (`src/search/ordering.rs`)

Staged, incremental move selection plus the cutoff bookkeeping.

- **`RootMovePicker`** — orders root moves by a tuple key: previously-seen
  flag, previous score, previous subtree node count, then a static fallback
  (TT move → tactical SEE score → history). This makes iterative deepening
  re-examine the most promising and most-worked root moves first.
- **`MovePicker`** — staged generator: **TT move → non-losing tactical
  (captures with SEE ≥ 0 plus promotions) → special (killers, countermove,
  threat move) → losing captures → quiet (history-scored) → done**. Losing
  captures are held in a compact fixed-capacity move list, so their SEE is not
  recomputed and they do not require a second scored-move array. Selection is
  incremental (best-of-remaining scan), and every legal move is returned
  exactly once (verified by test).
- **`QuiescencePicker`** — generates captures/promotions (or all evasions when
  in check), scoring each by SEE class + capture history + captured value, and
  yields `(move, see)` pairs.
- **History updates** — `update_history` uses a saturating,
  magnitude-damped bonus (`bonus - value·|bonus|/LIMIT`) clamped to
  `±16384`; `record_quiet_cutoff` updates killers/main+continuation history/
  countermove; `record_capture_cutoff` updates capture history. Failed
  siblings receive the negated bonus (malus).
- **`continuation_index`** — recovers the moved piece even for castling
  (encoded as king-to-rook) so the continuation sample isn't dropped.

Tests verify countermove population, TT-first ordering, complete legal-move
coverage, threat ordering, root ordering, and the exact memory footprints of
the continuation (288 KiB) and capture (9 KiB) history tables, including
their halving on position change.

---

## 4. Static exchange evaluation (`src/search/see.rs`)

`static_exchange` computes the material outcome of a capture sequence on the
target square using the classic swap-list algorithm: it repeatedly finds the
least-valuable attacker for the side to move (via `attackers_to`, which
recomputes x-ray attackers as pieces are removed from the occupancy), builds a
gain array, then folds it back with negamax minimaxing. Handles en-passant
(removes the captured pawn from occupancy) and promotions (adds the promotion
gain). Used for capture ordering, capture pruning, and quiescence filtering.

---

## 5. Move/position utilities (`src/search/moves.rs`)

- **`legal_moves` / `tactical_moves` / `quiet_moves`** — full and partitioned
  legal-move generation (tactical = captures + promotions incl. en passant;
  quiet = the complement, promotions excluded). A test confirms the two
  staged sets partition the full legal set.
- **`played`** — clone-and-play helper.
- **`fallback`** — the one-ply static "best move" used when search produced
  nothing, correctly scoring immediate mates/draws.
- **`is_capture` / `captured_value`** — capture detection (including en
  passant) that correctly treats castling as *quiet*, and per-piece captured
  value.
- **`repetition_key`** — a Zobrist-style key that includes the en-passant file
  **only when an en-passant capture is actually legal** (so otherwise-identical
  positions hash together for repetition purposes).
- **`rule_key`** — the repetition key mixed with the halfmove clock, used to
  key the TT so fifty-move-relevant positions stay distinct.
- **`encode_move` / `decode_move`** — 16-bit packed move encoding
  (from | to<<6 | promotion<<12) used throughout the TT, PV, and history
  tables.

---

## 6. Transposition table (`src/search/tt.rs`)

- **`TTEntry`** — a `#[repr(C)]` 16-byte record: 64-bit key, 16-bit best move,
  16-bit score, 16-bit static eval (with a sentinel for "none"), 8-bit depth,
  and an 8-bit metadata byte packing a 2-bit `Bound` (Empty/Exact/Lower/Upper)
  and a 6-bit generation.
- **`TTBucket`** — 4 entries, `align(64)` (one cache line). `2^15` buckets ⇒
  `2^17` entries ⇒ exactly 2 MiB.
- **`score_to_tt` / `score_from_tt`** — adjust mate scores by ply on
  store/probe so mate distances stay correct regardless of where in the tree
  a position is found.

Tests assert the compact layout, depth-preferring replacement, and that
colliding keys coexist within a bucket.

---

## 7. Pawn cache (`src/search/pawn_cache.rs`)

A `4096`-entry direct-mapped cache (128 KiB) keyed by a pawn-only Zobrist
hash. `raw_evaluate` looks up the cached `PawnStructure` (doubled/isolated/
passed terms + passer bitboards) and only recomputes it on a miss, then runs
the full `evaluate_with_pawns`. A test confirms that king-distance passer
terms are still computed against the *current* position even on a cache hit
(the cached structure carries only pawn-derived data).

---

## 8. Correction history (`src/search/correction.rs`)

Learns a bounded, repeatable residual to remove systematic bias from the
handcrafted static eval. Two tables — **pawn-structure-indexed** and
**material-signature-indexed**, each per-side (`128 KiB` total) — are updated
after a search from the gap between the search score and the raw eval, but
**only** when the bound makes that gap trustworthy (exact; upper below the
corrected eval; lower above it, excluding capture fail-highs). `corrected_
static_eval` adds the averaged, clamped (`±32 cp`) correction to the raw eval.
Depth-weighted updates converge toward the observed residual. Tests confirm
the 128 KiB footprint, bounded convergence, and that capture fail-highs don't
train the tables.

---

## 9. Evaluation (`src/eval.rs`)

A handcrafted **tapered** evaluation, PeSTO-style, with a packed
midgame/endgame score (`eg<<16 + mg`) interpolated by a 0–24 game phase.

Terms, all individually tunable via the generated delta table:
- **Material** — tapered piece values (mid/endgame).
- **Piece-square tables** — separate mid/endgame PSTs, 6 pieces × 64 squares.
- **Mobility** — safe reachable squares per sliding/leaping piece
  (excluding own pieces and squares attacked by enemy pawns).
- **King safety** — a nonlinear king-attack curve (32 entries) driven by
  accumulated attack units, attacker count, undefended king-zone squares, and
  reduced by the pawn shield.
- **Threats** — pawn threats, minor-on-major threats, and hanging (undefended
  attacked) pieces, each with mid/endgame weights.
- **Pawn structure** — doubled and isolated penalties; passed-pawn bonuses by
  relative rank, plus an endgame king-distance term (own king escorts /
  enemy king races).
- **Piece-specific** — bishop pair, rook on open/semi-open file.
- **Bare-king mating guidance** — when one side has only a king, rewards
  driving the enemy king to an edge (and, for bishop+knight, to the correct-
  colored corner), plus rook/queen "confinement" boxing and attacking-king
  proximity. This gives shallow searches a usable gradient in won endings.
- **Tempo** bonus for the side to move.
- **KPK adjustment** — probes the bitbase: forced draws score exactly 0; won
  KPK positions get a large win score.
- **Fifty-move damping** — the whole eval is scaled down linearly as the
  halfmove clock approaches 100, so the engine avoids shuffling toward a draw.

Two **separate** piece-value scales exist by design: the tapered `MG/EG_VALUE`
used only by `evaluate`, and a flat `piece_value` used by SEE / move ordering /
capture pruning (a single canonical scale for exchange comparison).
`insufficient_material` recognizes dead positions (bare kings, K+minor, and
same-colored bishops) while correctly *not* declaring opposite-colored bishops
or two-knight positions dead.

### `tuning` submodule (feature `tuning`)
Exposes the evaluation as **855 linear parameters** for Texel tuning.
`extract` produces `LinearFeatures` (separate mg/eg/direct coefficient vectors
+ phase, perspective, rule scale, offset, forced-draw flag) that reproduce the
production integer eval **exactly** (asserted by test) while also supporting
`f64` value + gradient computation. `base_weights`/`current_weights` assemble
the parameter vector from the hand-authored constants plus the checked-in
deltas; `generated_source` emits the `eval_tuned.rs` delta file (with a source
hash); `is_quiet` filters out checks and positions with an immediate capture/
promotion for the training set.

---

## 10. Tuned parameters (`src/eval_tuned.rs`)

Generated file holding `SOURCE_HASH` and `DELTAS: [i16; 855]`. The checked-in
version is **all zeros**, deliberately preserving the documented base evaluator
until a licensed training corpus is fitted and validated. `eval::tuned(base,
i)` adds `DELTAS[i]` to each base weight at `const` evaluation time, so with
zero deltas there is no runtime cost and the tables compile to the base values.

---

## 11. KPK bitbase (`src/kpk.rs`)

A compute-at-initialization king-and-pawn-vs-king bitbase. Exploiting symmetry
(pawn restricted to files a–d, ranks 2–7), the full state space is
`2 × 24 × 64 × 64` states packed to a **24 KiB** bit array. It's generated by
retrograde fixpoint iteration: white-to-move states are winning if any legal
king/pawn move reaches a known win (including promotion-to-8th logic); black-
to-move states are winning only if *every* legal defence leads to a win (with
correct handling of pawn capture, stalemate vs. mate). `probe` normalizes an
arbitrary board (orienting for pawn color, mirroring files) and returns
`Some(true)` (pawn side wins), `Some(false)` (draw), or `None` (not exact KPK
material). Tests cover the rook-pawn stalemate draw, black-pawn/both-wing
normalization, and an exact-0 search result for a nonterminal fortress.

---

## 12. Opening book (`src/book.rs`, feature `book`)

Embeds `book.bin` (a `MOJOBK01`-magic, 48-byte header + 12-byte sorted records
of key/move/weight). `book_move` binary-searches the current position's Zobrist
hash, then does weighted random selection among the replies using a seeded
mixing hash, returning a reply only if it's still legal. A validation test
cross-checks every record against `book-validation.tsv` and confirms the book
stays within an 8–32 KiB size band and that weighted seeds actually vary the
opening.

---

## 13. Offline binaries (`src/bin/`)

- **`texel.rs`** (feature `tuning`) — the Texel tuner. Loads a TSV of
  `<result>\t<FEN>` rows, dedupes by Zobrist hash (averaging duplicate
  results), keeps only quiet positions, and deterministically splits ~10% into
  a validation set. Runs full-batch gradient descent on logistic loss with L2
  regularization toward the initial weights, logs train/validation loss, and
  writes a new `eval_tuned.rs` atomically. Uses an FNV-1a hash of the input to
  stamp provenance.
- **`bookgen.rs`** (feature `bookgen`) — builds `book.bin` and a validation
  TSV from a text file of opening lines. Accumulates per-position reply
  weights, ranks positions by total weight, keeps the top ≤3 replies per
  position up to a record cap, and writes the sorted binary with a source-hash
  header.

---

## 14. Node/JavaScript tooling (`*.mjs`)

These are offline scripts (run via `npm --prefix web run …`), not part of the
shipped engine, split into **build**, **validation gates** (part of
`npm run check` / CI), and **strength/tuning pipelines**.

### Build
- **`build-wasm.mjs`** — invokes `wasm-pack build` twice: a baseline build and
  a `+simd128` build, copying the SIMD `.wasm` alongside the baseline as
  `mojo_engine_simd_bg.wasm`. The browser worker picks the variant by SIMD
  probe.

### Validation gates
- **`validate-size.mjs`** — enforces the gzip size budget (230 KB) for both
  the baseline and SIMD Wasm; reports raw/gzip/brotli sizes.
- **`validate-cancellation.mjs`** — asserts a stale stop watermark is ignored
  and a current watermark stops the search promptly (within ~2 clock-check
  intervals).
- **`validate-seeding.mjs`** — confirms `seed_pv` warms the TT so a seeded
  search preserves the principal move and uses strictly fewer nodes than a cold
  search.
- **`validate-openings.mjs`** — validates `openings.json` (record count,
  source hash, ECO metadata, canonical FENs, unique-FEN count) and that the
  engine accepts every position.
- **`validate-book.mjs`** *(needs the `book` build)* — validates
  `book-validation.tsv` records against chess.js legality and the engine's
  book replies.
- **`accuracy.mjs`** — a fixed-depth, hand-checkable regression suite of
  tactical/mate positions; a search-correctness gate rather than an Elo
  estimate.

### Strength / tuning pipelines
- **`bench.mjs`** — per-position node/time benchmark across start/tactical/
  middlegame/endgame FENs, isolating each sample in a fresh engine and matching
  the browser time-management path. `--fixed-depth N` reports deterministic
  cumulative iterative-deepening nodes, and `--wasm PATH` benchmarks a saved
  baseline artifact through the current ABI.
- **`selfplay.mjs`** — SPRT-style self-play harness (baseline vs. candidate
  Wasm) over an opening set, with configurable Elo bounds, alpha/beta, adjudi-
  cation (win/draw score thresholds), and optional training-data / JSON output.
- **`spsa.mjs`** — SPSA search-parameter tuning: perturbs the six bounded
  `SearchParameters` (with per-parameter `c`/`a` step specs), plays gauntlets
  via the `pkg-spsa` build (which enables `set_search_parameters`). Its default
  run is 10 iterations of 16 paired openings at 100 ms/move, with its
  intermediate parameter file written outside the worktree. Candidates are
  frozen into the shipping constants only after a color-swapped 128-opening
  100 ms/move SPRT accepts +10 Elo at `alpha = beta = 0.05`; `--depth` remains
  available as an explicit fixed-depth alternative.
- **`generate-openings.mjs`** — regenerates `openings.json` from an ECO PGN
  (with a source-hash guard).
- **`generate-book.mjs`** — converts an ECO PGN into opening lines, verifies
  the source hash matches `openings.json`, and drives the `bookgen` binary.

---

## 15. Data & license files

- **`book.bin`** — the embedded opening book (feature `book`).
- **`book-validation.tsv`** — golden records asserting book integrity.
- **`openings.json`** — 2014 ECO opening positions (2010 unique FENs) with
  metadata and a source hash; drives the UI's opening naming and validation.
- **`pkg-spsa/`** — a prebuilt SPSA-featured Wasm package used by `spsa.mjs`.
- **`LICENSE`** (MIT) and **`OPENINGS_LICENSE`** (attribution for the ECO
  source, kevinludwig/chess-eco-codes).

---

## Cross-cutting design themes

- **Determinism** — fixed table sizes, seeded randomness, `panic = abort`, and
  reproducible builds make results repeatable in CI.
- **Persistence over recomputation** — TT, history, pawn cache, correction
  history, and root stats all survive across positions; position changes *age*
  rather than clear them.
- **Two-worker safety** — a single `analyze_depth` call is synchronous and
  uninterruptible, so the web layer runs separate move/analysis engines;
  cancellation is the only mid-search interruption, via the shared atomic flag.
- **Size discipline** — optional features (book, SIMD, tuning) are gated out of
  the default build, and a gzip size gate guards every change.
- **Frozen tuning constants** — search and eval constants are treated as
  strength decisions, changed only through the self-play / Texel / SPSA
  pipelines, never as part of a refactor.

---

## Reference: metrics, constants, current state, constraints

Factual reference data — sizes, constant values, what currently exists and
what does not, and the invariants any change must preserve.

### Key sizes and memory footprint

| Structure | Size | Notes |
|-----------|------|-------|
| Transposition table | **2 MiB** | 2^15 buckets × 4 entries × 16 B; cache-line aligned |
| Continuation history | 288 KiB | `(6·64)²` × i16 |
| Correction history | 128 KiB | pawn + material, per-side, i16 |
| Pawn cache | 128 KiB | 4096 entries, direct-mapped |
| Capture history | 9 KiB | 12·64·6 × i16 |
| KPK bitbase | 24 KiB | computed at init, not embedded |
| LMR table | 2 KiB | 32×64 u8, `const fn` built |
| Killers / static evals / PV | small | bounded by `MAX_PLY = 64` |
| **Wasm gzip budget** | **≤ 230 KB** | enforced for baseline *and* SIMD in `validate-size.mjs` |

`MAX_PLY = 64`, `MAX_MOVES = 218`, `MATE_SCORE = 30_000`, `INF = 32_000`.

### Search tuning constants (current frozen values)

Aspiration: initial delta **20**, max **4** retries, doubling on each widen.
Null move: base reduction **2** + depth/**4**, min depth **3**, verification
from depth **10**, disabled when halfmove clock ≥ **90** or no non-pawn
material. LMR: from depth **3** / move index **3**. History pruning: depth ≤
**3**, move index ≥ **4**, threshold **−4000** (good = **+4000**). RFP: depth ≤
**8**, margin **120**/ply (−**40** when not improving). Razoring: depth ≤ **2**,
base **200** + **250**/ply. SEE pruning (main search): depth ≤ **8**, −**90**/ply.
Futility: depth ≤ **3**, base **100** + **100**/ply. LMP: depth ≤ **4**, base
**4** + depth². ProbCut: from depth **5**, margin **180**, depth reduction **4**.
Singular: from depth **7**, TT-depth allowance **3**, margin **2**/ply. IIR:
from depth **4**. Delta pruning (quiescence): margin **120**. Check extensions:
max **2**. Clock check: target **1.5 ms**, interval bounded **[64, 4096]** nodes.

Only **6** of these are exposed to SPSA (aspiration delta, RFP, futility
base/per-ply, probcut, delta pruning) — the rest are hardcoded and only
tunable by editing constants + rerunning self-play.

### Evaluation term inventory (855 tunable parameters)

Material (12), PSTs (768 = 6×64×2), doubled/isolated pawns, bishop pair, rook
open/semi-open, tempo, mop-up (edge/king/rook-confinement/BN-corner), 4 mobility
weights, a 32-entry king-attack curve, passed-pawn tables (mid/eg by rank +
own/enemy king distance), and 3 threat categories (pawn / minor-on-major /
hanging). **The checked-in delta table is all zeros** — the evaluation has
never been Texel-tuned on a corpus; it runs on the hand-authored PeSTO-derived
base weights.

### What is currently absent

- **No NNUE / no neural eval** — evaluation is entirely handcrafted and linear.
- **No SMP / multithreading inside the engine** — single-threaded search; the
  web layer runs two *separate* engines (move + analysis), not a shared
  parallel search. (`no unsafe` + Wasm single-thread model constrain this.)
- **No incremental Zobrist / eval** — `repetition_key`/`rule_key` and the full
  static eval are recomputed per node rather than updated incrementally on
  make/unmake (the code clones boards via `played` rather than make/unmake).
- **No syzygy / larger endgame tablebases** — only the compute-at-init KPK
  bitbase exists; KRK, KQK, KBNK etc. rely on the handcrafted mop-up terms.
- **No pondering, no explicit repetition-aware contempt, no dynamic contempt.**
- **No multi-cut beyond the singular path; no history of static-eval-based
  move ordering corrections beyond correction history.**
- **Opening book is tiny and optional** (8–32 KiB), ECO-derived, ≤3 replies
  per position; no learning or personalization.
- **Eval deltas unused; SPSA output (`spsa-parameters.json`) is not wired into
  the default build** — it must be manually promoted into constants.

### Architectural constraints any change must respect

1. **`unsafe_code = forbid`** crate-wide — no raw pointers, no manual SIMD
   intrinsics in Rust (SIMD comes only from the `+simd128` codegen build).
2. **Gzip size gate (225 KB)** — a `twiggy` profile runs in CI; features that
   grow the binary must be gated (as book/SIMD/tuning already are).
3. **Plain-data JS boundary** — public API takes/returns serializable data; no
   Rust internals leak into the worker protocol.
4. **Determinism** — CI gates (`accuracy`, `openings`, `seeding`,
   `cancellation`, `size`, `selfplay`) assume reproducible output.
5. **Strength claims require the harness** — any eval/search-constant change
   must be validated with `selfplay.mjs` (SPRT) or Texel loss, never eyeballed.
   This is stated as a hard rule in the repo's guidance.

### How to measure a proposed change

- **Node efficiency / speed:** `npm run bench:engine` (`bench.mjs`) — per-
  position nodes and time at matched time management.
- **Search correctness:** `accuracy.mjs` — fixed-depth tactical/mate suite.
- **Playing strength (Elo):** `selfplay.mjs` — SPRT baseline-vs-candidate with
  configurable Elo bounds (`--elo0`/`--elo1`), alpha/beta, and adjudication.
- **Eval fit:** `texel` binary — logistic train/validation loss on a FEN/result
  TSV.
- **Search-param optimization:** `spsa.mjs` — SPSA over the 6 exposed margins.
- **Binary size:** `validate-size.mjs` — gzip/brotli against the 230 KB budget.
