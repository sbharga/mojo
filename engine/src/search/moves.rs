//! Generic move/position utilities: legal move generation, UCI-independent
//! move encoding, the one-ply evaluation fallback, and the repetition/rule
//! hash keys used to detect draws and to key the transposition table.

use arrayvec::ArrayVec;
use cozy_chess::{BitBoard, Board, Move, Piece, Rank, Square, get_pawn_attacks};

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

pub(crate) fn tactical_moves(board: &Board) -> MoveList {
    let mut moves = moves_to_targets(board, capture_targets(board));
    let promotion_rank = Rank::Eighth.relative_to(board.side_to_move()).bitboard();
    let pawns = board.colored_pieces(board.side_to_move(), Piece::Pawn);
    board.generate_moves_for(pawns, |mut piece_moves| {
        piece_moves.to &= promotion_rank & !board.colors(!board.side_to_move());
        moves.extend(piece_moves);
        false
    });
    moves
}

pub(crate) fn quiet_moves(board: &Board) -> MoveList {
    let mut moves = MoveList::new();
    let targets = !capture_targets(board);
    board.generate_moves(|mut piece_moves| {
        piece_moves.to &= targets;
        moves.extend(piece_moves.into_iter().filter(|mv| mv.promotion.is_none()));
        false
    });
    moves
}

fn moves_to_targets(board: &Board, targets: BitBoard) -> MoveList {
    let mut moves = MoveList::new();
    board.generate_moves(|mut piece_moves| {
        piece_moves.to &= targets;
        moves.extend(piece_moves);
        false
    });
    moves
}

fn capture_targets(board: &Board) -> BitBoard {
    let en_passant = board.en_passant().map_or(BitBoard::EMPTY, |file| {
        Square::new(file, Rank::Third.relative_to(!board.side_to_move())).bitboard()
    });
    board.colors(!board.side_to_move()) | en_passant
}

pub(crate) fn played(board: &Board, mv: Move) -> Board {
    debug_assert!(board.is_legal(mv));
    let mut next = board.clone();
    next.play_unchecked(mv);
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
    board.colors(!board.side_to_move()).has(mv.to)
        || (board.pieces(Piece::Pawn).has(mv.from) && mv.from.file() != mv.to.file())
}

pub(crate) fn captured_value_for_capture(board: &Board, mv: Move) -> i32 {
    debug_assert!(is_capture(board, mv));
    board
        .piece_on(mv.to)
        .map_or_else(|| piece_value(Piece::Pawn), piece_value)
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
    rule_key_from(repetition_key(board), board.halfmove_clock())
}

/// `rule_key` for a position whose `repetition_key` is already known (e.g. from
/// the search path stack), avoiding a second hash computation.
pub(crate) fn rule_key_from(repetition: u64, halfmove_clock: u8) -> u64 {
    repetition ^ u64::from(halfmove_clock).wrapping_mul(0x9E37_79B9_7F4A_7C15)
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

    #[test]
    fn staged_generation_partitions_every_legal_move() {
        for fen in [
            Board::default().to_string(),
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1".to_owned(),
            "4k3/6P1/8/3pP3/8/8/8/4K3 w - d6 0 1".to_owned(),
        ] {
            let board = fen.parse::<Board>().unwrap();
            let mut all: Vec<_> = legal_moves(&board).into_iter().map(encode_move).collect();
            let mut staged: Vec<_> = tactical_moves(&board)
                .into_iter()
                .chain(quiet_moves(&board))
                .map(encode_move)
                .collect();
            all.sort_unstable();
            staged.sort_unstable();
            assert_eq!(staged, all, "{fen}");
        }
    }

    #[test]
    fn castling_is_a_quiet_move_not_a_rook_capture() {
        let board = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1"
            .parse::<Board>()
            .unwrap();
        for text in ["e1a1", "e1h1"] {
            let mv = text.parse::<Move>().unwrap();
            assert!(board.is_legal(mv));
            assert!(!is_capture(&board, mv));
        }
    }
}
