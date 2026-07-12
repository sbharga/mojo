use std::cmp::Reverse;

use arrayvec::ArrayVec;
use cozy_chess::{
    BitBoard, Board, Color, Move, Piece, Rank, Square, get_bishop_moves, get_king_moves,
    get_knight_moves, get_pawn_attacks, get_rook_moves,
};

use crate::eval::{evaluate, insufficient_material, piece_value};

pub(crate) const MATE_SCORE: i32 = 30_000;
pub(crate) const INF: i32 = 32_000;
pub(crate) const MAX_PLY: usize = 64;
const MAX_MOVES: usize = 218;
const TT_ENTRIES: usize = 1 << 17;
const TIME_CHECK_INTERVAL: u64 = 256;

// --- Search tuning constants (values intentionally unchanged during
// cleanup; retuning these is a strength-tuning decision that can't be
// validated without a self-play/SPRT harness, which this repo lacks) ---

/// Half-width of the aspiration window placed around the previous
/// iteration's score.
const ASPIRATION_WINDOW: i32 = 35;
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
/// Late move reduction only applies from this depth and move index onward,
/// with a deeper reduction once both a higher depth and move index are hit.
const LMR_MIN_DEPTH: i16 = 3;
const LMR_MIN_MOVE_INDEX: usize = 3;
const LMR_DEEP_DEPTH: i16 = 6;
const LMR_DEEP_MOVE_INDEX: usize = 8;
/// Quiescence delta-pruning margin added to a capture's value before
/// comparing against alpha.
const DELTA_PRUNING_MARGIN: i32 = 120;
/// Maximum number of check-extension plies applied along a single line.
const MAX_CHECK_EXTENSIONS: u8 = 2;
/// Reverse futility (static null-move) pruning: at shallow depth, if the
/// static eval already exceeds beta by this margin per ply, cut off early.
const RFP_MAX_DEPTH: i16 = 8;
const RFP_MARGIN_PER_PLY: i32 = 120;
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
/// Razoring: at shallow depth, if the static eval falls this far below alpha,
/// drop straight into quiescence and trust it (the fail-low mirror of RFP).
const RAZOR_MAX_DEPTH: i16 = 2;
const RAZOR_MARGIN_BASE: i32 = 200;
const RAZOR_MARGIN_PER_PLY: i32 = 250;

