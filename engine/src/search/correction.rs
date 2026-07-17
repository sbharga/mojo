//! Pawn-structure and material correction history for removing repeatable
//! bias from the handcrafted static evaluation.

use cozy_chess::{Board, Color, Piece};

use super::SearchCore;
use super::tt::Bound;

const CORRECTION_BUCKETS: usize = 1 << 14;
pub(crate) const CORRECTION_HISTORY_ENTRIES: usize = 2 * CORRECTION_BUCKETS;
const CORRECTION_SCALE: i32 = 256;
const CORRECTION_LIMIT: i32 = 32 * CORRECTION_SCALE;
const CORRECTION_MAX: i32 = 32;

impl SearchCore {
    pub(crate) fn corrected_static_eval(&self, board: &Board, raw_eval: i32) -> i32 {
        let side = board.side_to_move() as usize;
        let pawn = i32::from(self.pawn_correction[side * CORRECTION_BUCKETS + pawn_index(board)]);
        let material =
            i32::from(self.material_correction[side * CORRECTION_BUCKETS + material_index(board)]);
        let nonpawn =
            i32::from(self.nonpawn_correction[side * CORRECTION_BUCKETS + nonpawn_index(board)]);
        // Each source contributes up to CORRECTION_MAX/2 cp (a single table at its
        // ±CORRECTION_LIMIT); agreeing sources reinforce up to the ±CORRECTION_MAX cap.
        let correction = ((pawn + material + nonpawn) / (2 * CORRECTION_SCALE))
            .clamp(-CORRECTION_MAX, CORRECTION_MAX);
        raw_eval + correction
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn update_correction_history(
        &mut self,
        board: &Board,
        raw_eval: i32,
        corrected_eval: i32,
        search_score: i32,
        depth: i16,
        bound: Bound,
        fail_high_capture: bool,
    ) {
        let usable = match bound {
            Bound::Exact => true,
            Bound::Upper => search_score < corrected_eval,
            Bound::Lower => search_score > corrected_eval && !fail_high_capture,
            Bound::Empty => false,
        };
        if !usable || search_score.abs() >= super::MATE_SCORE - super::MAX_PLY as i32 {
            return;
        }

        let target = ((search_score - raw_eval) * CORRECTION_SCALE)
            .clamp(-CORRECTION_LIMIT, CORRECTION_LIMIT);
        let weight = i32::from(depth.clamp(1, 16));
        let side = board.side_to_move() as usize;
        let pawn_index = side * CORRECTION_BUCKETS + pawn_index(board);
        let material_index = side * CORRECTION_BUCKETS + material_index(board);
        let nonpawn_index = side * CORRECTION_BUCKETS + nonpawn_index(board);
        update_entry(&mut self.pawn_correction[pawn_index], target, weight);
        update_entry(
            &mut self.material_correction[material_index],
            target,
            weight,
        );
        update_entry(&mut self.nonpawn_correction[nonpawn_index], target, weight);
    }
}

fn update_entry(entry: &mut i16, target: i32, weight: i32) {
    let current = i32::from(*entry);
    let updated = current + (target - current) * weight / 32;
    *entry = updated.clamp(-CORRECTION_LIMIT, CORRECTION_LIMIT) as i16;
}

fn pawn_index(board: &Board) -> usize {
    let white = (board.colored_pieces(Color::White, Piece::Pawn)).0;
    let black = (board.colored_pieces(Color::Black, Piece::Pawn)).0;
    fold_hash(mix(white) ^ mix(black.rotate_left(32)))
}

fn nonpawn_index(board: &Board) -> usize {
    // Hash the *placement* of the non-pawn, non-king pieces of both colors so the
    // residual can key on piece configuration (not just counts as material does).
    let mut hash = 0_u64;
    for (i, color) in [Color::White, Color::Black].into_iter().enumerate() {
        for piece in [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen] {
            let bits = board.colored_pieces(color, piece).0;
            hash ^= mix(bits.rotate_left((piece as u32) * 8 + (i as u32) * 4));
        }
    }
    fold_hash(hash)
}

fn material_index(board: &Board) -> usize {
    let mut signature = 0_u64;
    let mut shift = 0;
    for color in [Color::White, Color::Black] {
        for piece in [
            Piece::Pawn,
            Piece::Knight,
            Piece::Bishop,
            Piece::Rook,
            Piece::Queen,
        ] {
            signature |= u64::from(board.colored_pieces(color, piece).len()) << shift;
            shift += 4;
        }
    }
    fold_hash(mix(signature))
}

fn mix(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn fold_hash(hash: u64) -> usize {
    (hash as usize ^ (hash >> 32) as usize) & (CORRECTION_BUCKETS - 1)
}

#[cfg(test)]
mod tests {
    use cozy_chess::Board;

    use super::*;

    #[test]
    fn correction_tables_use_192_kib_and_learn_a_bounded_residual() {
        // pawn + material + nonpawn tables.
        assert_eq!(
            3 * CORRECTION_HISTORY_ENTRIES * std::mem::size_of::<i16>(),
            192 * 1024
        );
        let board = Board::default();
        let mut search = SearchCore::new();
        for _ in 0..32 {
            search.update_correction_history(
                &board,
                0,
                search.corrected_static_eval(&board, 0),
                200,
                16,
                Bound::Exact,
                false,
            );
        }
        assert!((31..=32).contains(&search.corrected_static_eval(&board, 0)));

        let black = board.null_move().unwrap();
        assert_eq!(search.corrected_static_eval(&black, 0), 0);
    }

    #[test]
    fn capture_fail_high_does_not_train_correction() {
        let board = Board::default();
        let mut search = SearchCore::new();
        search.update_correction_history(&board, 0, 0, 100, 8, Bound::Lower, true);
        assert_eq!(search.corrected_static_eval(&board, 0), 0);
    }
}
