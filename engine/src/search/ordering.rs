//! Move ordering: TT/capture/killer/countermove/history scoring, plus the
//! quiet-move cutoff bookkeeping (killers, history gravity, countermove
//! table) that feeds it back.

use std::cmp::Reverse;

use arrayvec::ArrayVec;
use cozy_chess::{Board, Move};

use crate::eval::piece_value;

use super::moves::{captured_value, encode_move, is_capture};
use super::see::static_exchange;
use super::{MAX_MOVES, MAX_PLY, MoveList, SearchCore};

impl SearchCore {
    pub(crate) fn order_moves(
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
    pub(crate) fn order_quiescence_moves(
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

    pub(crate) fn record_quiet_cutoff(
        &mut self,
        mv: Move,
        depth: i16,
        ply: usize,
        prev_move: Option<Move>,
    ) {
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
}

#[cfg(test)]
mod tests {
    use cozy_chess::Board;

    use super::SearchCore;

    #[test]
    fn countermove_table_populates_after_a_quiet_cutoff() {
        let board = Board::default();
        let mut search = SearchCore::new();
        search.set_position(&board, &[]);
        search.analyze_depth(&board, 4, 1, 10_000.0);
        assert!(search.countermove.iter().flatten().any(|&entry| entry != 0));
    }
}