type MoveList = ArrayVec<Move, MAX_MOVES>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Bound {
    Empty,
    Exact,
    Lower,
    Upper,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct TTEntry {
    key: u64,
    best: u16,
    score: i16,
    depth: i8,
    bound: Bound,
    generation: u8,
    padding: u8,
}

impl Default for TTEntry {
    fn default() -> Self {
        Self {
            key: 0,
            best: 0,
            score: 0,
            depth: -1,
            bound: Bound::Empty,
            generation: 0,
            padding: 0,
        }
    }
}

#[derive(Debug)]
pub(crate) struct SearchLine {
    pub(crate) score: i32,
    pub(crate) moves: Vec<Move>,
}

#[derive(Debug)]
pub(crate) struct SearchResult {
    pub(crate) nodes: u64,
    pub(crate) timed_out: bool,
    pub(crate) lines: Vec<SearchLine>,
}

pub(crate) struct SearchCore {
    table: Box<[TTEntry]>,
    killers: [[Option<Move>; 2]; MAX_PLY],
    history: [[i32; 64]; 64],
    countermove: [[u16; 64]; 64],
    pv: [[u16; MAX_PLY]; MAX_PLY],
    pv_len: [u8; MAX_PLY],
    prior_positions: Vec<u64>,
    path: ArrayVec<u64, MAX_PLY>,
    null_boundary: Option<usize>,
    previous_scores: Vec<i32>,
    root_key: u64,
    generation: u8,
    nodes: u64,
    deadline_ms: f64,
    timed_out: bool,
    #[cfg(test)]
    node_limit: Option<u64>,
}

impl SearchCore {
    pub(crate) fn new() -> Self {
        debug_assert_eq!(std::mem::size_of::<TTEntry>(), 16);
        Self {
            table: vec![TTEntry::default(); TT_ENTRIES].into_boxed_slice(),
            killers: [[None; 2]; MAX_PLY],
            history: [[0; 64]; 64],
            countermove: [[0; 64]; 64],
            pv: [[0; MAX_PLY]; MAX_PLY],
            pv_len: [0; MAX_PLY],
            prior_positions: Vec::new(),
            path: ArrayVec::new(),
            null_boundary: None,
            previous_scores: Vec::new(),
            root_key: 0,
            generation: 0,
            nodes: 0,
            deadline_ms: f64::INFINITY,
            timed_out: false,
            #[cfg(test)]
            node_limit: None,
        }
    }

    pub(crate) fn set_position(&mut self, board: &Board, prior: &[Board]) {
        let root_key = repetition_key(board);
        if self.root_key != root_key {
            self.root_key = root_key;
            self.previous_scores.clear();
            self.generation = self.generation.wrapping_add(1);
            for row in &mut self.history {
                for value in row {
                    *value /= 2;
                }
            }
            self.killers = [[None; 2]; MAX_PLY];
            self.countermove = [[0; 64]; 64];
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
        self.nodes = 0;
        self.timed_out = false;
        self.deadline_ms = crate::now_ms() + time_limit_ms.max(5.0);
        self.path.clear();
        self.path.push(repetition_key(board));
        self.null_boundary = None;
        if self.is_draw(board) {
            return SearchResult {
                nodes: 0,
                timed_out: false,
                lines: Vec::new(),
            };
        }
        let mut excluded = MoveList::new();
        let mut lines = Vec::new();

        for pv_index in 0..multi_pv.clamp(1, 5) as usize {
            let previous = self.previous_scores.get(pv_index).copied();
            let (mut alpha, mut beta) = previous.map_or((-INF, INF), |score| {
                (
                    score.saturating_sub(ASPIRATION_WINDOW),
                    score.saturating_add(ASPIRATION_WINDOW),
                )
            });
            let mut line = self.search_root(board, depth, &excluded, alpha, beta);
            if !self.timed_out
                && line
                    .as_ref()
                    .is_some_and(|result| result.score <= alpha || result.score >= beta)
            {
                alpha = -INF;
                beta = INF;
                line = self.search_root(board, depth, &excluded, alpha, beta);
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

        SearchResult {
            nodes: self.nodes,
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
        let mut moves = legal_moves(board);
        moves.retain(|mv| !excluded.contains(mv));
        if moves.is_empty() {
            return None;
        }
        let tt_best = self.probe(key).and_then(|entry| decode_move(entry.best));
        self.order_moves(board, &mut moves, tt_best, None, 0);
        self.pv_len[0] = 0;
        let mut best_score = -INF;
        let mut best_move = None;

        for (index, mv) in moves.into_iter().enumerate() {
            if self.expired() {
                break;
            }
            let child = played(board, mv);
            self.path.push(repetition_key(&child));
            let mut score = if index == 0 {
                -self.negamax(&child, depth - 1, -beta, -alpha, 1, true, true, 0, Some(mv))
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
                );
                if probe > alpha && probe < beta && !self.timed_out {
                    probe =
                        -self.negamax(&child, depth - 1, -beta, -alpha, 1, true, true, 0, Some(mv));
                }
                probe
            };
            self.path.pop();
            if self.timed_out {
                break;
            }
            score = score.clamp(-INF, INF);
            if score > best_score {
                best_score = score;
                best_move = Some(mv);
                self.update_pv(0, mv);
            }
            alpha = alpha.max(score);
            if alpha >= beta {
                break;
            }
        }

        best_move.map(|best_move| {
            // Only the primary root search represents the full legal move set.
            // Storing an excluded MultiPV search here would make its second- or
            // third-best move displace the principal move before the next
            // iterative-deepening step.
            if excluded.is_empty() && !self.timed_out {
                let bound = if best_score <= original_alpha {
                    Bound::Upper
                } else if best_score >= beta {
                    Bound::Lower
                } else {
                    Bound::Exact
                };
                self.store(key, depth, score_to_tt(best_score, 0), bound, best_move);
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
    ) -> i32 {
        self.nodes += 1;
        self.pv_len[ply.min(MAX_PLY - 1)] = 0;
        if self.expired() {
            return 0;
        }
        if ply >= MAX_PLY - 1 {
            return evaluate(board);
        }
        if self.is_draw(board) {
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
        let entry = self.probe(key);
        let tt_best = entry.and_then(|value| decode_move(value.best));
        if let Some(entry) = entry
            && i16::from(entry.depth) >= depth
            && !pv_node
        {
            let score = score_from_tt(i32::from(entry.score), ply);
            match entry.bound {
                Bound::Exact => return score,
                Bound::Lower if score >= beta => return score,
                Bound::Upper if score <= alpha => return score,
                _ => {}
            }
        }

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
            let score = -self.negamax(
                &null_board,
                depth - 1 - reduction,
                -beta,
                -beta + 1,
                ply + 1,
                false,
                false,
                next_extensions,
                None,
            );
            self.path.pop();
            self.null_boundary = previous_null_boundary;
            if self.timed_out {
                return 0;
            }
            if score >= beta {
                return if board.generate_moves(|_| true) {
                    score
                } else {
                    0
                };
            }
        }

        let mut static_eval = None;
        if !pv_node
            && !in_check
            && depth <= RFP_MAX_DEPTH
            && beta.abs() < MATE_SCORE - MAX_PLY as i32
        {
            let eval = evaluate(board);
            static_eval = Some(eval);
            if eval - RFP_MARGIN_PER_PLY * i32::from(depth) >= beta {
                return if board.generate_moves(|_| true) {
                    eval
                } else {
                    0
                };
            }
        }

        if !pv_node
            && !in_check
            && depth <= RAZOR_MAX_DEPTH
            && beta.abs() < MATE_SCORE - MAX_PLY as i32
        {
            let eval = *static_eval.get_or_insert_with(|| evaluate(board));
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

        let mut moves = legal_moves(board);
        if moves.is_empty() {
            return terminal_score(board, ply);
        }
        let countermove =
            prev_move.and_then(|p| decode_move(self.countermove[p.from as usize][p.to as usize]));
        self.order_moves(board, &mut moves, tt_best, countermove, ply);
        let mut best_score = -INF;
        let mut best_move = None;

        for (index, mv) in moves.into_iter().enumerate() {
            let capture = is_capture(board, mv);
            let child = played(board, mv);
            let gives_check = !child.checkers().is_empty();
            let quiet = !capture && mv.promotion.is_none();

            if !pv_node && !in_check && index > 0 && !gives_check {
                if capture
                    && depth <= SEE_PRUNE_MAX_DEPTH
                    && static_exchange(board, mv) < -SEE_PRUNE_MARGIN_PER_PLY * i32::from(depth)
                {
                    continue;
                }
                if quiet && depth <= LMP_MAX_DEPTH {
                    let depth = depth as usize;
                    if index >= LMP_BASE_MOVE_COUNT + depth * depth {
                        continue;
                    }
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

            self.path.push(repetition_key(&child));
            let mut score;
            if index == 0 {
                score = -self.negamax(
                    &child,
                    depth - 1,
                    -beta,
                    -alpha,
                    ply + 1,
                    pv_node,
                    true,
                    next_extensions,
                    Some(mv),
                );
            } else {
                let reduction = lmr_reduction(depth, index, capture, in_check, gives_check);
                score = -self.negamax(
                    &child,
                    depth - 1 - reduction,
                    -alpha - 1,
                    -alpha,
                    ply + 1,
                    false,
                    true,
                    next_extensions,
                    Some(mv),
                );
                if reduction > 0 && score > alpha && !self.timed_out {
                    score = -self.negamax(
                        &child,
                        depth - 1,
                        -alpha - 1,
                        -alpha,
                        ply + 1,
                        false,
                        true,
                        next_extensions,
                        Some(mv),
                    );
                }
                if score > alpha && score < beta && !self.timed_out {
                    score = -self.negamax(
                        &child,
                        depth - 1,
                        -beta,
                        -alpha,
                        ply + 1,
                        true,
                        true,
                        next_extensions,
                        Some(mv),
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
                if !capture {
                    self.record_quiet_cutoff(mv, depth, ply, prev_move);
                }
                break;
            }
        }

        let bound = if best_score <= original_alpha {
            Bound::Upper
        } else if best_score >= beta {
            Bound::Lower
        } else {
            Bound::Exact
        };
        if let Some(best_move) = best_move {
            self.store(key, depth, score_to_tt(best_score, ply), bound, best_move);
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
            return evaluate(board);
        }
        if self.is_draw(board) {
            return 0;
        }

        let in_check = !board.checkers().is_empty();
        let all_moves = legal_moves(board);
        if all_moves.is_empty() {
            return terminal_score(board, ply);
        }
        let stand_pat = evaluate(board);
        if !in_check {
            if stand_pat >= beta {
                return beta;
            }
            alpha = alpha.max(stand_pat);
        }

        let mut tactical = MoveList::new();
        tactical.extend(
            all_moves
                .into_iter()
                .filter(|mv| in_check || is_capture(board, *mv) || mv.promotion.is_some()),
        );
        let see_values = self.order_quiescence_moves(board, &mut tactical, ply);

        for (mv, see) in tactical.into_iter().zip(see_values) {
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
                return beta;
            }
            if score > alpha {
                alpha = score;
                self.update_pv(ply, mv);
            }
        }
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

    fn order_moves(
        &self,
        board: &Board,
        moves: &mut [Move],
        tt_best: Option<Move>,
        countermove: Option<Move>,
        ply: usize,
    ) {
        // `sort_unstable_by_key` may evaluate its key function more than once
        // per element. SEE is much more expensive than comparing an integer,
        // so cache every move's ordering score before sorting.
        let mut scored: ArrayVec<(i32, Move), MAX_MOVES> = moves
            .iter()
            .map(|&mv| {
                (
                    self.move_order_score(board, mv, tt_best, countermove, ply),
                    mv,
                )
            })
            .collect();
        scored.sort_unstable_by_key(|&(score, _)| Reverse(score));
        for (slot, (_, mv)) in moves.iter_mut().zip(scored) {
            *slot = mv;
        }
    }

    fn move_order_score(
        &self,
        board: &Board,
        mv: Move,
        tt_best: Option<Move>,
        countermove: Option<Move>,
        ply: usize,
    ) -> i32 {
        if Some(mv) == tt_best {
            10_000_000
        } else if is_capture(board, mv) {
            1_000_000 + static_exchange(board, mv) * 32 + captured_value(board, mv)
        } else if mv.promotion.is_some() {
            950_000 + mv.promotion.map_or(0, piece_value)
        } else if ply < MAX_PLY && self.killers[ply].contains(&Some(mv)) {
            900_000
        } else if Some(mv) == countermove {
            890_000
        } else {
            self.history[mv.from as usize][mv.to as usize]
        }
    }

    /// Like `order_moves`, but for the tactical-only move list used in
    /// `quiescence`: it also returns each move's SEE value (meaningful only
    /// for captures) so the caller doesn't need to recompute
    /// `static_exchange` a second time for its own pruning decisions.
    fn order_quiescence_moves(
        &self,
        board: &Board,
        moves: &mut MoveList,
        ply: usize,
    ) -> ArrayVec<i32, MAX_MOVES> {
        let mut scored: ArrayVec<(i32, i32, Move), MAX_MOVES> = moves
            .iter()
            .map(|&mv| {
                let see = if is_capture(board, mv) {
                    static_exchange(board, mv)
                } else {
                    0
                };
                let sort_score = if is_capture(board, mv) {
                    1_000_000 + see * 32 + captured_value(board, mv)
                } else if mv.promotion.is_some() {
                    950_000 + mv.promotion.map_or(0, piece_value)
                } else if ply < MAX_PLY && self.killers[ply].contains(&Some(mv)) {
                    900_000
                } else {
                    self.history[mv.from as usize][mv.to as usize]
                };
                (sort_score, see, mv)
            })
            .collect();
        scored.sort_unstable_by_key(|&(score, ..)| Reverse(score));
        moves.clear();
        let mut see_values = ArrayVec::new();
        for (_, see, mv) in scored {
            moves.push(mv);
            see_values.push(see);
        }
        see_values
    }

    fn record_quiet_cutoff(&mut self, mv: Move, depth: i16, ply: usize, prev_move: Option<Move>) {
        if ply < MAX_PLY {
            if self.killers[ply][0] != Some(mv) {
                self.killers[ply][1] = self.killers[ply][0];
                self.killers[ply][0] = Some(mv);
            }
            let bonus = i32::from(depth).pow(2).min(2048);
            let value = &mut self.history[mv.from as usize][mv.to as usize];
            *value += bonus - (*value * bonus / 16_384);
        }
        if let Some(prev) = prev_move {
            self.countermove[prev.from as usize][prev.to as usize] = encode_move(mv);
        }
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
        let entry = self.table[key as usize & (TT_ENTRIES - 1)];
        (entry.bound != Bound::Empty && entry.key == key).then_some(entry)
    }

    fn store(&mut self, key: u64, depth: i16, score: i32, bound: Bound, best: Move) {
        let slot = &mut self.table[key as usize & (TT_ENTRIES - 1)];
        let replace = slot.bound == Bound::Empty
            || slot.generation != self.generation
            || depth >= i16::from(slot.depth);
        if replace {
            *slot = TTEntry {
                key,
                best: encode_move(best),
                score: score.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
                depth: depth.clamp(i16::from(i8::MIN), i16::from(i8::MAX)) as i8,
                bound,
                generation: self.generation,
                padding: 0,
            };
        }
    }

    #[cfg(test)]
    pub(crate) fn set_node_limit(&mut self, limit: Option<u64>) {
        self.node_limit = limit;
    }
}

pub(crate) fn legal_moves(board: &Board) -> MoveList {
    let mut moves = MoveList::new();
    board.generate_moves(|piece_moves| {
        moves.extend(piece_moves);
        false
    });
    moves
}

pub(crate) fn played(board: &Board, mv: Move) -> Board {
    let mut next = board.clone();
    next.play(mv);
    next
}

pub(crate) fn fallback(board: &Board) -> Option<Move> {
    let mut scored: ArrayVec<(i32, Move), MAX_MOVES> = legal_moves(board)
        .into_iter()
        .map(|mv| {
            let child = played(board, mv);
            let score = if child.halfmove_clock() >= 100 || insufficient_material(&child) {
                0
            } else if legal_moves(&child).is_empty() {
                if child.checkers().is_empty() {
                    0
                } else {
                    -MATE_SCORE
                }
            } else {
                evaluate(&child)
            };
            (score, mv)
        })
        .collect();
    scored.sort_unstable_by_key(|&(score, _)| score);
    scored.first().map(|&(_, mv)| mv)
}

fn terminal_score(board: &Board, ply: usize) -> i32 {
    if board.checkers().is_empty() {
        0
    } else {
        -MATE_SCORE + ply as i32
    }
}

fn is_capture(board: &Board, mv: Move) -> bool {
    board.piece_on(mv.to).is_some()
        || (board.piece_on(mv.from) == Some(Piece::Pawn) && mv.from.file() != mv.to.file())
}

fn captured_value(board: &Board, mv: Move) -> i32 {
    board.piece_on(mv.to).map_or_else(
        || {
            if is_capture(board, mv) {
                piece_value(Piece::Pawn)
            } else {
                0
            }
        },
        piece_value,
    )
}

fn lmr_reduction(
    depth: i16,
    index: usize,
    capture: bool,
    in_check: bool,
    gives_check: bool,
) -> i16 {
    if depth < LMR_MIN_DEPTH || index < LMR_MIN_MOVE_INDEX || capture || in_check || gives_check {
        0
    } else if depth >= LMR_DEEP_DEPTH && index >= LMR_DEEP_MOVE_INDEX {
        2
    } else {
        1
    }
}

fn has_non_pawn_material(board: &Board, color: Color) -> bool {
    !(board.colors(color)
        & (board.pieces(Piece::Knight)
            | board.pieces(Piece::Bishop)
            | board.pieces(Piece::Rook)
            | board.pieces(Piece::Queen)))
    .is_empty()
}

fn repetition_key(board: &Board) -> u64 {
    let Some(ep_file) = board.en_passant() else {
        return board.hash_without_ep();
    };
    let side = board.side_to_move();
    let ep_square = Square::new(ep_file, Rank::Third.relative_to(!side));
    let candidates = get_pawn_attacks(ep_square, !side) & board.colored_pieces(side, Piece::Pawn);
    let legal_ep = candidates.into_iter().any(|from| {
        board.is_legal(Move {
            from,
            to: ep_square,
            promotion: None,
        })
    });
    if legal_ep {
        board.hash()
    } else {
        board.hash_without_ep()
    }
}

fn rule_key(board: &Board) -> u64 {
    repetition_key(board) ^ u64::from(board.halfmove_clock()).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

fn score_to_tt(score: i32, ply: usize) -> i32 {
    if score >= MATE_SCORE - MAX_PLY as i32 {
        score + ply as i32
    } else if score <= -MATE_SCORE + MAX_PLY as i32 {
        score - ply as i32
    } else {
        score
    }
}

fn score_from_tt(score: i32, ply: usize) -> i32 {
    if score >= MATE_SCORE - MAX_PLY as i32 {
        score - ply as i32
    } else if score <= -MATE_SCORE + MAX_PLY as i32 {
        score + ply as i32
    } else {
        score
    }
}

fn encode_move(mv: Move) -> u16 {
    let promotion = match mv.promotion {
        None => 0,
        Some(Piece::Knight) => 1,
        Some(Piece::Bishop) => 2,
        Some(Piece::Rook) => 3,
        Some(Piece::Queen) => 4,
        Some(Piece::Pawn | Piece::King) => 0,
    };
    (mv.from as u16) | ((mv.to as u16) << 6) | (promotion << 12)
}

fn decode_move(encoded: u16) -> Option<Move> {
    if encoded == 0 {
        return None;
    }
    let value = encoded;
    let promotion = match (value >> 12) & 0x7 {
        0 => None,
        1 => Some(Piece::Knight),
        2 => Some(Piece::Bishop),
        3 => Some(Piece::Rook),
        4 => Some(Piece::Queen),
        _ => return None,
    };
    Some(Move {
        from: Square::ALL[usize::from(value & 0x3f)],
        to: Square::ALL[usize::from((value >> 6) & 0x3f)],
        promotion,
    })
}

fn attackers_to(board: &Board, target: Square, color: Color, occupied: BitBoard) -> BitBoard {
    let pieces = board.colors(color) & occupied;
    (get_pawn_attacks(target, !color) & pieces & board.pieces(Piece::Pawn))
        | (get_knight_moves(target) & pieces & board.pieces(Piece::Knight))
        | (get_king_moves(target) & pieces & board.pieces(Piece::King))
        | (get_bishop_moves(target, occupied)
            & pieces
            & (board.pieces(Piece::Bishop) | board.pieces(Piece::Queen)))
        | (get_rook_moves(target, occupied)
            & pieces
            & (board.pieces(Piece::Rook) | board.pieces(Piece::Queen)))
}

fn static_exchange(board: &Board, mv: Move) -> i32 {
    if !is_capture(board, mv) {
        return 0;
    }
    let mut gains = [0_i32; 32];
    gains[0] = captured_value(board, mv)
        + mv.promotion
            .map_or(0, |piece| piece_value(piece) - piece_value(Piece::Pawn));
    let mut occupied = board.occupied() & !mv.from.bitboard();
    if board.piece_on(mv.to).is_none() {
        let captured = Square::new(mv.to.file(), mv.from.rank());
        occupied &= !captured.bitboard();
    }
    occupied |= mv.to.bitboard();
    let mut side = !board.side_to_move();
    let mut target_piece = mv
        .promotion
        .unwrap_or_else(|| board.piece_on(mv.from).unwrap_or(Piece::Pawn));
    let mut depth = 0;

    while depth + 1 < gains.len() {
        let attackers = attackers_to(board, mv.to, side, occupied);
        let Some((piece, from)) = Piece::ALL.into_iter().find_map(|piece| {
            (attackers & board.pieces(piece))
                .into_iter()
                .next()
                .map(|square| (piece, square))
        }) else {
            break;
        };
        depth += 1;
        gains[depth] = piece_value(target_piece) - gains[depth - 1];
        target_piece = piece;
        occupied &= !from.bitboard();
        side = !side;
    }
    while depth > 0 {
        gains[depth - 1] = -(-gains[depth - 1]).max(gains[depth]);
        depth -= 1;
    }
    gains[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_encoding_round_trips() {
        for text in ["e2e4", "e7e8q", "e1h1"] {
            let mv = text.parse::<Move>().unwrap();
            assert_eq!(decode_move(encode_move(mv)), Some(mv));
        }
    }

    #[test]
    fn tt_entry_stays_compact() {
        assert_eq!(std::mem::size_of::<TTEntry>(), 16);
    }

    #[test]
    fn countermove_table_populates_after_a_quiet_cutoff() {
        let board = Board::default();
        let mut search = SearchCore::new();
        search.set_position(&board, &[]);
        search.analyze_depth(&board, 4, 1, 10_000.0);
        assert!(search.countermove.iter().flatten().any(|&entry| entry != 0));
    }

    #[test]
    fn store_keeps_deeper_entry_on_same_key_shallow_rewrite() {
        let mut search = SearchCore::new();
        let key = 0x1234_5678_9abc_def0;
        let mv = "e2e4".parse::<Move>().unwrap();
        search.store(key, 10, 0, Bound::Exact, mv);
        search.store(key, 2, 0, Bound::Exact, mv);
        assert_eq!(search.probe(key).unwrap().depth, 10);
    }

    #[test]
    fn static_exchange_distinguishes_winning_and_losing_captures() {
        let winning = "7k/3q4/8/8/8/8/3R4/7K w - - 0 1".parse::<Board>().unwrap();
        assert!(static_exchange(&winning, "d2d7".parse().unwrap()) > 0);

        let losing = "3q3k/3p4/8/8/8/8/8/3Q3K w - - 0 1"
            .parse::<Board>()
            .unwrap();
        assert!(static_exchange(&losing, "d1d7".parse().unwrap()) < 0);
    }

    #[test]
    fn root_table_entry_tracks_the_primary_multipv_line() {
        let board = Board::default();
        let mut search = SearchCore::new();
        search.set_position(&board, &[]);
        let result = search.analyze_depth(&board, 4, 3, 10_000.0);
        let primary = result.lines[0].moves[0];
        let entry = search.probe(rule_key(&board)).unwrap();

        assert_eq!(entry.depth, 4);
        assert_eq!(entry.bound, Bound::Exact);
        assert_eq!(decode_move(entry.best), Some(primary));
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
    fn fallback_prefers_immediate_checkmate() {
        let board = "7k/5Q2/6K1/8/8/8/8/8 w - - 0 1".parse::<Board>().unwrap();
        let child = played(&board, fallback(&board).unwrap());

        assert!(legal_moves(&child).is_empty());
        assert!(!child.checkers().is_empty());
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
            search.negamax(&board, 3, -1_001, -1_000, 0, false, true, 0, None),
            0
        );
    }
}
