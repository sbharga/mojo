//! Static exchange evaluation (SEE) for capture ordering and pruning.

use cozy_chess::{
    BitBoard, Board, Color, Move, Piece, Square, get_bishop_moves, get_king_moves,
    get_knight_moves, get_pawn_attacks, get_rook_moves,
};

use crate::eval::piece_value;

use super::moves::{captured_value_for_capture, is_capture};

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

pub(crate) fn static_exchange_capture(board: &Board, mv: Move) -> i32 {
    debug_assert!(is_capture(board, mv));
    let mut gains = [0_i32; 32];
    gains[0] = captured_value_for_capture(board, mv)
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
    use cozy_chess::Board;

    use super::*;

    #[test]
    fn static_exchange_distinguishes_winning_and_losing_captures() {
        let winning = "7k/3q4/8/8/8/8/3R4/7K w - - 0 1".parse::<Board>().unwrap();
        assert!(static_exchange_capture(&winning, "d2d7".parse().unwrap()) > 0);

        let losing = "3q3k/3p4/8/8/8/8/8/3Q3K w - - 0 1"
            .parse::<Board>()
            .unwrap();
        assert!(static_exchange_capture(&losing, "d1d7".parse().unwrap()) < 0);
    }
}
