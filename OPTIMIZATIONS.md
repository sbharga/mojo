# Mojo Optimization Roadmap

This document proposes concrete optimizations and features for Mojo, organized so that each section is independently actionable. Every proposal is grounded in what ENGINE.md already describes: a single-threaded Rust/Wasm negamax-PVS core with a fixed 2 MiB transposition table, a handcrafted tapered evaluation, a two-worker browser integration, and an SPRT self-play harness. Sections are tagged with three quick facts:

- **Origin** — whether the technique is established engine practice, recent research (roughly 2023 onward), or a novel adaptation designed specifically for Mojo's browser/edge architecture.
- **Cost** — binary size, linear memory, and complexity added.
- **Payoff** — what it buys in speed, strength, or accuracy, stated qualitatively. Your SPRT harness is the final arbiter; treat every claim here as a hypothesis to test with it.

A closing priority matrix suggests an implementation order that front-loads the highest Elo-per-byte and Elo-per-hour items.

---

## Part 1 — Search

Search improvements are the cheapest strength you can buy for an edge engine: they cost essentially zero binary bytes, bounded memory, and they compound with everything else. Mojo already has a strong classical core (PVS, aspiration, null move, RFP, razoring, futility, LMP, SEE pruning, LMR, killers, countermove, history with gravity). The sections below are the highest-value pieces that core is still missing, ordered roughly by expected impact.

### 1. Transposition table redesign: buckets, in-entry static eval, and qsearch entries

**Origin:** established (bucketed/clustered tables and TT-resident static eval are standard in mature engines). **Cost:** zero binary, zero extra memory — same 2 MiB. **Payoff:** higher effective hit rate, cheaper interior nodes, better pruning decisions.

Mojo's table is direct-mapped: one 16-byte entry per index. Direct mapping is simple, but it means a single hot index can only remember one position, and depth-preferred replacement fights generation-preferred replacement inside a single slot. Three changes extract more value from the same 2 MiB:

1. **Bucketize.** Group entries into 64-byte, cache-line-aligned buckets of four 16-byte entries. A probe scans the four slots for a key match (one cache line, so nearly free); replacement picks the least valuable slot in the bucket using your existing policy (empty first, then oldest generation, then shallowest depth). Bucketing consistently raises the useful-hit rate over direct mapping at identical memory, because deep entries and fresh entries no longer evict each other when they collide on an index.
2. **Store the static evaluation in the entry.** Your entry layout (8-byte key, 2-byte move, 2-byte score, depth, bound, generation) leaves roughly two spare bytes if the 2-bit bound type is packed into the generation byte. Put a 16-bit static eval there. On any TT hit — even one too shallow for a cutoff — the node then skips recomputing static evaluation, which Mojo consults constantly for reverse futility, razoring, futility, and null-move gating. This also enables the *improving* heuristic (section 9) and correction history (section 6) without extra probes.
3. **Write and probe quiescence entries at depth 0 (or −1).** Quiescence dominates node counts in tactical positions; TT hits there prune whole capture trees. Your replacement policy already protects deeper entries, so qsearch entries naturally live in the "cheap to evict" tier and cannot displace main-search results of the same generation.

One safety note that matters more in a bucketed table: validate the TT move for pseudo-legality before playing it in-tree. With a full 64-bit stored key your collision risk is tiny, but a one-time cheap legality check (cozy-chess can verify a move against the board) converts "astronomically rare crash or illegal PV" into "astronomically rare wasted probe," which is the right trade for an engine that ships in other people's browsers. If you later shorten the stored key to 32 bits to make room for richer entries, this check stops being optional and becomes load-bearing.

### 2. Staged move generation with incremental selection

**Origin:** established. **Cost:** zero bytes, moderate refactor of the move loop. **Payoff:** meaningful node-rate increase; most beta cutoffs stop paying for work they never use.

Mojo currently generates the full legal move list, scores every move, and sorts. But alpha-beta's whole premise is that most nodes cut off on the first move or two — commonly the TT move alone. Every fully generated, SEE-scored, sorted list at a node that cuts on move one is wasted work. The standard fix has two halves:

- **Stage the generator.** Emit moves in phases: TT move (verified, no generation at all), then captures and promotions, then killers and countermove, then remaining quiets. cozy-chess's mask-driven generation supports this cleanly — restrict targets to enemy occupancy for the capture phase, its complement for quiets. If the TT move or an early capture cuts off, the quiet phase is never generated, never SEE-scored, never history-scored.
- **Select instead of sort.** Within a phase, replace the up-front `sort_unstable` with an incremental selection: each time the loop needs the next move, scan the remaining scored moves for the max and swap it forward. Selection is O(n) per move taken, so a node that examines two moves pays ~2n comparisons instead of n·log n for a full sort, and the common case examines very few. Your fixed-capacity arrays make this a small, allocation-free change.

The interaction with your SEE caching is worth preserving: keep computing SEE lazily, at the moment a capture is first scored in its phase, and carry it into quiescence pruning exactly as you do now — staging makes that laziness pay off even more, because captures behind an early cutoff are never scored at all.

### 3. Continuation history (and history-driven pruning/reductions)

**Origin:** established in all modern top engines; the memory-slimmed variants below are adapted for Mojo's budget. **Cost:** zero binary; 288 KiB–1.1 MiB linear memory depending on variant. **Payoff:** one of the largest remaining move-ordering gains available to you; unlocks better LMR and pruning decisions.

Your quiet-move ordering is from/to history plus one countermove. Modern engines get a large share of their ordering quality from *continuation history*: a table indexed by the previous move's (piece, to-square) and the candidate move's (piece, to-square), answering "given the opponent just did X, how good has reply Y historically been?" This generalizes your single stored countermove into a full scored distribution and is typically worth substantially more than plain history.

