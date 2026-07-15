//! Generic move/position utilities: legal move generation, UCI-independent
//! move encoding, the one-ply evaluation fallback, and the repetition/rule
//! hash keys used to detect draws and to key the transposition table.

use arrayvec::ArrayVec;
use cozy_chess::{Board, Move, Piece, Rank, Square, get_pawn_attacks};

use crate::eval::{evaluate, insufficient_material, piece_value};

use super::{MATE_SCORE, MAX_MOVES, MoveList};

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

pub(crate) fn is_capture(board: &Board, mv: Move) -> bool {
    board.piece_on(mv.to).is_some()
        || (board.piece_on(mv.from) == Some(Piece::Pawn) && mv.from.file() != mv.to.file())
}

pub(crate) fn captured_value(board: &Board, mv: Move) -> i32 {
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

pub(crate) fn repetition_key(board: &Board) -> u64 {
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

pub(crate) fn rule_key(board: &Board) -> u64 {
    repetition_key(board) ^ u64::from(board.halfmove_clock()).wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

pub(crate) fn encode_move(mv: Move) -> u16 {
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

pub(crate) fn decode_move(encoded: u16) -> Option<Move> {
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

#[cfg(test)]
mod tests {
    use cozy_chess::Board;

    use super::*;

    #[test]
    fn move_encoding_round_trips() {
        for text in ["e2e4", "e7e8q", "e1h1"] {
            let mv = text.parse::<Move>().unwrap();
            assert_eq!(decode_move(encode_move(mv)), Some(mv));
        }
    }

    #[test]
    fn fallback_prefers_immediate_checkmate() {
        let board = "7k/5Q2/6K1/8/8/8/8/8 w - - 0 1".parse::<Board>().unwrap();
        let child = played(&board, fallback(&board).unwrap());

        assert!(legal_moves(&child).is_empty());
        assert!(!child.checkers().is_empty());
    }
}
