//! Static exchange evaluation (SEE) for capture ordering and pruning.

use cozy_chess::{
    BitBoard, Board, Color, Move, Piece, Square, get_bishop_moves, get_king_moves,
    get_knight_moves, get_pawn_attacks, get_rook_moves,
};

use crate::eval::piece_value;

use super::moves::{captured_value_for_capture, is_capture};

fn attackers_to(board: &Board, target: Square, occupied: BitBoard) -> BitBoard {
    (get_pawn_attacks(target, Color::Black)
        & board.colors(Color::White)
        & board.pieces(Piece::Pawn))
        | (get_pawn_attacks(target, Color::White)
            & board.colors(Color::Black)
            & board.pieces(Piece::Pawn))
        | (get_knight_moves(target) & board.pieces(Piece::Knight))
        | (get_king_moves(target) & board.pieces(Piece::King))
        | (get_bishop_moves(target, occupied)
            & (board.pieces(Piece::Bishop) | board.pieces(Piece::Queen)))
        | (get_rook_moves(target, occupied)
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
    let mut attackers = attackers_to(board, mv.to, occupied);
    let diagonal_sliders = board.pieces(Piece::Bishop) | board.pieces(Piece::Queen);
    let orthogonal_sliders = board.pieces(Piece::Rook) | board.pieces(Piece::Queen);

    while depth + 1 < gains.len() {
        let side_attackers = attackers & board.colors(side) & occupied;
        let Some((piece, from)) = Piece::ALL.into_iter().find_map(|piece| {
            (side_attackers & board.pieces(piece))
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
        // Leaper attacks are unchanged; only newly exposed slider rays need
        // extending after the least-valuable attacker leaves its square.
        attackers |= get_bishop_moves(mv.to, occupied) & diagonal_sliders;
        attackers |= get_rook_moves(mv.to, occupied) & orthogonal_sliders;
        side = !side;
    }
    while depth > 0 {
        gains[depth - 1] = -(-gains[depth - 1]).max(gains[depth]);
        depth -= 1;
    }
    gains[0]
}

/// Tests a capture against a material threshold without building the complete
/// swap list when the first exchange already proves the answer.
pub(crate) fn static_exchange_ge_capture(board: &Board, mv: Move, threshold: i32) -> bool {
    debug_assert!(is_capture(board, mv));
    let immediate = captured_value_for_capture(board, mv)
        + mv.promotion
            .map_or(0, |piece| piece_value(piece) - piece_value(Piece::Pawn));
    if immediate < threshold {
        return false;
    }
    let moved = mv
        .promotion
        .unwrap_or_else(|| board.piece_on(mv.from).unwrap_or(Piece::Pawn));
    if immediate - piece_value(moved) >= threshold {
        return true;
    }
    static_exchange_capture(board, mv) >= threshold
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

    #[test]
    fn threshold_see_matches_full_swap_across_legal_captures() {
        for fen in [
            "r3k2r/p1ppqpb1/bn2pnp1/2pP4/1p2P3/2N2N2/PPQBBPPP/R3K2R w KQkq - 0 1",
            "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 2",
            "1r2k3/P6p/8/8/8/8/7P/4K3 w - - 0 1",
            "3q3k/3p4/8/8/8/8/8/3Q3K w - - 0 1",
        ] {
            let board = fen.parse::<Board>().unwrap();
            for mv in crate::search::moves::legal_moves(&board)
                .into_iter()
                .filter(|mv| is_capture(&board, *mv))
            {
                let see = static_exchange_capture(&board, mv);
                for threshold in [-500, -1, 0, 1, 100, 500, 1_000] {
                    assert_eq!(
                        static_exchange_ge_capture(&board, mv, threshold),
                        see >= threshold,
                        "{fen}: {mv}, SEE {see}, threshold {threshold}"
                    );
                }
            }
        }
    }
}