Memory is the design decision on a 2 MiB-class engine:

- Full-fidelity 1-ply table: `[12 piece][64 to] × [12 piece][64 to]` of i16 ≈ **1.125 MiB**.
- Side-to-move-relative pieces (fold color): `[6][64] × [6][64]` of i16 ≈ **288 KiB** — the sweet spot for Mojo. You lose almost nothing because the previous move's color is always the opponent's.
- Same table in i8 with saturating gravity updates ≈ **144 KiB** if you want two plies (previous move and the move before that, i.e., your own last move) for ~288 KiB total. The 2-ply "follow-up" plane is a well-documented additional gain.

Update it exactly like your existing history (depth-squared bonus, gravity saturation, halve on root change), applied to the quiet move that caused a cutoff and, negatively, to the quiets searched before it. Then spend the signal twice more:

- **History pruning:** at shallow depth, skip late quiets whose combined history (main + continuation) is strongly negative.
- **History-adjusted LMR:** reduce badly-scored quiets one ply more and well-scored quiets one ply less (see section 8).

### 4. Capture history

**Origin:** established. **Cost:** ~9 KiB linear memory (`[12 piece][64 to][6 captured]` i16), zero binary. **Payoff:** small but consistent; nearly free.

SEE plus victim value orders captures well, but ties are common (every PxN looks alike to SEE) and SEE is blind to positional consequences. A capture-history table — updated when a capture causes a cutoff or fails — breaks those ties with observed results. Use it as a secondary key after SEE class, and as an input to the SEE-pruning margin for captures (a capture with strongly positive capture history earns a slightly more lenient prune threshold). At 9 KiB this is the cheapest ordering signal on this list.

### 5. Correction history

**Origin:** recent research — introduced in the engine Caissa in October 2023, adopted rapidly across the field, and merged into Stockfish at the end of 2023 with several follow-on variants (material-indexed, continuation-indexed). **Cost:** ~64–256 KiB linear memory, zero binary. **Payoff:** notably improves the accuracy of every static-eval-based decision (RFP, razoring, futility, null-move gating, stand-pat) without touching the evaluator; reported gains grow at longer thinking times.

The idea: your handcrafted eval is systematically wrong in consistent ways for particular structures — it might chronically underrate a given pawn formation by 40 centipawns. Correction history measures this online. Keep a small table indexed by a *pawn-structure hash* (a Zobrist over pawns only, folded to 14–16 bits) per side to move. After a search at a node returns a score that is usable as a bound on the true value, update the table toward `(search_score − static_eval)`, scaled by depth, with the same gravity-style saturation you already use for history. When evaluating, output `static_eval + correction[stm][pawn_hash] / scale`.

Why this is a particularly good fit for Mojo: your evaluator is deliberately cheap and handcrafted, which means its systematic biases are larger than a big NNUE's — so the correction signal is stronger. And because the corrected eval feeds your aggressive shallow-depth pruning stack, accuracy there converts directly into fewer wrong prunes. Implementation notes that match established practice: exclude nodes in check, exclude fail-highs produced by captures, clamp the applied correction (e.g., ±32 internal units in Stockfish's first version), and clear or decay the table with your existing root-change halving. Once the pawn-keyed table works, a second table keyed by a coarse material signature is the standard next increment.

### 6. Singular extensions (with negative extensions and multicut for free)

**Origin:** established in every top engine; one of the largest single search features Mojo lacks. **Cost:** zero bytes; moderate implementation care. **Payoff:** consistently among the bigger single-patch strength gains reported by engine authors when added to a mature PVS core.

When the TT suggests a move at sufficient depth with a lower-bound score, test whether that move is *singular* — clearly better than every alternative. Run a reduced-depth search over all other moves with a window just below the TT score (`beta = tt_score − margin`, zero width). If everything fails low, the TT move is the position's only idea: extend it one ply, because getting forced lines right is worth extra depth. Mojo's existing check-extension cap machinery (two per line) generalizes naturally into a per-line extension budget covering both.

The same exclusion search gives you two bonus features for a few lines each:

- **Multicut:** if the exclusion search instead *fails high* — several alternatives also beat the reduced bound — the node is rich in refutations; you can often return the bound immediately without a full search.
- **Negative extensions:** if the TT move is not singular and the exclusion search suggests the node is unstable, search the TT move at slightly *reduced* depth instead.

Gate the feature the way the field does: require a minimum depth (say 7–8), a TT entry of adequate depth with a lower/exact bound, and skip at the root and near mate scores. This feature interacts with everything, so it is the strongest argument on this list for the fixed-node determinism mode your test harness already supports — get it correct at fixed nodes, then let SPRT judge it at time.

### 7. Internal iterative reduction (IIR)

**Origin:** established modern practice (successor to internal iterative deepening). **Cost:** one or two lines. **Payoff:** small, cheap, additive.

At a node that *should* have a TT move (sufficient depth, expected PV or cut node) but doesn't, the position was never properly explored — its move ordering will be poor and the search there is likely to be wasted at full depth. Instead of the old remedy (an internal shallower search to find a move), modern engines simply reduce the depth of the current search by one ply. The re-search that iterative deepening performs next iteration then has a TT move. It is the cheapest depth-shaping rule in the modern toolkit and slots into Mojo before the move loop with a single conditional.

### 8. LMR refinement: a log-log reduction table with contextual adjustments

**Origin:** established. **Cost:** ~2 KiB for a precomputed table, zero binary otherwise. **Payoff:** LMR quality is a first-order driver of effective depth; tuning it is high leverage.

ENGINE.md describes which moves are excluded from reduction, but the size of the reduction is where modern engines concentrate value. The standard base is a precomputed table `R[depth][move_index] ≈ a + ln(depth)·ln(move_index)/b`, with `a` and `b` as tunable constants (SPSA candidates, section 27). On top of the base reduction, apply small contextual deltas, each of which is one comparison:

