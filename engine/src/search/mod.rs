//! Iterative-deepening alpha-beta search core: negamax/quiescence, the
//! transposition table lifecycle, and the search-tuning constants that
//! govern pruning/reduction. Move ordering lives in [`ordering`], SEE in
//! [`see`], and generic move/position utilities in [`moves`].

mod correction;
mod moves;
mod ordering;
mod pawn_cache;
mod see;
mod tt;

use arrayvec::ArrayVec;
use cozy_chess::{Board, Color, Move, Piece};

use crate::eval::insufficient_material;

#[cfg(test)]
pub(crate) use moves::legal_moves;
pub(crate) use moves::{fallback, played};

use moves::{captured_value, decode_move, encode_move, is_capture, repetition_key, rule_key};
use ordering::{MovePicker, QuiescencePicker, RootMovePicker, RootMoveStat};
use see::static_exchange;
use tt::{Bound, TT_BUCKETS, TT_ENTRIES, TTBucket, TTEntry, score_from_tt, score_to_tt};

pub(crate) const MATE_SCORE: i32 = 30_000;
pub(crate) const INF: i32 = 32_000;
pub(crate) const MAX_PLY: usize = 64;
const MAX_MOVES: usize = 218;
const TIME_CHECK_INTERVAL: u64 = 256;

// --- Search tuning constants (values intentionally unchanged during
// cleanup; retuning these is a strength-tuning decision that can't be
// validated without a self-play/SPRT harness, which this repo lacks) ---

/// Half-width of the aspiration window placed around the previous
/// iteration's score.
const ASPIRATION_INITIAL_DELTA: i32 = 20;
const ASPIRATION_MAX_RETRIES: u8 = 4;
/// Null-move reduction is `NULL_MOVE_BASE_REDUCTION + depth / NULL_MOVE_DEPTH_DIVISOR`,
/// growing more conservative (smaller reduction) at higher depth.
const NULL_MOVE_BASE_REDUCTION: i16 = 2;
const NULL_MOVE_DEPTH_DIVISOR: i16 = 4;
/// Null-move pruning is disabled once the halfmove clock gets this close to
/// the 50-move rule, since a near-forced zugzwang-prone endgame is exactly
/// where null-move pruning is least safe.
const NULL_MOVE_HALFMOVE_LIMIT: u8 = 90;
/// Null-move pruning only applies from this depth onward.
const NULL_MOVE_MIN_DEPTH: i16 = 3;
const NULL_VERIFICATION_MIN_DEPTH: i16 = 10;
/// Late move reduction only applies from this depth and move index onward,
/// with a deeper reduction once both a higher depth and move index are hit.
const LMR_MIN_DEPTH: i16 = 3;
const LMR_MIN_MOVE_INDEX: usize = 3;
const LMR_TABLE_DEPTHS: usize = 32;
const LMR_TABLE_MOVES: usize = 64;
const LMR_TABLE: [[u8; LMR_TABLE_MOVES]; LMR_TABLE_DEPTHS] = build_lmr_table();
/// Continuation/main-history thresholds for pruning and LMR adjustments.
const HISTORY_PRUNE_MAX_DEPTH: i16 = 3;
const HISTORY_PRUNE_MIN_MOVE_INDEX: usize = 4;
const HISTORY_BAD: i32 = -4_000;
const HISTORY_GOOD: i32 = 4_000;
/// Quiescence delta-pruning margin added to a capture's value before
/// comparing against alpha.
const DELTA_PRUNING_MARGIN: i32 = 120;
/// Maximum number of check-extension plies applied along a single line.
const MAX_CHECK_EXTENSIONS: u8 = 2;
const SINGULAR_MIN_DEPTH: i16 = 7;
const SINGULAR_TT_DEPTH_ALLOWANCE: i16 = 3;
const SINGULAR_MARGIN_PER_PLY: i32 = 2;
const IIR_MIN_DEPTH: i16 = 4;
/// Reverse futility (static null-move) pruning: at shallow depth, if the
/// static eval already exceeds beta by this margin per ply, cut off early.
const RFP_MAX_DEPTH: i16 = 8;
const RFP_MARGIN_PER_PLY: i32 = 120;
const RFP_NOT_IMPROVING_MARGIN_REDUCTION: i32 = 40;
/// SEE pruning of clearly-losing captures in the main search (not just
/// quiescence): at shallow depth, skip a capture whose SEE falls below
/// `-SEE_PRUNE_MARGIN_PER_PLY * depth`.
const SEE_PRUNE_MAX_DEPTH: i16 = 8;
const SEE_PRUNE_MARGIN_PER_PLY: i32 = 90;
/// Futility pruning: at shallow depth, skip a quiet, non-check move if the
/// static eval plus this margin still can't reach alpha.
const FUTILITY_MAX_DEPTH: i16 = 3;
const FUTILITY_MARGIN_BASE: i32 = 100;
const FUTILITY_MARGIN_PER_PLY: i32 = 100;
/// Late move pruning: at shallow depth, stop searching further quiet moves
/// once the move count exceeds a depth-scaled threshold.
const LMP_MAX_DEPTH: i16 = 4;
const LMP_BASE_MOVE_COUNT: usize = 4;
const LMP_NOT_IMPROVING_REDUCTION: usize = 2;
/// Razoring: at shallow depth, if the static eval falls this far below alpha,
/// drop straight into quiescence and trust it (the fail-low mirror of RFP).
const RAZOR_MAX_DEPTH: i16 = 2;
const RAZOR_MARGIN_BASE: i32 = 200;
const RAZOR_MARGIN_PER_PLY: i32 = 250;
const PROBCUT_MIN_DEPTH: i16 = 5;
const PROBCUT_MARGIN: i32 = 180;
const PROBCUT_DEPTH_REDUCTION: i16 = 4;

