//! Pawn-structure and material correction history for removing repeatable
//! bias from the handcrafted static evaluation.

use cozy_chess::{Board, Color, Move, Piece, Square};

use crate::eval::MoveFacts;

use super::SearchCore;
use super::position::SearchPosition;
use super::tt::Bound;

const CORRECTION_BUCKETS: usize = 1 << 14;
pub(crate) const CORRECTION_HISTORY_ENTRIES: usize = 2 * CORRECTION_BUCKETS;
const CORRECTION_SCALE: i32 = 256;
const CORRECTION_LIMIT: i32 = 32 * CORRECTION_SCALE;
const CORRECTION_MAX: i32 = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct CorrectionAccumulator {
    material_hash: u64,
    nonpawn_hash: u64,
}

impl CorrectionAccumulator {
    pub(super) fn from_board(board: &Board) -> Self {
        let mut material_hash = 0_u64;
        for color in [Color::White, Color::Black] {
            for piece in [
                Piece::Pawn,
                Piece::Knight,
                Piece::Bishop,
                Piece::Rook,
                Piece::Queen,
            ] {
                material_hash ^= material_component(
                    color,
                    piece,
                    board.colored_pieces(color, piece).len() as usize,
                );
            }
        }
        let mut nonpawn_hash = 0_u64;
        for color in [Color::White, Color::Black] {
            for piece in [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen] {
                for square in board.colored_pieces(color, piece) {
                    nonpawn_hash ^= nonpawn_component(square, color, piece);
                }
            }
        }
        Self {
            material_hash,
            nonpawn_hash,
        }
    }

    pub(super) fn after_move(mut self, board: &Board, mv: Move, facts: MoveFacts) -> Self {
        let MoveFacts {
            color,
            moved,
            victim,
            is_castle,
        } = facts;

        if is_castle {
            let rook_to = Square::new(
                if mv.from.file() < mv.to.file() {
                    cozy_chess::File::F
                } else {
                    cozy_chess::File::D
                },
                mv.from.rank(),
            );
            self.move_nonpawn(color, Piece::Rook, mv.to, rook_to);
            return self;
        }

        if moved == Piece::Pawn {
            if mv.promotion.is_some() {
                self.adjust_material(board, color, Piece::Pawn, -1);
            }
        } else if moved != Piece::King {
            self.move_nonpawn(color, moved, mv.from, mv.to);
        }

        if let Some(victim) = victim {
            self.remove_piece(board, !color, victim, mv.to);
        } else if moved == Piece::Pawn && mv.from.file() != mv.to.file() {
            let captured = Square::new(mv.to.file(), mv.from.rank());
            self.remove_piece(board, !color, Piece::Pawn, captured);
        }

        if let Some(promotion) = mv.promotion {
            self.adjust_material(board, color, promotion, 1);
            self.add_nonpawn(color, promotion, mv.to);
        }
        self
    }

    pub(super) fn indices(self, pawn_key: u64) -> [usize; 3] {
        [
            fold_hash(pawn_key),
            fold_hash(self.material_hash),
            fold_hash(self.nonpawn_hash),
        ]
    }

    fn adjust_material(&mut self, board: &Board, color: Color, piece: Piece, delta: i8) {
        if piece == Piece::King {
            return;
        }
        let current = board.colored_pieces(color, piece).len() as i8;
        let updated = current + delta;
        debug_assert!((0..=15).contains(&updated));
        self.material_hash ^= material_component(color, piece, current as usize);
        self.material_hash ^= material_component(color, piece, updated as usize);
    }

    fn move_nonpawn(&mut self, color: Color, piece: Piece, from: Square, to: Square) {
        self.nonpawn_hash ^= nonpawn_component(from, color, piece);
        self.nonpawn_hash ^= nonpawn_component(to, color, piece);
    }

    fn remove_piece(&mut self, board: &Board, color: Color, piece: Piece, square: Square) {
        self.adjust_material(board, color, piece, -1);
        if piece != Piece::Pawn && piece != Piece::King {
            self.nonpawn_hash ^= nonpawn_component(square, color, piece);
        }
    }

    fn add_nonpawn(&mut self, color: Color, piece: Piece, square: Square) {
        self.nonpawn_hash ^= nonpawn_component(square, color, piece);
    }
}

impl SearchCore {
    pub(super) fn corrected_static_eval(&self, position: &SearchPosition, raw_eval: i32) -> i32 {
        let side = position.board().side_to_move() as usize;
        let [pawn, material, nonpawn] = position.correction_indices();
        let pawn = i32::from(self.pawn_correction[side * CORRECTION_BUCKETS + pawn]);
        let material = i32::from(self.material_correction[side * CORRECTION_BUCKETS + material]);
        let nonpawn = i32::from(self.nonpawn_correction[side * CORRECTION_BUCKETS + nonpawn]);
        // Each source contributes up to CORRECTION_MAX/2 cp (a single table at its
        // ±CORRECTION_LIMIT); agreeing sources reinforce up to the ±CORRECTION_MAX cap.
        let correction = ((pawn + material + nonpawn) / (2 * CORRECTION_SCALE))
            .clamp(-CORRECTION_MAX, CORRECTION_MAX);
        raw_eval + correction
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn update_correction_history(
        &mut self,
        position: &SearchPosition,
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
        let side = position.board().side_to_move() as usize;
        let [pawn, material, nonpawn] = position.correction_indices();
        let pawn_index = side * CORRECTION_BUCKETS + pawn;
        let material_index = side * CORRECTION_BUCKETS + material;
        let nonpawn_index = side * CORRECTION_BUCKETS + nonpawn;
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

fn material_component(color: Color, piece: Piece, count: usize) -> u64 {
    correction_hash(1 + count as u64 + 16 * piece as u64 + 80 * color as u64)
}

fn nonpawn_component(square: Square, color: Color, piece: Piece) -> u64 {
    correction_hash(161 + square as u64 + 64 * piece as u64 + 384 * color as u64)
}

fn correction_hash(value: u64) -> u64 {
    let value = value.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value ^ value.rotate_left(25) ^ (value >> 27)
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
        let position = SearchPosition::from_board(&board);
        let mut search = SearchCore::new();
        for _ in 0..32 {
            let corrected = search.corrected_static_eval(&position, 0);
            search.update_correction_history(&position, 0, corrected, 200, 16, Bound::Exact, false);
        }
        assert!((31..=32).contains(&search.corrected_static_eval(&position, 0)));

        let black = board.null_move().unwrap();
        let black_position = SearchPosition::from_board(&black);
        assert_eq!(search.corrected_static_eval(&black_position, 0), 0);
    }

    #[test]
    fn capture_fail_high_does_not_train_correction() {
        let board = Board::default();
        let position = SearchPosition::from_board(&board);
        let mut search = SearchCore::new();
        search.update_correction_history(&position, 0, 0, 100, 8, Bound::Lower, true);
        assert_eq!(search.corrected_static_eval(&position, 0), 0);
    }
}