- reduce **less** when the node is a PV node, when the move has strongly positive (continuation) history, when the move gives check, or when the position is *improving* (section 9);
- reduce **more** when the node is a cut node, when history is strongly negative, when the TT move is a capture, or when the move is late and the node is not improving.

Keep your existing re-search ladder (reduced → full depth → full window) untouched; only the initial reduction changes. Because every delta is a tunable integer, this section is the single best customer for the SPSA harness proposed later — engines routinely harvest a long tail of small gains here.

### 9. The improving heuristic

**Origin:** established. **Cost:** a two-slot static-eval stack, effectively free once section 1 stores eval in the TT. **Payoff:** sharpens nearly every margin-based pruning rule you already have.

Track static eval per ply and define `improving = eval[ply] > eval[ply − 2]` (side to move is doing better than two plies ago). A position that is improving deserves more caution before pruning; one that is deteriorating can be pruned harder. Standard consumers, all of which Mojo already implements in margin form: reverse futility margins shrink when not improving; late-move-pruning move-count thresholds drop when not improving; LMR reduces more when not improving. This is a classic example of a free feature whose value is unlocked by TT-resident static eval — the eval you need at `ply − 2` is usually already sitting in the entry you probed there.

### 10. Null-move upgrades: verification search and the threat move

**Origin:** established. **Cost:** zero bytes. **Payoff:** safer null-move pruning at depth; a free ordering/extension signal.

Two additions to your already well-guarded null move:

- **Verification search.** At high remaining depth (say ≥ 10), when the null-move probe fails high, confirm with a reduced-depth real search before trusting the cutoff. This lets you *loosen* the null-move guards elsewhere (e.g., permit it with less non-pawn material) because zugzwang mistakes get caught. Net effect in practice is more pruning, not less.
- **Capture the threat move.** When the null-move search refutes the pass, the refuting move is the opponent's *threat*. Store it: order it early at sibling nodes, and treat quiet moves that defend against it as less prunable by futility/LMP. It costs one field on the search stack and recovers information the null-move search already paid for.

### 11. ProbCut

**Origin:** established. **Cost:** zero bytes. **Payoff:** moderate; prunes deep nodes where a good capture already decides the position.

At depth ≥ 5, when `beta` is not a mate score, try only winning-SEE captures with a shallow search against a raised bound (`beta + margin`, typically 150–200 cp). If a shallow search already exceeds beta by a comfortable margin, the full-depth search almost certainly would too — return the bound. ProbCut complements your existing suite: RFP and razoring act on *static* eval at shallow depth; ProbCut acts on a *shallow search* at high depth, which is exactly the region your current pruning stack leaves untouched (everything but null move is capped at depth ≤ 8). Store ProbCut results in the TT (bounded, at the reduced depth) so repeated probes are free.

### 12. Aspiration windows: progressive widening

**Origin:** established. **Cost:** a small loop. **Payoff:** modest time savings at higher depths, exactly where your wall-clock budget is scarcest.

ENGINE.md says a fail outside the aspiration window triggers a *full-window* re-search. The standard refinement is geometric widening: on a fail-high, keep alpha and widen beta by a growing delta (e.g., delta ← delta·2 each retry, starting near 12–20 cp, re-centered on the failed bound); symmetric for fail-low; fall back to the full window only after a few failed rounds or near mate scores. Most fails resolve one small step outside the window, so paying for an immediate full-window search discards the aspiration savings precisely on the iterations that matter most (the deepest, most expensive ones). Widen asymmetrically — only the failing side — and preserve your MultiPV per-line centering as is.

### 13. Root ordering by subtree effort

**Origin:** established. **Cost:** one u64 per root move. **Payoff:** faster convergence at the root; better time-management signals for Part 3.

Order root moves for iteration *n+1* primarily by iteration *n*'s scores, but break ties and rank non-PV moves by the *node count of their subtrees* in the previous iteration. Subtree size is a strong proxy for "this move poses real problems," and it is information the search already generates. As a side benefit, the ratio `nodes(best move) / nodes(all root moves)` is the standard "node-fraction" input to smarter time management (section 22): when the best move consumed almost all the effort, the position is decided and the search can stop early.

---

## Part 2 — Evaluation

Two paths exist for evaluation strength: keep the handcrafted evaluator (HCE) and make it *correct* (sections 14–20), or adopt a micro-NNUE (section 21). They are not mutually exclusive — the HCE improvements are worthwhile even as a fallback build or a training-data bootstrap — but the tuning infrastructure in section 14 is the prerequisite for getting full value from any HCE term you add.

### 14. Texel-style tuning of the existing evaluation — do this before adding terms

**Origin:** established (Texel tuning, 2014; modern practice does the same fit with gradient descent). **Cost:** zero runtime bytes; offline tooling only. **Payoff:** typically the single largest accuracy improvement available to an untuned or hand-tuned HCE — often worth more than any individual new evaluation term.

Every weight in ENGINE.md's evaluation — material pairs, PST cells, mobility per-square weights, tropism weights, pawn-structure penalties, passed-pawn bonuses — is currently a hand-chosen constant. Texel tuning replaces hand-chosen with *fitted*: collect a few hundred thousand to a few million quiet positions labeled with game results, model `P(win) = sigmoid(eval / K)`, and minimize the error over all evaluation parameters by gradient descent (the mg/eg tapered blend is linear in the weights, so gradients are exact and cheap).

Mojo is unusually well positioned to do this with zero external dependencies, which matters for your licensing discipline:

1. Your self-play harness already produces games from a 2,014-opening ECO corpus with reproducible adjudication — run it in a data-generation mode that dumps (FEN, game result) pairs.
2. Filter to quiet positions (your SEE and check machinery already define "quiet") and deduplicate by Zobrist key.
3. Fit offline in Rust; emit the weight tables as generated source, exactly like your opening-corpus pipeline emits data with a recorded source hash.
4. Iterate: retuned eval → stronger self-play → better data → retune. Two or three loops is standard before returns flatten.

Every subsequent evaluation section becomes "add the term with a rough weight, let the tuner set it," which is both faster and stronger than hand-tuning each addition. Verify each retune with the SPRT harness at fixed time, since tuning shifts the speed/knowledge balance.

### 15. Pawn-structure hash table

**Origin:** established. **Cost:** ~64–128 KiB linear memory; zero binary. **Payoff:** makes the entire pawn/king-structure slice of your evaluation nearly free, which in turn makes richer pawn terms (section 16) affordable.

Pawn structure changes rarely — only pawn moves and captures touch it — so its evaluation is the classic caching target. Maintain an incremental pawn-only Zobrist key (cozy-chess exposes piece/square deltas cleanly at your make/copy boundary), and cache the pawn-dependent evaluation in a small direct-mapped table: doubled/isolated/passed terms, pawn-shield skeleton, and the passed-pawn *identification* (store the passer bitboards; the king-distance parts of your endgame passer terms depend on king squares and stay outside the cache). Hit rates in normal middlegame search run extremely high, so the amortized cost of the pawn slice drops to a table lookup. Size it modestly (e.g., 4,096–8,192 entries × 16 bytes) — even small pawn tables hit constantly because search churns pieces, not pawns.

### 16. Safe mobility and threat terms

**Origin:** established. **Cost:** a few hundred bytes of weights; small eval-time cost, offset by section 15. **Payoff:** meaningful accuracy in exactly the positions where cheap evaluations blunder — pieces that look active but are actually loose.

Two targeted upgrades to your existing mobility/activity block:

- **Safe mobility.** Count only attacked squares *not controlled by enemy pawns* (and optionally exclude squares occupied by your own blocked pawns). One extra bitboard AND per piece against a pawn-attack mask you can precompute once per node (or store in the pawn hash). A knight with eight moves that all land on pawn-controlled squares is not mobile, and plain mobility counts systematically overrate such pieces.
- **Threats.** Small bonuses for pawn attacks on enemy pieces, minor-piece attacks on majors, and any attack on an undefended (hanging) piece. These give one-ply tactical awareness to the *static* eval, which pays off most in quiescence stand-pat decisions and futility margins, where the search is explicitly trusting the static score.

Both slot into the Texel pipeline for weighting, and both reuse attack bitboards your mobility loop already computes.

### 17. King-attack units (upgrade tropism to an attack model)

**Origin:** established. **Cost:** ~100–200 bytes (a small nonlinear lookup); minor eval cost. **Payoff:** substantially better king-safety judgment than distance-based tropism; the standard cure for cheap-engine attacking blindness.

Your middlegame tropism rewards proximity-zone attacks linearly per piece. The established stronger model is *attack units*: for each enemy piece attacking the king zone, add piece-specific units (e.g., minor 2, rook 3, queen 5), add units for multiple attackers and for squares in the zone that are attacked and undefended, then map the accumulated total through a small **nonlinear** table to centipawns. The nonlinearity is the entire point — one attacker is noise, three attackers with the defending pieces far away is usually decisive, and a linear model cannot express that. Keep your pawn-shield term as an input (reduce units for intact shields) rather than an independent bonus, so the two cannot double-count. Fit the curve with the tuner.

### 18. Packed mg/eg scores

**Origin:** established (the classic packed-score trick). **Cost:** none — this *removes* work and shrinks tables. **Payoff:** a clean evaluation speedup and a small binary-size reduction.

Represent every tapered term as a single i32 holding (mg: i16, eg: i16) packed together, accumulate the whole evaluation as one i32 sum, and split once at the end for the phase blend. Addition and subtraction of packed pairs is a single integer op (with the standard carry-guard trick for the sign of the low half). Your PSTs and weight tables halve in count (one packed table instead of separate mg/eg tables), every eval term does one accumulation instead of two, and the final tapered blend happens exactly once. This is also the natural stepping stone to incremental evaluation: with packed scores, maintaining a running material+PST accumulator updated per move (rather than recomputed per node) becomes a small, contained change — worthwhile on its own, and mandatory groundwork if you ever adopt NNUE-style accumulation.

### 19. Halfmove-clock evaluation damping ("shuffle scaling")

**Origin:** established in top engines. **Cost:** one multiply. **Payoff:** better conversion behavior and draw handling — an *accuracy* feature that directly targets your stated goal.

Scale the static evaluation toward zero as the halfmove clock climbs, e.g. `eval ← eval · (256 − 2·halfmove_clock) / 256` or a tuned equivalent. The justification: a position where the stronger side has made no progress for 30 reversible moves is empirically much more drawish than its material suggests, and the 50-move rule looms. The search consequence is what you want — the engine actively prefers lines that make progress (pawn pushes, exchanges) over shuffling, which specifically strengthens the endgame-technique goals ENGINE.md already invests in with its mating-gradient terms. Your rule-sensitive TT key already includes the halfmove clock, so cached scores remain consistent with the damped eval — a prerequisite many engines had to retrofit that Mojo gets for free.

### 20. Compute-at-init KPK bitbase (and DIY micro-bitbases)

**Origin:** established (Stockfish has generated its KPK bitbase at startup for over a decade). **Cost:** **zero download bytes**; ~24 KiB linear memory; a few milliseconds at init. **Payoff:** perfect play in king-and-pawn-vs-king, the single most common "technically won but botchable" ending, and correct *draw* claims in drawn KPK — a pure accuracy win.