type MoveList = ArrayVec<Move, MAX_MOVES>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SingularOutcome {
    Extend,
    MultiCut,
    Reduce,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AspirationFailure {
    Low,
    High,
}

#[derive(Debug, Clone, Copy, Default)]
struct LmrContext {
    capture: bool,
    in_check: bool,
    gives_check: bool,
    history_score: i32,
    pv_node: bool,
    cut_node: bool,
    tt_move_capture: bool,
    improving: Option<bool>,
}

#[derive(Debug)]
pub(crate) struct SearchLine {
    pub(crate) score: i32,
    pub(crate) moves: Vec<Move>,
}

#[derive(Debug)]
pub(crate) struct SearchResult {
    pub(crate) nodes: u64,
    pub(crate) root_node_fraction: f64,
    pub(crate) soft_time_fraction: f64,
    pub(crate) predicted_next_ms: f64,
    pub(crate) ebf_gate_override: bool,
    pub(crate) timed_out: bool,
    pub(crate) lines: Vec<SearchLine>,
}

pub(crate) struct SearchCore {
    table: Box<[TTBucket]>,
    killers: [[Option<Move>; 2]; MAX_PLY],
    history: [[i32; 64]; 64],
    continuation_history: Box<[i16]>,
    capture_history: Box<[i16]>,
    pawn_correction: Box<[i16]>,
    material_correction: Box<[i16]>,
    pawn_cache: Box<[pawn_cache::PawnCacheEntry]>,
    static_evals: [i16; MAX_PLY],
    root_stats: ArrayVec<RootMoveStat, MAX_MOVES>,
    countermove: [[u16; 64]; 64],
    pv: [[u16; MAX_PLY]; MAX_PLY],
    pv_len: [u8; MAX_PLY],
    prior_positions: Vec<u64>,
    path: ArrayVec<u64, MAX_PLY>,
    null_boundary: Option<usize>,
    previous_scores: Vec<i32>,
    previous_best_move: Option<Move>,
    previous_iteration_score: Option<i32>,
    stable_best_iterations: u8,
    previous_iteration_ms: Option<f64>,
    smoothed_ebf: Option<f64>,
    root_key: u64,
    generation: u8,
    nodes: u64,
    deadline_ms: f64,
    timed_out: bool,
    #[cfg(test)]
    node_limit: Option<u64>,
    #[cfg(test)]
    null_verifications: u64,
    #[cfg(test)]
    probcut_cutoffs: u64,
    #[cfg(test)]
    aspiration_retries: u64,
}

impl SearchCore {
    pub(crate) fn new() -> Self {
        crate::kpk::initialize();
        debug_assert_eq!(std::mem::size_of::<TTEntry>(), 16);
        debug_assert_eq!(std::mem::size_of::<TTBucket>(), 64);
        debug_assert_eq!(TT_ENTRIES * std::mem::size_of::<TTEntry>(), 2 * 1024 * 1024);
        Self {
            table: vec![TTBucket::default(); TT_BUCKETS].into_boxed_slice(),
            killers: [[None; 2]; MAX_PLY],
            history: [[0; 64]; 64],
            continuation_history: vec![0; ordering::CONTINUATION_HISTORY_ENTRIES]
                .into_boxed_slice(),
            capture_history: vec![0; ordering::CAPTURE_HISTORY_ENTRIES].into_boxed_slice(),
            pawn_correction: vec![0; correction::CORRECTION_HISTORY_ENTRIES].into_boxed_slice(),
            material_correction: vec![0; correction::CORRECTION_HISTORY_ENTRIES].into_boxed_slice(),
            pawn_cache: pawn_cache::empty_cache(),
            static_evals: [i16::MIN; MAX_PLY],
            root_stats: ArrayVec::new(),
            countermove: [[0; 64]; 64],
            pv: [[0; MAX_PLY]; MAX_PLY],
            pv_len: [0; MAX_PLY],
            prior_positions: Vec::new(),
            path: ArrayVec::new(),
            null_boundary: None,
            previous_scores: Vec::new(),
            previous_best_move: None,
            previous_iteration_score: None,
            stable_best_iterations: 0,
            previous_iteration_ms: None,
            smoothed_ebf: None,
            root_key: 0,
            generation: 0,
            nodes: 0,
            deadline_ms: f64::INFINITY,
            timed_out: false,
            #[cfg(test)]
            node_limit: None,
            #[cfg(test)]
            null_verifications: 0,
            #[cfg(test)]
            probcut_cutoffs: 0,
            #[cfg(test)]
            aspiration_retries: 0,
        }
    }

    pub(crate) fn set_position(&mut self, board: &Board, prior: &[Board]) {
        let root_key = repetition_key(board);
        if self.root_key != root_key {
            self.root_key = root_key;
            self.previous_scores.clear();
            self.previous_best_move = None;
            self.previous_iteration_score = None;
            self.stable_best_iterations = 0;
            self.previous_iteration_ms = None;
            self.smoothed_ebf = None;
            self.generation = self.generation.wrapping_add(1);
            for row in &mut self.history {
                for value in row {
                    *value /= 2;
                }
            }
            for value in &mut self.continuation_history {
                *value /= 2;
            }
            for value in &mut self.capture_history {
                *value /= 2;
            }
            for value in &mut self.pawn_correction {
                *value /= 2;
            }
            for value in &mut self.material_correction {
                *value /= 2;
            }
            self.killers = [[None; 2]; MAX_PLY];
            self.countermove = [[0; 64]; 64];
            self.root_stats.clear();
        }
        self.prior_positions.clear();
        self.prior_positions
            .extend(prior.iter().map(repetition_key));
    }

    pub(crate) fn analyze_depth(
        &mut self,
        board: &Board,
        depth: i16,
        multi_pv: u8,
        time_limit_ms: f64,
    ) -> SearchResult {
        let iteration_started = crate::now_ms();
        self.nodes = 0;
        self.timed_out = false;
        #[cfg(test)]
        {
            self.aspiration_retries = 0;
        }
        self.deadline_ms = crate::now_ms() + time_limit_ms.max(5.0);
        self.path.clear();
        self.path.push(repetition_key(board));
        self.static_evals.fill(i16::MIN);
        let raw_eval = self.raw_evaluate(board);
        self.static_evals[0] = compact_static_eval(self.corrected_static_eval(board, raw_eval));
        self.null_boundary = None;
        if self.is_draw(board) {
            return SearchResult {
                nodes: 0,
                root_node_fraction: 0.0,
                soft_time_fraction: 0.5,
                predicted_next_ms: 0.0,
                ebf_gate_override: false,
                timed_out: false,
                lines: Vec::new(),
            };
        }
        let mut excluded = MoveList::new();
        let mut lines = Vec::new();

        for pv_index in 0..multi_pv.clamp(1, 5) as usize {
            let previous = self.previous_scores.get(pv_index).copied();
            let (mut alpha, mut beta) = initial_aspiration_window(previous);
            let mut delta = ASPIRATION_INITIAL_DELTA;
            let mut retries = 0;
            let mut line;
            loop {
                line = self.search_root(board, depth, &excluded, alpha, beta);
                if self.timed_out {
                    break;
                }
                let Some(score) = line.as_ref().map(|result| result.score) else {
                    break;
                };
                let failure = if score <= alpha {
                    Some(AspirationFailure::Low)
                } else if score >= beta {
                    Some(AspirationFailure::High)
                } else {
                    None
                };
                let Some(failure) = failure else { break };
                if alpha == -INF && beta == INF {
                    break;
                }
                retries += 1;
                #[cfg(test)]
                {
                    self.aspiration_retries += 1;
                }
                (alpha, beta, delta) =
                    widen_aspiration(alpha, beta, score, delta, retries, failure);
            }
            let Some(line) = line else { break };
            if self.timed_out {
                break;
            }
            if let Some(first) = line.moves.first().copied() {
                excluded.push(first);
            }
            if pv_index < self.previous_scores.len() {
                self.previous_scores[pv_index] = line.score;
            } else {
                self.previous_scores.push(line.score);
            }
            lines.push(line);
        }

        let root_node_fraction = self.root_node_fraction(lines.first());
        let soft_time_fraction = self.update_time_management(
            (!self.timed_out).then(|| lines.first()).flatten(),
            root_node_fraction,
        );
        let ebf_gate_override = soft_time_fraction >= 0.8;
        let predicted_next_ms = if !self.timed_out && !lines.is_empty() {
            self.predict_next_iteration(crate::now_ms() - iteration_started)
        } else {
            0.0
        };
        SearchResult {
            nodes: self.nodes,
            root_node_fraction,
            soft_time_fraction,
            predicted_next_ms,
            ebf_gate_override,
            timed_out: self.timed_out,
            lines,
        }
    }

    fn search_root(
        &mut self,
        board: &Board,
        depth: i16,
        excluded: &[Move],
        mut alpha: i32,
        beta: i32,
    ) -> Option<SearchLine> {
        let key = rule_key(board);
        let original_alpha = alpha;
        let tt_best = self
            .probe(key)
            .and_then(|entry| self.valid_tt_move(board, entry));
        let previous_stats = self.root_stats.clone();
        let mut picker = RootMovePicker::new(board, tt_best, excluded, &previous_stats, self);
        self.pv_len[0] = 0;
        let mut best_score = -INF;
        let mut best_move = None;
        let mut index = 0;
        let mut current_stats: ArrayVec<RootMoveStat, MAX_MOVES> = ArrayVec::new();

        while let Some(mv) = picker.next() {
            if self.expired() {
                break;
            }
            let subtree_start = self.nodes;
            let child = played(board, mv);
            self.path.push(repetition_key(&child));
            let mut score = if index == 0 {
                -self.negamax(
                    &child,
                    depth - 1,
                    -beta,
                    -alpha,
                    1,
                    true,
                    true,
                    0,
                    Some(mv),
                    None,
                    None,
                )
            } else {
                let mut probe = -self.negamax(
                    &child,
                    depth - 1,
                    -alpha - 1,
                    -alpha,
                    1,
                    false,
                    true,
                    0,
                    Some(mv),
                    None,
                    None,
                );
                if probe > alpha && probe < beta && !self.timed_out {
                    probe = -self.negamax(
                        &child,
                        depth - 1,
                        -beta,
                        -alpha,
                        1,
                        true,
                        true,
                        0,
                        Some(mv),
                        None,
                        None,
                    );
                }
                probe
            };
            self.path.pop();
            if self.timed_out {
                break;
            }
            score = score.clamp(-INF, INF);
            current_stats.push(RootMoveStat {
                mv,
                score,
                nodes: self.nodes - subtree_start,
            });
            if score > best_score {
                best_score = score;
                best_move = Some(mv);
                self.update_pv(0, mv);
            }
            alpha = alpha.max(score);
            if alpha >= beta {
                break;
            }
            index += 1;
        }

        let bound = if best_score <= original_alpha {
            Bound::Upper
        } else if best_score >= beta {
            Bound::Lower
        } else {
            Bound::Exact
        };
        if excluded.is_empty() && !self.timed_out && bound == Bound::Exact {
            self.root_stats = current_stats;
        }

        best_move.map(|best_move| {
            // Only the primary root search represents the full legal move set.
            // Storing an excluded MultiPV search here would make its second- or
            // third-best move displace the principal move before the next
            // iterative-deepening step.
            if excluded.is_empty() && !self.timed_out {
                self.store(
                    key,
                    depth,
                    score_to_tt(best_score, 0),
                    bound,
                    Some(best_move),
                    None,
                );
            }
            SearchLine {
                score: best_score,
                moves: self.pv_line(0),
            }
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn negamax(
        &mut self,
        board: &Board,
        mut depth: i16,
        mut alpha: i32,
        mut beta: i32,
        ply: usize,
        pv_node: bool,
        allow_null: bool,
        extensions: u8,
        prev_move: Option<Move>,
        excluded_move: Option<Move>,
        ordering_threat: Option<Move>,
    ) -> i32 {
        self.nodes += 1;
        self.pv_len[ply.min(MAX_PLY - 1)] = 0;
        if self.expired() {
            return 0;
        }
        if ply >= MAX_PLY - 1 {
            let raw_eval = self.raw_evaluate(board);
            return self.corrected_static_eval(board, raw_eval);
        }
        if self.is_draw(board) {
            return 0;
        }
        if crate::kpk::probe(board) == Some(false) {
            return 0;
        }

        let in_check = !board.checkers().is_empty();
        let mut next_extensions = extensions;
        if in_check && depth > 0 && extensions < MAX_CHECK_EXTENSIONS {
            depth += 1;
            next_extensions += 1;
        }
        alpha = alpha.max(-MATE_SCORE + ply as i32);
        beta = beta.min(MATE_SCORE - ply as i32 - 1);
        if alpha >= beta {
            return alpha;
        }
        if depth <= 0 {
            return self.quiescence(board, alpha, beta, ply);
        }

        let key = rule_key(board);
        let original_alpha = alpha;
        let entry = excluded_move.is_none().then(|| self.probe(key)).flatten();
        let tt_best = entry.and_then(|value| self.valid_tt_move(board, value));
        if let Some(entry) = entry
            && i16::from(entry.depth) >= depth
            && !pv_node
        {
            let score = score_from_tt(i32::from(entry.score), ply);
            match entry.bound() {
                Bound::Exact => return score,
                Bound::Lower if score >= beta => return score,
                Bound::Upper if score <= alpha => return score,
                _ => {}
            }
        }

        if should_apply_iir(
            depth,
            tt_best.is_some(),
            pv_node,
            alpha,
            beta,
            excluded_move.is_some(),
        ) {
            depth -= 1;
        }

        let mut threat_move = None;
        if allow_null
            && !pv_node
            && !in_check
            && depth >= NULL_MOVE_MIN_DEPTH
            && beta.abs() < MATE_SCORE - MAX_PLY as i32
            && board.halfmove_clock() < NULL_MOVE_HALFMOVE_LIMIT
            && has_non_pawn_material(board, board.side_to_move())
            && let Some(null_board) = board.null_move()
        {
            let reduction = NULL_MOVE_BASE_REDUCTION + depth / NULL_MOVE_DEPTH_DIVISOR;
            let previous_null_boundary = self.null_boundary;
            self.null_boundary = Some(self.path.len());
            self.path.push(repetition_key(&null_board));
            let null_score = -self.negamax(
                &null_board,
                depth - 1 - reduction,
                -beta,
                -beta + 1,
                ply + 1,
                false,
                false,
                next_extensions,
                None,
                None,
                None,
            );
            let null_threat = self.null_threat(&null_board, ply + 1);
            self.path.pop();
            self.null_boundary = previous_null_boundary;
            if self.timed_out {
                return 0;
            }
            if null_score >= beta {
                if requires_null_verification(depth) {
                    #[cfg(test)]
                    {
                        self.null_verifications += 1;
                    }
                    let verification_score = self.negamax(
                        board,
                        depth - reduction,
                        beta - 1,
                        beta,
                        ply,
                        true,
                        false,
                        next_extensions,
                        prev_move,
                        None,
                        ordering_threat,
                    );
                    if self.timed_out {
                        return 0;
                    }
                    if verification_score >= beta {
                        return verification_score;
                    }
                } else {
                    return if board.generate_moves(|_| true) {
                        null_score
                    } else {
                        0
                    };
                }
            } else {
                threat_move = null_threat;
            }
        }

        let raw_static_eval = if in_check {
            None
        } else {
            Some(
                entry
                    .and_then(TTEntry::static_eval)
                    .unwrap_or_else(|| self.raw_evaluate(board)),
            )
        };
        let mut static_eval = raw_static_eval.map(|raw| self.corrected_static_eval(board, raw));
        let improving = static_eval.and_then(|eval| self.record_static_eval(ply, eval));
        if static_eval.is_none() {
            self.static_evals[ply] = i16::MIN;
        }
        if !pv_node
            && !in_check
            && excluded_move.is_none()
            && depth <= RFP_MAX_DEPTH
            && beta.abs() < MATE_SCORE - MAX_PLY as i32
        {
            let eval = *static_eval.get_or_insert_with(|| self.raw_evaluate(board));
            static_eval = Some(eval);
            if eval - rfp_margin(depth, improving) >= beta {
                return if board.generate_moves(|_| true) {
                    eval
                } else {
                    0
                };
            }
        }

        if !pv_node
            && !in_check
            && excluded_move.is_none()
            && depth <= RAZOR_MAX_DEPTH
            && beta.abs() < MATE_SCORE - MAX_PLY as i32
        {
            let eval = *static_eval.get_or_insert_with(|| self.raw_evaluate(board));
            if eval + RAZOR_MARGIN_BASE + RAZOR_MARGIN_PER_PLY * i32::from(depth) <= alpha {
                let score = self.quiescence(board, alpha, alpha + 1, ply);
                if self.timed_out {
                    return 0;
                }
                if score <= alpha {
                    return score;
                }
            }
        }

        if should_probcut(depth, beta, pv_node, in_check, excluded_move.is_some()) {
            let probcut_beta = beta + PROBCUT_MARGIN;
            let mut captures = QuiescencePicker::new(board, false, self, ply);
            while let Some((mv, see)) = captures.next() {
                if !is_capture(board, mv) || see < 0 {
                    continue;
                }
                let child = played(board, mv);
                self.path.push(repetition_key(&child));
                let score = -self.negamax(
                    &child,
                    depth - PROBCUT_DEPTH_REDUCTION,
                    -probcut_beta,
                    -probcut_beta + 1,
                    ply + 1,
                    false,
                    true,
                    next_extensions,
                    Some(mv),
                    None,
                    threat_move,
                );
                self.path.pop();
                if self.timed_out {
                    return 0;
                }
                if score >= probcut_beta {
                    #[cfg(test)]
                    {
                        self.probcut_cutoffs += 1;
                    }
                    self.record_capture_cutoff(board, mv, depth, &[mv]);
                    self.store(
                        key,
                        depth - PROBCUT_DEPTH_REDUCTION + 1,
                        score_to_tt(score, ply),
                        Bound::Lower,
                        Some(mv),
                        raw_static_eval,
                    );
                    return score;
                }
            }
        }

        let mut singular_move = None;
        let mut negative_extension_move = None;
        if let (Some(entry), Some(tt_move)) = (entry, tt_best)
            && next_extensions < MAX_CHECK_EXTENSIONS
            && singular_candidate(depth, entry.depth, entry.bound(), entry.score)
        {
            let tt_score = score_from_tt(i32::from(entry.score), ply);
            let singular_beta = tt_score - SINGULAR_MARGIN_PER_PLY * i32::from(depth);
            let exclusion_depth = (depth - 1) / 2;
            let exclusion_score = self.negamax(
                board,
                exclusion_depth,
                singular_beta - 1,
                singular_beta,
                ply,
                false,
                false,
                next_extensions,
                prev_move,
                Some(tt_move),
                ordering_threat,
            );
            if self.timed_out {
                return 0;
            }
            match singular_outcome(exclusion_score, singular_beta, tt_score, beta) {
                SingularOutcome::Extend => singular_move = Some(tt_move),
                SingularOutcome::MultiCut => return singular_beta,
                SingularOutcome::Reduce => negative_extension_move = Some(tt_move),
                SingularOutcome::None => {}
            }
        }

        let countermove =
            prev_move.and_then(|p| decode_move(self.countermove[p.from as usize][p.to as usize]));
        let cut_node = !pv_node && beta == alpha + 1;
        let tt_move_capture = tt_best.is_some_and(|mv| is_capture(board, mv));
        let mut picker = MovePicker::new(
            board,
            tt_best,
            self.killers[ply],
            countermove,
            ordering_threat,
            prev_move,
            excluded_move.as_slice(),
        );
        let mut best_score = -INF;
        let mut best_move = None;
        let mut index = 0;
        let mut legal_moves_seen = 0;
        let mut searched_quiets = MoveList::new();
        let mut searched_captures = MoveList::new();

        while let Some(mv) = picker.next(self) {
            legal_moves_seen += 1;
            let move_index = index;
            index += 1;
            let capture = is_capture(board, mv);
            let child = played(board, mv);
            let gives_check = !child.checkers().is_empty();
            let quiet = !capture && mv.promotion.is_none();
            let defends_threat = quiet
                && threat_move.is_some_and(|threat| defends_against_threat(board, &child, threat));
            let quiet_history = if quiet {
                self.quiet_history_score(board, mv, prev_move)
            } else {
                0
            };
            let capture_history = if capture {
                self.capture_history_score(board, mv)
            } else {
                0
            };

            if !pv_node
                && !in_check
                && excluded_move.is_none()
                && move_index > 0
                && !gives_check
                && !defends_threat
            {
                if capture
                    && depth <= SEE_PRUNE_MAX_DEPTH
                    && static_exchange(board, mv) < capture_see_threshold(depth, capture_history)
                {
                    continue;
                }
                if quiet && depth <= LMP_MAX_DEPTH {
                    let depth = depth as usize;
                    if move_index >= lmp_threshold(depth, improving) {
                        continue;
                    }
                }
                if quiet && history_prunable(depth, move_index, quiet_history) {
                    continue;
                }
                if quiet
                    && depth <= FUTILITY_MAX_DEPTH
                    && let Some(eval) = static_eval
                    && eval + FUTILITY_MARGIN_BASE + FUTILITY_MARGIN_PER_PLY * i32::from(depth)
                        <= alpha
                {
                    continue;
                }
            }
            if quiet {
                searched_quiets.push(mv);
            } else if capture {
                searched_captures.push(mv);
            }
            self.path.push(repetition_key(&child));
            let singular_extension =
                i16::from(Some(mv) == singular_move && next_extensions < MAX_CHECK_EXTENSIONS);
            let negative_extension = i16::from(Some(mv) == negative_extension_move);
            let full_depth = depth - 1 + singular_extension - negative_extension;
            let child_extensions = next_extensions + singular_extension as u8;
            let mut score;
            if move_index == 0 {
                score = -self.negamax(
                    &child,
                    full_depth,
                    -beta,
                    -alpha,
                    ply + 1,
                    pv_node,
                    true,
                    child_extensions,
                    Some(mv),
                    None,
                    threat_move,
                );
            } else {
                let reduction = lmr_reduction(
                    depth,
                    move_index,
                    LmrContext {
                        capture,
                        in_check,
                        gives_check,
                        history_score: quiet_history,
                        pv_node,
                        cut_node,
                        tt_move_capture,
                        improving,
                    },
                );
                score = -self.negamax(
                    &child,
                    full_depth - reduction,
                    -alpha - 1,
                    -alpha,
                    ply + 1,
                    false,
                    true,
                    child_extensions,
                    Some(mv),
                    None,
                    threat_move,
                );
                if reduction > 0 && score > alpha && !self.timed_out {
                    score = -self.negamax(
                        &child,
                        full_depth,
                        -alpha - 1,
                        -alpha,
                        ply + 1,
                        false,
                        true,
                        child_extensions,
                        Some(mv),
                        None,
                        threat_move,
                    );
                }
                if score > alpha && score < beta && !self.timed_out {
                    score = -self.negamax(
                        &child,
                        full_depth,
                        -beta,
                        -alpha,
                        ply + 1,
                        true,
                        true,
                        child_extensions,
                        Some(mv),
                        None,
                        threat_move,
                    );
                }
            }
            self.path.pop();
            if self.timed_out {
                return 0;
            }
            if score > best_score {
                best_score = score;
                best_move = Some(mv);
                self.update_pv(ply, mv);
            }
            alpha = alpha.max(score);
            if alpha >= beta {
                if capture {
                    self.record_capture_cutoff(board, mv, depth, &searched_captures);
                } else {
                    self.record_quiet_cutoff(board, mv, depth, ply, prev_move, &searched_quiets);
                }
                break;
            }
        }

        if legal_moves_seen == 0 {
            if excluded_move.is_some() {
                return original_alpha;
            }
            return terminal_score(board, ply);
        }

        let bound = if best_score <= original_alpha {
            Bound::Upper
        } else if best_score >= beta {
            Bound::Lower
        } else {
            Bound::Exact
        };
        if let Some(best_move) = best_move
            && excluded_move.is_none()
        {
            if !in_check
                && let (Some(raw_eval), Some(corrected_eval)) = (raw_static_eval, static_eval)
            {
                self.update_correction_history(
                    board,
                    raw_eval,
                    corrected_eval,
                    best_score,
                    depth,
                    bound,
                    bound == Bound::Lower && is_capture(board, best_move),
                );
            }
            self.store(
                key,
                depth,
                score_to_tt(best_score, ply),
                bound,
                Some(best_move),
                raw_static_eval,
            );
        }
        best_score
    }

    fn quiescence(&mut self, board: &Board, mut alpha: i32, beta: i32, ply: usize) -> i32 {
        self.nodes += 1;
        self.pv_len[ply.min(MAX_PLY - 1)] = 0;
        if self.expired() {
            return 0;
        }
        if ply >= MAX_PLY - 1 {
            let raw_eval = self.raw_evaluate(board);
            return self.corrected_static_eval(board, raw_eval);
        }
        if self.is_draw(board) {
            return 0;
        }
        if crate::kpk::probe(board) == Some(false) {
            return 0;
        }

        let key = rule_key(board);
        let original_alpha = alpha;
        let entry = self.probe(key);
        if let Some(entry) = entry
            && entry.depth >= 0
        {
            let score = score_from_tt(i32::from(entry.score), ply);
            match entry.bound() {
                Bound::Exact => return score,
                Bound::Lower if score >= beta => return score,
                Bound::Upper if score <= alpha => return score,
                _ => {}
            }
        }

        let in_check = !board.checkers().is_empty();
        if !board.generate_moves(|_| true) {
            return terminal_score(board, ply);
        }
        let raw_stand_pat = entry
            .and_then(TTEntry::static_eval)
            .unwrap_or_else(|| self.raw_evaluate(board));
        let stand_pat = self.corrected_static_eval(board, raw_stand_pat);
        if !in_check {
            if stand_pat >= beta {
                self.store(
                    key,
                    0,
                    score_to_tt(beta, ply),
                    Bound::Lower,
                    None,
                    Some(raw_stand_pat),
                );
                return beta;
            }
            alpha = alpha.max(stand_pat);
        }

        let mut picker = QuiescencePicker::new(board, in_check, self, ply);
        let mut best_move = None;
        while let Some((mv, see)) = picker.next() {
            let capture = is_capture(board, mv);
            let child = played(board, mv);
            let gives_check = !child.checkers().is_empty();
            if !in_check && capture && mv.promotion.is_none() && !gives_check && see < 0 {
                continue;
            }
            if !in_check
                && capture
                && mv.promotion.is_none()
                && !gives_check
                && stand_pat + captured_value(board, mv) + DELTA_PRUNING_MARGIN < alpha
            {
                continue;
            }
            self.path.push(repetition_key(&child));
            let score = -self.quiescence(&child, -beta, -alpha, ply + 1);
            self.path.pop();
            if self.timed_out {
                return 0;
            }
            if score >= beta {
                self.store(
                    key,
                    0,
                    score_to_tt(beta, ply),
                    Bound::Lower,
                    Some(mv),
                    Some(raw_stand_pat),
                );
                return beta;
            }
            if score > alpha {
                alpha = score;
                best_move = Some(mv);
                self.update_pv(ply, mv);
            }
        }
        let bound = if alpha <= original_alpha {
            Bound::Upper
        } else {
            Bound::Exact
        };
        self.store(
            key,
            0,
            score_to_tt(alpha, ply),
            bound,
            best_move,
            Some(raw_stand_pat),
        );
        alpha
    }

    fn is_draw(&self, board: &Board) -> bool {
        if board.halfmove_clock() >= 100 || insufficient_material(board) {
            return true;
        }
        let current = *self.path.last().unwrap_or(&repetition_key(board));
        // A null move is a search heuristic, not a legal move. Positions from
        // real game history or the pre-null search path therefore cannot
        // contribute to a repetition claim inside that synthetic subtree.
        let prior_matches = if self.null_boundary.is_none() {
            self.prior_positions
                .iter()
                .filter(|key| **key == current)
                .count()
        } else {
            0
        };
        let path_start = self.null_boundary.unwrap_or(0);
        prior_matches
            + self.path[path_start..]
                .iter()
                .filter(|key| **key == current)
                .count()
            >= 3
    }

    fn expired(&mut self) -> bool {
        #[cfg(test)]
        if self.node_limit.is_some_and(|limit| self.nodes >= limit) {
            self.timed_out = true;
        }
        if self.nodes.is_multiple_of(TIME_CHECK_INTERVAL) && crate::now_ms() >= self.deadline_ms {
            self.timed_out = true;
        }
        self.timed_out
    }

    fn record_static_eval(&mut self, ply: usize, eval: i32) -> Option<bool> {
        let previous = ply
            .checked_sub(2)
            .map(|previous_ply| self.static_evals[previous_ply])
            .filter(|previous| *previous != i16::MIN);
        self.static_evals[ply] = compact_static_eval(eval);
        previous.map(|previous| eval > i32::from(previous))
    }

    fn update_pv(&mut self, ply: usize, mv: Move) {
        if ply >= MAX_PLY - 1 {
            return;
        }
        self.pv[ply][0] = encode_move(mv);
        let child_len = usize::from(self.pv_len[ply + 1]).min(MAX_PLY - ply - 1);
        for index in 0..child_len {
            self.pv[ply][index + 1] = self.pv[ply + 1][index];
        }
        self.pv_len[ply] = (child_len + 1) as u8;
    }

    fn pv_line(&self, ply: usize) -> Vec<Move> {
        self.pv[ply][..usize::from(self.pv_len[ply])]
            .iter()
            .filter_map(|encoded| decode_move(*encoded))
            .collect()
    }

    fn probe(&self, key: u64) -> Option<TTEntry> {
        self.table[key as usize & (TT_BUCKETS - 1)]
            .0
            .iter()
            .copied()
            .find(|entry| entry.bound() != Bound::Empty && entry.key == key)
    }

    fn valid_tt_move(&self, board: &Board, entry: TTEntry) -> Option<Move> {
        decode_move(entry.best).filter(|mv| board.is_legal(*mv))
    }

    fn null_threat(&self, board: &Board, ply: usize) -> Option<Move> {
        let pv_move = (ply < MAX_PLY && self.pv_len[ply] > 0)
            .then(|| decode_move(self.pv[ply][0]))
            .flatten()
            .filter(|mv| board.is_legal(*mv));
        pv_move.or_else(|| {
            self.probe(rule_key(board))
                .and_then(|entry| self.valid_tt_move(board, entry))
        })
    }

    fn root_node_fraction(&self, primary: Option<&SearchLine>) -> f64 {
        let Some(best_move) = primary.and_then(|line| line.moves.first()) else {
            return 0.0;
        };
        let total: u64 = self.root_stats.iter().map(|stat| stat.nodes).sum();
        if total == 0 {
            return 0.0;
        }
        self.root_stats
            .iter()
            .find(|stat| stat.mv == *best_move)
            .map_or(0.0, |stat| stat.nodes as f64 / total as f64)
    }

    fn update_time_management(
        &mut self,
        primary: Option<&SearchLine>,
        root_node_fraction: f64,
    ) -> f64 {
        let Some(line) = primary else { return 0.5 };
        let Some(&best_move) = line.moves.first() else {
            return 0.5;
        };
        let changed = self
            .previous_best_move
            .is_some_and(|previous| previous != best_move);
        if self.previous_best_move == Some(best_move) {
            self.stable_best_iterations = self.stable_best_iterations.saturating_add(1);
        } else {
            self.stable_best_iterations = 0;
        }
        let score_dropped = self
            .previous_iteration_score
            .is_some_and(|previous| line.score < previous - 50);

        let mut fraction = 0.5_f64;
        if self.stable_best_iterations >= 2 {
            fraction *= 0.8_f64.powi(i32::from(self.stable_best_iterations - 1));
        }
        if root_node_fraction >= 0.75 {
            fraction *= 0.8;
        } else if root_node_fraction > 0.0 && root_node_fraction < 0.35 {
            fraction = fraction.max(0.7);
        }
        if changed {
            fraction = fraction.max(0.8);
        }
        if score_dropped {
            fraction = fraction.max(0.9);
        }

        self.previous_best_move = Some(best_move);
        self.previous_iteration_score = Some(line.score);
        fraction.clamp(0.25, 0.9)
    }

    fn predict_next_iteration(&mut self, elapsed_ms: f64) -> f64 {
        let Some(previous_ms) = self.previous_iteration_ms.replace(elapsed_ms) else {
            return 0.0;
        };
        if previous_ms <= 0.0 || elapsed_ms <= 0.0 {
            return 0.0;
        }
        let recent_ebf = (elapsed_ms / previous_ms).clamp(1.0, 8.0);
        let smoothed = self
            .smoothed_ebf
            .map_or(recent_ebf, |previous| 0.6 * previous + 0.4 * recent_ebf);
        self.smoothed_ebf = Some(smoothed);
        elapsed_ms * smoothed
    }

    fn store(
        &mut self,
        key: u64,
        depth: i16,
        score: i32,
        bound: Bound,
        best: Option<Move>,
        static_eval: Option<i32>,
    ) {
        let bucket = &mut self.table[key as usize & (TT_BUCKETS - 1)].0;
        let matching = bucket
            .iter()
            .position(|entry| entry.bound() != Bound::Empty && entry.key == key);
        if let Some(index) = matching
            && depth < i16::from(bucket[index].depth)
        {
            return;
        }
        let index = matching
            .or_else(|| {
                bucket
                    .iter()
                    .position(|entry| entry.bound() == Bound::Empty)
            })
            .unwrap_or_else(|| {
                let generation = self.generation & 0b11_1111;
                bucket
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, entry)| {
                        let age = generation.wrapping_sub(entry.generation()) & 0b11_1111;
                        (age, -i16::from(entry.depth))
                    })
                    .map_or(0, |(index, _)| index)
            });
        bucket[index] = TTEntry::new(
            key,
            best.map_or(0, encode_move),
            score.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
            static_eval,
            depth.clamp(i16::from(i8::MIN), i16::from(i8::MAX)) as i8,
            bound,
            self.generation,
        );
    }

    #[cfg(test)]
    pub(crate) fn set_node_limit(&mut self, limit: Option<u64>) {
        self.node_limit = limit;
    }
}

fn terminal_score(board: &Board, ply: usize) -> i32 {
    if board.checkers().is_empty() {
        0
    } else {
        -MATE_SCORE + ply as i32
    }
}

fn lmr_reduction(depth: i16, index: usize, context: LmrContext) -> i16 {
    if depth < LMR_MIN_DEPTH || index < LMR_MIN_MOVE_INDEX || context.capture || context.in_check {
        return 0;
    }
    let depth_index = usize::from(depth as u16).min(LMR_TABLE_DEPTHS - 1);
    let move_index = index.min(LMR_TABLE_MOVES - 1);
    let mut reduction = i16::from(LMR_TABLE[depth_index][move_index]);
    reduction -= i16::from(context.pv_node);
    reduction += i16::from(context.cut_node);
    reduction -= i16::from(context.gives_check);
    reduction += i16::from(context.tt_move_capture);
    reduction += i16::from(context.history_score <= HISTORY_BAD);
    reduction -= i16::from(context.history_score >= HISTORY_GOOD);
    match context.improving {
        Some(true) => reduction -= 1,
        Some(false) => reduction += 1,
        None => {}
    }
    reduction.clamp(0, depth - 1)
}

fn rfp_margin(depth: i16, improving: Option<bool>) -> i32 {
    let per_ply = if improving == Some(false) {
        RFP_MARGIN_PER_PLY - RFP_NOT_IMPROVING_MARGIN_REDUCTION
    } else {
        RFP_MARGIN_PER_PLY
    };
    per_ply * i32::from(depth)
}

fn lmp_threshold(depth: usize, improving: Option<bool>) -> usize {
    let threshold = LMP_BASE_MOVE_COUNT + depth * depth;
    if improving == Some(false) {
        threshold.saturating_sub(LMP_NOT_IMPROVING_REDUCTION)
    } else {
        threshold
    }
}

fn compact_static_eval(eval: i32) -> i16 {
    eval.clamp(i32::from(i16::MIN) + 1, i32::from(i16::MAX)) as i16
}

fn initial_aspiration_window(previous: Option<i32>) -> (i32, i32) {
    previous
        .filter(|score| score.abs() < MATE_SCORE - MAX_PLY as i32)
        .map_or((-INF, INF), |score| {
            (
                score.saturating_sub(ASPIRATION_INITIAL_DELTA).max(-INF),
                score.saturating_add(ASPIRATION_INITIAL_DELTA).min(INF),
            )
        })
}

fn widen_aspiration(
    alpha: i32,
    beta: i32,
    score: i32,
    delta: i32,
    retries: u8,
    failure: AspirationFailure,
) -> (i32, i32, i32) {
    let next_delta = delta.saturating_mul(2).min(INF);
    if retries >= ASPIRATION_MAX_RETRIES || score.abs() >= MATE_SCORE - MAX_PLY as i32 {
        return (-INF, INF, next_delta);
    }
    match failure {
        AspirationFailure::Low => (score.saturating_sub(next_delta).max(-INF), beta, next_delta),
        AspirationFailure::High => (alpha, score.saturating_add(next_delta).min(INF), next_delta),
    }
}

const fn build_lmr_table() -> [[u8; LMR_TABLE_MOVES]; LMR_TABLE_DEPTHS] {
    let mut table = [[0; LMR_TABLE_MOVES]; LMR_TABLE_DEPTHS];
    let mut depth = 1;
    while depth < LMR_TABLE_DEPTHS {
        let mut move_index = 1;
        while move_index < LMR_TABLE_MOVES {
            let product = log_scaled(depth) * log_scaled(move_index + 1);
            table[depth][move_index] = ((product + 18_022) / 36_045) as u8;
            move_index += 1;
        }
        depth += 1;
    }
    table
}

/// Fixed-point natural-log approximation, scaled by 128. Linear interpolation
/// within each power-of-two interval is sufficient for a reduction table.
const fn log_scaled(value: usize) -> usize {
    if value <= 1 {
        return 0;
    }
    let mut exponent = 0;
    let mut base = 1;
    while base * 2 <= value {
        base *= 2;
        exponent += 1;
    }
    let log2_scaled = exponent * 128 + (value - base) * 128 / base;
    log2_scaled * 89 / 128
}

fn history_prunable(depth: i16, move_index: usize, history_score: i32) -> bool {
    depth <= HISTORY_PRUNE_MAX_DEPTH
        && move_index >= HISTORY_PRUNE_MIN_MOVE_INDEX
        && history_score < HISTORY_BAD
}

fn capture_see_threshold(depth: i16, capture_history: i32) -> i32 {
    -SEE_PRUNE_MARGIN_PER_PLY * i32::from(depth) - capture_history.max(0) / 64
}

fn singular_candidate(depth: i16, tt_depth: i8, bound: Bound, stored_score: i16) -> bool {
    depth >= SINGULAR_MIN_DEPTH
        && i16::from(tt_depth) >= depth - SINGULAR_TT_DEPTH_ALLOWANCE
        && matches!(bound, Bound::Exact | Bound::Lower)
        && i32::from(stored_score).abs() < MATE_SCORE - MAX_PLY as i32
}

fn singular_outcome(
    exclusion_score: i32,
    singular_beta: i32,
    tt_score: i32,
    beta: i32,
) -> SingularOutcome {
    if exclusion_score < singular_beta {
        SingularOutcome::Extend
    } else if singular_beta >= beta {
        SingularOutcome::MultiCut
    } else if tt_score >= beta {
        SingularOutcome::Reduce
    } else {
        SingularOutcome::None
    }
}

fn should_apply_iir(
    depth: i16,
    has_tt_move: bool,
    pv_node: bool,
    alpha: i32,
    beta: i32,
    exclusion_search: bool,
) -> bool {
    depth >= IIR_MIN_DEPTH && !has_tt_move && !exclusion_search && (pv_node || beta == alpha + 1)
}

fn requires_null_verification(depth: i16) -> bool {
    depth >= NULL_VERIFICATION_MIN_DEPTH
}

fn defends_against_threat(board: &Board, child: &Board, threat: Move) -> bool {
    let Some(null_board) = board.null_move() else {
        return false;
    };
    if !null_board.is_legal(threat) {
        return false;
    }
    if !child.is_legal(threat) {
        return true;
    }
    is_capture(&null_board, threat)
        && (!is_capture(child, threat) || static_exchange(child, threat) < 0)
}

fn should_probcut(
    depth: i16,
    beta: i32,
    pv_node: bool,
    in_check: bool,
    exclusion_search: bool,
) -> bool {
    depth >= PROBCUT_MIN_DEPTH
        && !pv_node
        && !in_check
        && !exclusion_search
        && beta.abs() < MATE_SCORE - MAX_PLY as i32 - PROBCUT_MARGIN
}

fn has_non_pawn_material(board: &Board, color: Color) -> bool {
    !(board.colors(color)
        & (board.pieces(Piece::Knight)
            | board.pieces(Piece::Bishop)
            | board.pieces(Piece::Rook)
            | board.pieces(Piece::Queen)))
    .is_empty()
}

#[cfg(test)]
mod tests {
    use cozy_chess::Board;

    use super::*;

    #[test]
    fn root_table_entry_tracks_the_primary_multipv_line() {
        let board = Board::default();
        let mut search = SearchCore::new();
        search.set_position(&board, &[]);
        let result = search.analyze_depth(&board, 4, 3, 10_000.0);
        let primary = result.lines[0].moves[0];
        let entry = search.probe(rule_key(&board)).unwrap();

        assert_eq!(entry.depth, 4);
        assert_eq!(entry.bound(), Bound::Exact);
        assert_eq!(decode_move(entry.best), Some(primary));
        assert_eq!(search.root_stats.len(), legal_moves(&board).len());
        assert!(result.root_node_fraction > 0.0);
        assert!(result.root_node_fraction <= 1.0);
    }

    #[test]
    fn quiescence_reuses_depth_zero_table_entry() {
        let board = "4k3/8/8/3p4/4P3/8/8/4K3 w - - 0 1"
            .parse::<Board>()
            .unwrap();
        let mut search = SearchCore::new();
        search.path.push(repetition_key(&board));

        let first = search.quiescence(&board, -INF, INF, 0);
        let first_nodes = search.nodes;
        search.nodes = 0;
        let second = search.quiescence(&board, -INF, INF, 0);

        assert!(first_nodes > 1);
        assert_eq!(second, first);
        assert_eq!(search.nodes, 1);
    }

    #[test]
    fn continuation_history_adjusts_lmr_and_shallow_pruning() {
        assert_eq!(std::mem::size_of_val(&LMR_TABLE), 2 * 1024);
        let neutral = LmrContext::default();
        assert_eq!(lmr_reduction(6, 8, neutral), 2);
        assert_eq!(
            lmr_reduction(
                6,
                8,
                LmrContext {
                    history_score: HISTORY_GOOD,
                    ..neutral
                }
            ),
            1
        );
        assert_eq!(
            lmr_reduction(
                6,
                8,
                LmrContext {
                    history_score: HISTORY_BAD,
                    ..neutral
                }
            ),
            3
        );
        assert_eq!(
            lmr_reduction(
                6,
                8,
                LmrContext {
                    pv_node: true,
                    ..neutral
                }
            ),
            1
        );
        assert_eq!(
            lmr_reduction(
                6,
                8,
                LmrContext {
                    cut_node: true,
                    ..neutral
                }
            ),
            3
        );
        assert_eq!(
            lmr_reduction(
                6,
                8,
                LmrContext {
                    gives_check: true,
                    ..neutral
                }
            ),
            1
        );
        assert_eq!(
            lmr_reduction(
                6,
                8,
                LmrContext {
                    tt_move_capture: true,
                    ..neutral
                }
            ),
            3
        );
        assert_eq!(
            lmr_reduction(
                6,
                8,
                LmrContext {
                    improving: Some(true),
                    ..neutral
                }
            ),
            1
        );
        assert_eq!(
            lmr_reduction(
                6,
                8,
                LmrContext {
                    improving: Some(false),
                    ..neutral
                }
            ),
            3
        );
        assert!(history_prunable(3, 4, HISTORY_BAD - 1));
        assert!(!history_prunable(4, 4, HISTORY_BAD - 1));
        assert!(!history_prunable(3, 3, HISTORY_BAD - 1));
        assert!(!history_prunable(3, 4, HISTORY_BAD));
        assert_eq!(capture_see_threshold(2, 0), -180);
        assert_eq!(capture_see_threshold(2, 6_400), -280);
        assert_eq!(capture_see_threshold(2, -6_400), -180);
    }

    #[test]
    fn improving_eval_stack_tightens_pruning_when_the_position_worsens() {
        let mut search = SearchCore::new();
        assert_eq!(std::mem::size_of_val(&search.static_evals), 128);
        search.static_evals[0] = 100;
        assert_eq!(search.record_static_eval(2, 101), Some(true));
        assert_eq!(search.record_static_eval(4, 90), Some(false));
        assert_eq!(search.record_static_eval(1, 0), None);

        assert_eq!(rfp_margin(3, Some(true)), 360);
        assert_eq!(rfp_margin(3, None), 360);
        assert_eq!(rfp_margin(3, Some(false)), 240);
        assert_eq!(lmp_threshold(3, Some(true)), 13);
        assert_eq!(lmp_threshold(3, None), 13);
        assert_eq!(lmp_threshold(3, Some(false)), 11);
    }

    #[test]
    fn singular_extension_gate_requires_depth_bound_and_non_mate_score() {
        assert!(singular_candidate(7, 4, Bound::Lower, 100));
        assert!(singular_candidate(8, 8, Bound::Exact, -100));
        assert!(!singular_candidate(6, 6, Bound::Lower, 100));
        assert!(!singular_candidate(8, 4, Bound::Lower, 100));
        assert!(!singular_candidate(8, 8, Bound::Upper, 100));
        assert!(!singular_candidate(8, 8, Bound::Lower, 30_000));

        assert_eq!(singular_outcome(79, 80, 100, 90), SingularOutcome::Extend);
        assert_eq!(singular_outcome(90, 90, 100, 90), SingularOutcome::MultiCut);
        assert_eq!(singular_outcome(85, 85, 100, 90), SingularOutcome::Reduce);
        assert_eq!(singular_outcome(85, 85, 89, 90), SingularOutcome::None);
    }

    #[test]
    fn iir_only_reduces_deep_pv_or_zero_window_nodes_without_a_tt_move() {
        assert!(should_apply_iir(4, false, true, -100, 100, false));
        assert!(should_apply_iir(6, false, false, 10, 11, false));
        assert!(!should_apply_iir(3, false, true, -100, 100, false));
        assert!(!should_apply_iir(6, true, true, -100, 100, false));
        assert!(!should_apply_iir(6, false, false, -100, 100, false));
        assert!(!should_apply_iir(6, false, true, -100, 100, true));
    }

    #[test]
    fn null_verification_and_threat_defense_are_gated_correctly() {
        assert!(!requires_null_verification(9));
        assert!(requires_null_verification(10));

        let board = "r3k3/8/8/8/8/8/8/R3K3 w - - 0 1".parse::<Board>().unwrap();
        let threat = "a8a1".parse().unwrap();
        let defense = played(&board, "a1b1".parse().unwrap());
        let irrelevant = played(&board, "e1f1".parse().unwrap());
        assert!(defends_against_threat(&board, &defense, threat));
        assert!(!defends_against_threat(&board, &irrelevant, threat));
    }

    #[test]
    fn deep_null_fail_high_runs_a_real_verification_search() {
        let board = Board::default();
        let mut search = SearchCore::new();
        search.path.push(repetition_key(&board));

        search.negamax(
            &board, 11, -1_001, -1_000, 0, false, true, 0, None, None, None,
        );

        assert!(search.null_verifications > 0);
    }

    #[test]
    fn probcut_uses_a_winning_capture_and_stores_a_reduced_bound() {
        assert!(should_probcut(5, 0, false, false, false));
        assert!(!should_probcut(4, 0, false, false, false));
        assert!(!should_probcut(5, 0, true, false, false));
        assert!(!should_probcut(5, 0, false, true, false));
        assert!(!should_probcut(5, 0, false, false, true));

        let board = "7k/3q4/8/8/8/8/3R4/7K w - - 0 1".parse::<Board>().unwrap();
        let mut search = SearchCore::new();
        search.path.push(repetition_key(&board));
        let score = search.negamax(&board, 6, 99, 100, 0, false, true, 0, None, None, None);

        assert!(score >= 100 + PROBCUT_MARGIN);
        assert!(search.probcut_cutoffs > 0);
        let entry = search.probe(rule_key(&board)).unwrap();
        assert_eq!(entry.bound(), Bound::Lower);
        assert!(entry.depth < 5);
        assert_eq!(decode_move(entry.best), Some("d2d7".parse().unwrap()));
    }

    #[test]
    fn aspiration_windows_widen_only_the_failed_side_before_fallback() {
        assert_eq!(initial_aspiration_window(Some(100)), (80, 120));
        assert_eq!(initial_aspiration_window(Some(30_000)), (-INF, INF));
        assert_eq!(
            widen_aspiration(80, 120, 120, 20, 1, AspirationFailure::High),
            (80, 160, 40)
        );
        assert_eq!(
            widen_aspiration(80, 120, 80, 20, 1, AspirationFailure::Low),
            (40, 120, 40)
        );
        assert_eq!(
            widen_aspiration(80, 120, 120, 80, 4, AspirationFailure::High),
            (-INF, INF, 160)
        );

        let board = Board::default();
        let mut search = SearchCore::new();
        search.set_position(&board, &[]);
        search.previous_scores.push(1_000);
        let result = search.analyze_depth(&board, 2, 1, 10_000.0);
        assert!(!result.timed_out);
        assert_eq!(result.lines.len(), 1);
        assert!(search.aspiration_retries > 0);
    }

    #[test]
    fn null_move_boundary_excludes_real_history_from_repetition() {
        let board = Board::default().null_move().unwrap();
        let key = repetition_key(&board);
        let mut search = SearchCore::new();
        search.prior_positions = vec![key, key];
        search.path.push(repetition_key(&Board::default()));
        search.null_boundary = Some(search.path.len());
        search.path.push(key);

        assert!(!search.is_draw(&board));
        search.null_boundary = None;
        assert!(search.is_draw(&board));
    }

    #[test]
    fn quiescence_does_not_delta_prune_capture_promotion() {
        // The knight blocks g8, so gxh8=Q is White's only promotion. Counting
        // only the captured rook makes delta pruning miss the new queen.
        let board = "k5nr/6P1/2K5/8/8/8/8/8 w - - 0 1".parse::<Board>().unwrap();
        let mut search = SearchCore::new();
        search.path.push(repetition_key(&board));
        let alpha = 400;

        assert!(search.quiescence(&board, alpha, 1_000, 0) > alpha);
    }

    #[test]
    fn forward_pruning_does_not_miss_stalemate() {
        // The pinned knight cannot uncover the h-file rook, while the king's
        // two flight squares are covered by White's king.
        let board = "7k/5K1n/8/8/8/8/8/7R b - - 0 1".parse::<Board>().unwrap();
        let mut search = SearchCore::new();
        search.path.push(repetition_key(&board));

        assert!(legal_moves(&board).is_empty());
        assert!(board.checkers().is_empty());
        assert_eq!(
            search.negamax(
                &board, 3, -1_001, -1_000, 0, false, true, 0, None, None, None,
            ),
            0
        );
    }

    #[test]
    fn soft_time_fraction_responds_to_stability_effort_and_score_drops() {
        let first = SearchLine {
            score: 100,
            moves: vec!["e2e4".parse().unwrap()],
        };
        let changed = SearchLine {
            score: 100,
            moves: vec!["d2d4".parse().unwrap()],
        };
        let dropped = SearchLine {
            score: 0,
            moves: changed.moves.clone(),
        };
        let mut search = SearchCore::new();
        assert_eq!(search.update_time_management(Some(&first), 0.5), 0.5);
        assert_eq!(search.update_time_management(Some(&first), 0.8), 0.4);
        assert!((search.update_time_management(Some(&first), 0.8) - 0.32).abs() < f64::EPSILON);
        assert_eq!(search.update_time_management(Some(&changed), 0.5), 0.8);
        assert_eq!(search.update_time_management(Some(&dropped), 0.5), 0.9);

        let mut scattered = SearchCore::new();
        assert_eq!(scattered.update_time_management(Some(&first), 0.2), 0.7);
    }

    #[test]
    fn iteration_prediction_smooths_and_bounds_effective_branching_factor() {
        let mut search = SearchCore::new();
        assert_eq!(search.predict_next_iteration(10.0), 0.0);
        assert_eq!(search.predict_next_iteration(20.0), 40.0);
        assert_eq!(search.predict_next_iteration(40.0), 80.0);
        // A single 10x outlier is clamped to 8x and blended 40/60 with the
        // prior 2x estimate: 0.6*2 + 0.4*8 = 4.4.
        assert!((search.predict_next_iteration(400.0) - 1_760.0).abs() < 1.0e-9);
    }
}
