//! Search-local position state updated in lockstep with `cozy_chess::Board`.

use cozy_chess::{Board, Move};

use crate::eval::{EvalAccumulator, MoveFacts};

use super::correction::CorrectionAccumulator;
use super::moves::repetition_key;

#[derive(Clone)]
pub(super) struct SearchPosition {
    board: Board,
    repetition_key: u64,
    accumulator: EvalAccumulator,
    correction: CorrectionAccumulator,
}

impl SearchPosition {
    pub(super) fn from_board(board: &Board) -> Self {
        Self {
            board: board.clone(),
            repetition_key: repetition_key(board),
            accumulator: EvalAccumulator::from_board(board),
            correction: CorrectionAccumulator::from_board(board),
        }
    }

    pub(super) fn board(&self) -> &Board {
        &self.board
    }

    pub(super) fn repetition_key(&self) -> u64 {
        self.repetition_key
    }

    pub(super) fn accumulator(&self) -> EvalAccumulator {
        self.accumulator
    }

    pub(super) fn pawn_key(&self) -> u64 {
        self.accumulator.pawn_key()
    }

    pub(super) fn correction_indices(&self) -> [usize; 3] {
        self.correction.indices(self.accumulator.pawn_key())
    }

    /// Creates a child for a move already supplied by legal move generation or
    /// explicitly checked at an API/TT boundary.
    pub(super) fn child(&self, mv: Move) -> Self {
        debug_assert!(self.board.is_legal(mv));
        let board = self.played(mv);
        self.child_with_board(mv, board)
    }

    /// Plays only the board portion of a legal child. Search uses this before
    /// forward-pruning checks that need the resulting checkers/position, then
    /// fills the auxiliary state only when the move will actually be searched.
    pub(super) fn played(&self, mv: Move) -> Board {
        debug_assert!(self.board.is_legal(mv));
        let mut board = self.board.clone();
        board.play_unchecked(mv);
        board
    }

    pub(super) fn child_with_board(&self, mv: Move, board: Board) -> Self {
        debug_assert!(self.board.is_legal(mv));
        let facts = MoveFacts::from_board(&self.board, mv);
        let accumulator = self.accumulator.after_move(mv, facts);
        let correction = self.correction.after_move(&self.board, mv, facts);
        let child = Self {
            repetition_key: repetition_key(&board),
            board,
            accumulator,
            correction,
        };
        debug_assert_eq!(
            child.accumulator,
            EvalAccumulator::from_board(&child.board),
            "incremental evaluation state diverged after {mv}"
        );
        debug_assert_eq!(
            child.correction,
            CorrectionAccumulator::from_board(&child.board),
            "incremental correction state diverged after {mv}"
        );
        child
    }

    pub(super) fn null_child(&self) -> Option<Self> {
        let board = self.board.null_move()?;
        Some(Self {
            repetition_key: repetition_key(&board),
            board,
            accumulator: self.accumulator,
            correction: self.correction,
        })
    }
}

#[cfg(test)]
mod tests {
    use cozy_chess::{Board, Move};

    use super::*;
    use crate::eval::{evaluate, evaluate_with_pawns, pawn_structure};
    use crate::search::moves::legal_moves;

    fn assert_children(fen: &str) {
        let board = fen.parse::<Board>().unwrap();
        let position = SearchPosition::from_board(&board);
        for mv in legal_moves(&board) {
            let child = position.child(mv);
            assert_eq!(
                evaluate(child.board()),
                evaluate_with_pawns(
                    child.board(),
                    pawn_structure(child.board()),
                    child.accumulator()
                ),
                "{fen} after {mv}"
            );
            assert_eq!(
                child.pawn_key(),
                crate::eval::pawn_structure_key(child.board()),
                "{fen} after {mv}"
            );
            assert_eq!(
                child.repetition_key(),
                repetition_key(child.board()),
                "{fen} after {mv}"
            );
        }
    }

    #[test]
    fn incremental_state_matches_full_recomputation_for_all_move_types() {
        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/2pP4/1p2P3/2N2N2/PPQBBPPP/R3K2R w KQkq - 0 1",
            "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
            "4k3/3p4/8/4P3/8/8/8/4K3 b - - 0 1",
            "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 2",
            "1r2k3/P6p/8/8/8/8/7P/4K3 w - - 0 1",
        ] {
            assert_children(fen);
        }
    }

    #[test]
    fn null_move_preserves_piece_accumulator() {
        let position = SearchPosition::from_board(&Board::default());
        let child = position.null_child().unwrap();
        assert_eq!(position.accumulator(), child.accumulator());
        assert_ne!(position.repetition_key(), child.repetition_key());
    }

    #[test]
    fn castling_and_en_passant_examples_are_present() {
        for (fen, notation) in [
            ("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", "e1h1"),
            ("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", "e1a1"),
            ("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 2", "e5d6"),
            ("1r2k3/P7/8/8/8/8/8/4K3 w - - 0 1", "a7b8q"),
        ] {
            let board = fen.parse::<Board>().unwrap();
            let mv = notation.parse::<Move>().unwrap();
            assert!(board.is_legal(mv), "{fen}: {notation}");
            SearchPosition::from_board(&board).child(mv);
        }
    }
}