This is the ideal edge-computing shape for endgame knowledge: don't ship the table, ship the 100-line generator. KPK has ~196,608 relevant configurations (2 sides to move × 24 normalized pawn squares × 64 × 64 king squares); a retrograde fixpoint over one bit per configuration fits in 24 KiB and converges in milliseconds — fold it into your existing eager/lazy Wasm init promise. Consult it in evaluation (and optionally at horizon nodes): won positions get a decisive score plus your existing progress-gradient shaping; drawn positions return the draw score *exactly*, which stops the engine from ever trading into a drawn pawn ending it thinks is winning — a mistake gradient-only evaluations make.

The same pattern extends selectively: a KQKP bitbase (rook-pawn and bishop-pawn draws are famously misplayed by shallow searches) or KRKP are candidate follow-ups, each a similar-size generated table. Resist generality — five-man Syzygy-style coverage is hundreds of megabytes and contradicts Mojo's design; two or three *generated* micro-bitbases targeting the endings that actually decide browser games is the right trade. Your unit-test culture fits perfectly: validate the generated KPK table against known key positions (the classic mutual-zugzwang and rook-pawn draws) in CI.

### 21. The micro-NNUE decision

**Origin:** established architecture (NNUE, 2018), with a mature small-engine ecosystem as of the mid-2020s — the Rust-based `bullet` trainer is now the most widely adopted NNUE trainer among engines, and the standard small-engine architecture is a single-hidden-layer perspective network, `(768 → N)×2 → 1`. **Cost:** the big one — see the size math below — plus a training pipeline. **Payoff:** the largest accuracy jump available to Mojo by a wide margin; hundreds of Elo over even a well-tuned HCE is the routine experience of engines that made this transition.

ENGINE.md explicitly positions Mojo as having *no neural-network evaluator*, so treat this section as a decision document rather than a recommendation. The honest summary: if "accurate" ever becomes the binding constraint rather than "small," nothing else in this roadmap competes with a small net.

**Size math** (i16 weights, the standard quantization for the first layer; per-side accumulators of N i16s):

| Architecture | Feature-transformer weights | Approx. total raw | Note |
|---|---|---|---|
| (768 → 32)×2 → 1 | 768·32·2 B = 48 KiB | ~49 KiB | proven viable in small engines |
| (768 → 64)×2 → 1 | 96 KiB | ~97 KiB | good strength/size balance |
| (768 → 128)×2 → 1 | 192 KiB | ~194 KiB | comfortably strong for browser play |

Raw weights are high-entropy, so gzip/Brotli recovers only a modest fraction — budget close to the raw figure against your compressed-size line in the benchmark report. Even the 128-wide net is smaller than many hero images; the question is whether it fits *your* budget, not whether it fits a browser.

If you proceed, the pieces map cleanly onto infrastructure Mojo already has:

1. **Data:** your self-play harness + ECO corpus generate labeled positions with recorded provenance (same pipeline as section 14; HCE-Mojo bootstraps the first net's data, then the net regenerates better data).
2. **Training:** `bullet` is Rust, matching your toolchain; the (768→N)×2→1 dual-perspective layout is important — dual perspective lets the net encode tempo and has measured out dramatically stronger than single-perspective at equal size in community tests.
3. **Inference:** the accumulator update is the "efficiently updatable" part — a move touches at most four features, so updating two N-wide i16 accumulators costs a handful of adds/subs. With cozy-chess's copy-make style, keep an accumulator stack indexed by ply (copy + delta on make, pop on unmake) rather than true in-place undo. Quantized i16 dot products vectorize perfectly on Wasm SIMD128 (section 26).
4. **Fallback:** keep the HCE behind a feature flag as the no-SIMD/minimum-size build, preserving the "ultra lightweight" build as a product option rather than a casualty.

A middle path worth naming: keep the HCE for the main build, and use a *tiny* net (768→16 or a screened subset of inputs, ~25 KiB) purely as a **correction term** added to the HCE — this bounds the size cost while capturing pattern knowledge the handcrafted terms miss. It is nonstandard but composes naturally with section 5's correction philosophy: HCE for the explainable base, learned residual for the rest.

---

## Part 3 — Time management

Mojo's time model is sound (per-iteration wall-clock budgets, 256-node checks, completed-depths-only). But it currently treats the budget as a limit rather than a resource to allocate, and the "only completed depths count" rule — a good rule — makes wasted partial iterations more expensive for Mojo than for a typical UCI engine. These three sections turn timing into strength at zero byte cost.

### 22. Soft/hard deadlines with best-move stability and node-fraction stopping

**Origin:** established. **Cost:** zero bytes. **Payoff:** engine authors consistently report time-management work as one of the best Elo-per-effort areas; it strengthens *every* per-move time setting from 100 ms to 10 s.

Split the user's per-move budget into a **hard** deadline (the current behavior — never exceed) and a **soft** deadline (default around 40–60% of the budget) checked *between iterations*. The soft deadline flexes with evidence:

- **Stability:** if the best move has been identical for the last several completed iterations, shrink the soft deadline (multiply by ~0.8 per stable iteration, floored) — the position is decided; bank the time.
- **Instability / score drops:** if the best move just changed, or the score fell materially versus the previous depth, extend the soft deadline toward the hard one — this is exactly the move you must not play too quickly.
- **Node fraction:** using section 13's per-root-move node counts, when the best move consumed the overwhelming share of the last iteration's effort, stop early even if depth is low; when effort was scattered, keep thinking.

Because Mojo's user setting is per-move rather than per-game, "banked" time cannot roll over — so express the savings as *responsiveness* (move instantly in decided positions, a visible UX win) while spending the full budget only where the search is genuinely uncertain.

### 23. Don't start what you can't finish (EBF-gated iterations) — *novel adaptation*

**Origin:** the underlying prediction is classic; gating is unusually valuable in Mojo's architecture specifically, because a timed-out iteration contributes *nothing* under your completed-depths-only rule. **Cost:** zero bytes. **Payoff:** recovers the tail of the budget that today is regularly burned on doomed iterations.

Before launching depth *d+1*, estimate its cost from measured behavior: keep the per-depth elapsed times you already report, compute the recent effective branching factor `EBF ≈ t(d)/t(d−1)` (smoothed over 2–3 iterations), and predict `t(d+1) ≈ t(d) · EBF`. If `predicted > remaining_hard_budget × safety` (safety ≈ 1.2–1.5, tunable), skip the iteration and return the depth-*d* result immediately. Two Mojo-specific refinements: treat MultiPV analysis differently (its per-depth cost is ~3 root searches, so its EBF is noisier — widen the safety factor), and let a *strong instability signal* from section 22 override the gate, since a re-searching iteration that only needs to verify a new best move is much cheaper than its predecessor and the naive prediction over-forbids exactly the iterations you most want.

### 24. Adaptive clock-check granularity

**Origin:** established idea, tuned here for edge-device variance. **Cost:** zero bytes. **Payoff:** bounds deadline overrun *in milliseconds instead of nodes* across the enormous performance range of edge hardware.

A fixed 256-node check interval means the overrun bound in wall time varies by an order of magnitude between a desktop Chrome tab and a low-end Android phone — and the *cost* of the check (a call across the Wasm clock boundary) varies too. Calibrate instead: after depth 1 completes you know nodes-per-millisecond on *this* device; set the check interval to target a fixed real-time bound (e.g., check every ~1–2 ms of predicted work), clamped to [64, 4096] nodes, and recalibrate as depths complete. Fast devices check less often (fewer boundary crossings, higher nps); slow devices check more often (your 100 ms minimum budget stays honest). This preserves your deterministic node-limit test mode untouched — the calibration only replaces the constant 256 in the wall-clock path.

---

## Part 4 — Browser, Wasm, and edge-specific engineering

These sections exploit the part of Mojo's design space most engines never see: two cooperating single-threaded engine instances inside a browser's security and scheduling model. Several are novel adaptations rather than textbook techniques.

### 25. Mid-search cancellation via a SharedArrayBuffer stop flag — *novel adaptation*

**Origin:** standard UCI engines stop mid-search via an atomic flag; adapting it across the JS/Wasm worker boundary is Mojo-specific. **Cost:** zero binary; requires serving the app cross-origin isolated (COOP `same-origin` + COEP `require-corp` headers). **Payoff:** cancellation latency drops from "up to one full iteration" to "one clock-check interval"; the deepest iterations are precisely the ones you currently cannot interrupt.

Today, cancellation waits for the worker's between-depth yield — fine at depth 6, painful when depth 14 has 4 seconds left and the user has already moved on. The fix: allocate one `SharedArrayBuffer` Int32 per worker, hand it to both the main thread and the worker, and check it inside the search's existing periodic check (the same place the clock is read) via an atomic load. When the UI's cancellation watermark advances, `Atomics.store` the flag; the search aborts within one check interval and returns the last completed depth exactly as a timeout does — your "interrupted search is never reported as completed" test already specifies the correct semantics.

The check itself costs no more than the clock call you already make there. The deployment cost is the honest one: cross-origin isolation constrains what third-party resources the *page* can embed, so make the flag optional — feature-detect `SharedArrayBuffer`, fall back to current between-depth cancellation when absent. Note that shipping the headers also unlocks sections 28's threads, so the deployment decision is shared.

### 26. Wasm SIMD128 as a feature-detected second build

**Origin:** established platform capability. Fixed-width 128-bit Wasm SIMD is supported across all modern browsers (Chrome 91+, Firefox 89+, Safari 16.4+); Relaxed SIMD is newer (Chrome 114+, Firefox 120+, recent Safari) and best treated as optional icing. **Cost:** a second `.wasm` artifact (compiled with `-C target-feature=+simd128`) plus a ~30-byte `WebAssembly.validate` probe in the loader; near-zero code change if you rely on autovectorization. **Payoff:** modest for the current scalar HCE; *transformative* if section 21's NNUE happens (2–4× is the routine SIMD speedup reported for Wasm inference workloads), and useful for batch bitboard/eval loops either way.

Rust with `+simd128` autovectorizes eligible loops, and `core::arch::wasm32` intrinsics cover hand-written kernels (the i16 dot products of an NNUE accumulator are the canonical case). Ship the baseline build unchanged, probe once at startup with the standard `WebAssembly.validate` byte-string test, and load the SIMD build when available — the same dual-build pattern the ported Stockfish builds use in production. Wire raw-and-gzip size of *both* artifacts into your existing benchmark report so the cost stays visible. Skip Relaxed SIMD until you have a measured kernel that needs its FMA/dot-product forms; determinism across devices (which your test suite values) is slightly weaker there by design.

### 27. Cross-worker knowledge sharing: TT seeding and "ponder-lite" — *novel adaptation*

**Origin:** novel composition; the ingredients (pondering, TT sharing) are classic. **Cost:** zero binary; a small serialization path across the existing FEN/records boundary. **Payoff:** the move worker starts warm instead of cold, effectively converting the analysis worker's ongoing work — currently discarded for play — into search depth.

ENGINE.md isolates move selection and analysis into separate workers with separate engine instances so analysis can never delay a move. Keep that isolation, but stop discarding the analysis worker's knowledge:

- **PV seeding (cheap, do first).** When a move request arrives, the analysis worker has usually been searching the *same position* (or its predecessor) for seconds. Have the UI forward the analysis worker's latest completed result — best line, score, depth — to the move worker, which installs the PV moves into its TT as exact/lower-bound entries at their searched depths before starting. Iterative deepening then begins with correct move ordering from depth 1, which is most valuable at the short (100–500 ms) budgets where Mojo currently re-derives everything from scratch.
- **Bulk TT export (bigger, later).** Extend the Wasm boundary with "export top-K TT entries touched by the last search" / "import entries" (a flat array of key/move/score/depth/bound). A few thousand entries seed the move worker's table for a few kilobytes of postMessage traffic.
- **True shared TT (advanced).** With cross-origin isolation (section 25) and shared Wasm memory, both instances could address one TT using the classic lockless XOR scheme for torn-write safety. This is the highest-value endpoint but changes your memory model; the two message-based steps above capture most of the benefit without it.

The same machinery gives you **pondering** almost for free: while the human thinks, point the analysis worker at the position after the engine's *expected reply* rather than the current position; on a correct guess, the seed is a full search of the exact position to move in.

### 28. Optional Lazy SMP (threads) build

**Origin:** established (Lazy SMP is the standard shared-memory parallel search; the browser mechanics are proven by the threaded Stockfish Wasm ports lichess runs, which require the same COOP/COEP headers as section 25). **Cost:** significant build/deploy complexity: shared Wasm memory, a thread pool, per-thread search stacks (~a few hundred KiB each), cross-origin isolation. **Payoff:** the only way to use more than one core of an edge device; large at 1–10 s budgets, smaller at 100 ms where thread ramp-up eats the window.

Lazy SMP is philosophically friendly to Mojo's simplicity: N identical searches on the same shared TT with slightly varied depths/parameters, no work-splitting logic — the shared table *is* the coordination. In Rust-Wasm this means building with atomics + shared memory and either `wasm-bindgen-rayon` or a hand-rolled worker pool. Honest guidance: sequence this *after* sections 1, 25, and 27, since it depends on the bucketed lockless-friendly TT and the isolation headers, and cap default threads well below `navigator.hardwareConcurrency` on mobile (thermal throttling makes 2–3 threads the realistic sweet spot). Keep the single-threaded build as the default artifact; treat threads as a progressive enhancement your feature detection selects.

### 29. Binary-size squeeze: measure, then trim

**Origin:** established Rust/Wasm practice. **Cost:** none — this section only removes bytes. **Payoff:** directly serves "ultra lightweight"; typically a double-digit-percent reduction is available to a first size pass.

Your release profile is already good (fat LTO, one CGU, abort panics, strip, `wasm-opt -O`). The remaining checklist, in order of typical yield:

1. **Profile first:** `twiggy top` / `twiggy dominators` on the release `.wasm` tells you what actually costs bytes — panic formatting machinery, `core::fmt`, and monomorphization bloat are the usual suspects in engines.
2. **Try `opt-level = "z"`** (and `"s"`) against your benchmark: engines are branch-heavy and cache-sensitive, so `z` sometimes costs single-digit-percent speed for double-digit-percent size — your bench harness makes this a 10-minute measured decision instead of a guess.
3. **`wasm-opt -Oz`** instead of `-O`, plus `--strip-producers --strip-target-features`.
4. **Eliminate residual panic/format paths:** audit for formatting in reachable code (error strings crossing the boundary), prefer numeric error codes internally; `wasm-snip` can stub what the optimizer can't prove dead (it operates on the wasm, so it coexists with your `forbid(unsafe)` policy).
5. **Serve Brotli**, and add Brotli size next to gzip in the benchmark report — for Wasm it is consistently the smaller wire format, and wire size is the number an edge deployment actually pays.

The meta-recommendation: add `twiggy top -n 20` output to CI artifacts so size regressions are attributable per-commit, the same way your report already pins speed and strength.

### 30. Repetition-aware analysis-cache keys — fixes a documented limitation

**Origin:** novel, Mojo-specific; ENGINE.md itself flags the gap ("repetition-dependent results reached through different histories are not distinguished"). **Cost:** zero binary; ~8 bytes per cache key. **Payoff:** removes a correctness caveat from the analysis cache, letting it stay hot in exactly the navigation patterns (move back/forward through a game) where repetitions actually occur.

The fix follows from the chess rule itself: only positions since the last irreversible move can repeat, and the FEN's halfmove clock counts exactly that window. So augment the LRU key with a **repetition fingerprint**: an order-insensitive combination (e.g., a commutative hash-sum) of the Zobrist keys of the last `halfmove_clock` prior positions, *filtered to those that could still recur* — or, in the cheapest sufficient form, just the multiset of prior keys that match any position reachable in the search window. Two FENs that are identical but arrived with different relevant histories now cache separately; FENs whose differing histories are all pre-irreversible-move (the common case) still share one entry, preserving the hit rate. This costs one pass over the (already supplied) prior-FEN array per lookup and closes the one asterisk on an otherwise clean caching design.

---

## Part 5 — Tuning and data infrastructure

Mojo already has the rare thing: a reproducible SPRT self-play harness with a licensed, deduplicated opening corpus. These two sections build on it; they are what turn the dozens of tunable constants introduced above from guesses into measured values.

### 31. SPSA parameter tuning on top of the SPRT harness

**Origin:** established — SPSA is the workhorse of Stockfish's fishtest and the OpenBench ecosystem for continuous-parameter tuning. **Cost:** offline tooling only. **Payoff:** the compounding mechanism for this whole roadmap; individually un-SPRT-able ±2 Elo constants (LMR deltas, futility margins, aspiration deltas, history bonuses, time-management factors) become collectively harvestable.

SPRT answers "is patch A better than baseline B?"; it cannot efficiently answer "what should these 30 numbers be?" SPSA (simultaneous perturbation stochastic approximation) does: perturb all parameters simultaneously by random ±deltas, play a small batch of paired games between the + and − configurations, step every parameter along the estimated gradient, repeat for tens of thousands of games. Requirements Mojo mostly already meets: paired openings with color reversal (have), deterministic adjudication (have), fast games (your fixed-depth mode or 100 ms budgets), and a way to set parameters per-instance — the one addition, best done as a plain "set search constants" record across the existing serialized boundary, compiled out of release builds if you prefer. Convention from the fishtest world worth copying: tune at short time control, then *confirm the tuned bundle with a normal SPRT run* at your real time controls before merging, since a few parameters (especially time-management ones) don't transfer across speeds.

### 32. A tiny embedded opening book (optional, revisits a design decision)

**Origin:** established format (Polyglot-style Zobrist-keyed book), novel sourcing from infrastructure you already ship. **Cost:** ~8–32 KiB binary (the one section in this roadmap that *adds* download bytes besides NNUE); trivial runtime. **Payoff:** better and *faster* opening play — book moves cost 0 ms of the move budget — plus game-to-game variety, which pure determinism currently lacks.

ENGINE.md deliberately excludes a runtime book, and that is defensible. The reason to revisit: your repo already contains a licensed, validated, hash-recorded corpus of 2,014 ECO openings with a generation pipeline — the expensive part of a book (curated, license-clean data with provenance) is done. A build step can compact the first 6–10 plies of that corpus into a Zobrist-keyed table of (position key → 1–3 vetted replies, weighted), binary-searched at ~10–16 bytes per entry. At move time: on a book hit, play instantly (optionally with small weighted randomness for variety — a user-visible feature for a training tool); on a miss, search as today. Keep it behind a build flag so the minimal artifact stays bookless, and validate every book move with the same chess.js/Wasm acceptance checks your corpus pipeline already runs.

---

## Priority matrix

Ordered by expected return on effort for Mojo specifically, given its stated goals (lightweight, fast, accurate, edge). "Size" is added download bytes; "Memory" is added linear memory at runtime.

| # | Section | Size | Memory | Effort | Expected impact |
|---|---|---|---|---|---|
| 1 | 14. Texel tuning of existing eval | 0 | 0 | Medium (offline) | Very high — likely the largest single accuracy gain available without a net |
| 2 | 1. TT buckets + in-entry eval + qsearch TT | 0 | 0 | Medium | High — speed and pruning accuracy from the same 2 MiB |
| 3 | 2. Staged movegen + incremental selection | 0 | 0 | Medium | High — raw node rate |
| 4 | 6. Singular/negative extensions + multicut | 0 | 0 | Medium-high | High — biggest missing search feature |
| 5 | 3. Continuation history (+ history pruning/LMR) | 0 | ~288 KiB | Medium | High — largest remaining ordering gain |
| 6 | 22–24. Time management trio | 0 | 0 | Low-medium | High for a per-move-budget engine; also UX |
| 7 | 5. Correction history | 0 | ~64–256 KiB | Low-medium | Medium-high — recent technique, strong fit for a cheap HCE |
| 8 | 8–9. LMR table + improving | 0 | ~2 KiB | Low | Medium — many small compounding gains |
| 9 | 20. Compute-at-init KPK bitbase | 0 | ~24 KiB | Low-medium | Medium — pure accuracy, zero bytes |
| 10 | 7, 10, 11, 12, 13. IIR, null-move upgrades, ProbCut, aspiration widening, root effort ordering | 0 | ~0 | Low each | Small-medium each, additive |
| 11 | 4. Capture history | 0 | 9 KiB | Low | Small, nearly free |
| 12 | 15–19. Pawn hash, safe mobility/threats, king attack units, packed scores, shuffle damping | ~0 | ~64–128 KiB | Medium (with tuner) | Medium-high combined — the HCE accuracy program |
| 13 | 30. Repetition-aware cache keys | 0 | ~0 | Low | Correctness — closes a documented gap |
| 14 | 31. SPSA tuning | 0 | 0 | Medium (offline) | Compounding — multiplies everything above |
| 15 | 25. SAB stop flag | 0 | ~0 | Low (+deploy headers) | UX/responsiveness; prerequisite-shared with threads |
| 16 | 27. Cross-worker TT seeding / ponder-lite | 0 | ~0 | Medium | Medium — unique to Mojo's two-worker design |
| 17 | 26. SIMD dual build | 2nd artifact | 0 | Low-medium | Small now; large if NNUE lands |
| 18 | 29. Binary-size squeeze | negative | 0 | Low | Direct "ultra lightweight" progress |
| 19 | 32. Tiny opening book | +8–32 KiB | ~0 | Low-medium | Optional polish + variety |
| 20 | 28. Lazy SMP threads | ~0 | +stacks/thread | High | Large on multicore at long budgets; sequence last |
| 21 | 21. Micro-NNUE | +48–200 KiB | ~2× accum. | High | Transformative accuracy; a deliberate design decision |

### Suggested sequencing logic

Three dependency chains are worth respecting. First, **tuning before terms**: build the Texel pipeline (14) before investing in new evaluation features (16–19), because untuned terms routinely test neutral-to-negative and get wrongly discarded. Second, **TT before its dependents**: in-entry static eval (1) unlocks improving (9) and cheapens correction history (5); the bucketed layout is also the version of the table you want before any sharing (27) or threading (28). Third, **headers as one decision**: the SAB stop flag (25), true shared TT (27), and threads (28) all require the same cross-origin-isolation deployment change — decide it once, then harvest all three consumers on your own schedule.

Everything in Parts 1 and 3 should go through the harness you already trust: fixed-node determinism for correctness, fixed-depth SPRT for pure-eval changes, equal-time SPRT for anything touching speed — exactly the split ENGINE.md already prescribes. The roadmap's bet, made explicit: for an engine whose constraints are bytes and browser milliseconds, the order of returns is *tuning > search shaping > evaluation knowledge > parallelism > learned evaluation*, and each stage's infrastructure makes the next stage cheaper.
