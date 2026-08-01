//! Cheap "does this move give check" detection that avoids making the move
//! just to read `Board::checkers()` afterward. `negamax`/`quiescence` gate
//! several pruning decisions on `gives_check` for every move considered at a
//! node, and paying for a full `play_unchecked` before that gate runs would
//! make every pruned move pay for a make it never needed.

use cozy_chess::{
    BitBoard, Board, Move, Piece, Square, get_between_rays, get_bishop_moves, get_bishop_rays,
    get_knight_moves, get_line_rays, get_pawn_attacks, get_rook_moves, get_rook_rays,
};

/// Per-node facts needed to classify whether a move by the side to move
/// gives check, without making the move. Built once per node from the enemy
/// king's square and reused for every move considered there.
pub(super) struct CheckContext {
    enemy_king: Square,
    occupied: BitBoard,
    // Leaper direct-check destinations. Unlike sliders these are occupancy
    // independent, so they stay correct for every move regardless of which
    // square the mover vacates.
    pawn: BitBoard,
    knight: BitBoard,
    // Squares of our own pieces that each solely block one of our sliders'
    // line to the enemy king. Vacating one (without landing back on that
    // same line) opens a discovered check; see `gives_check`.
    discovery_blockers: BitBoard,
}

impl CheckContext {
    pub(super) fn new(board: &Board) -> Self {
        let us = board.side_to_move();
        let enemy_king = board.king(!us);
        let occupied = board.occupied();
        let our_diagonal_sliders =
            (board.pieces(Piece::Bishop) | board.pieces(Piece::Queen)) & board.colors(us);
        let our_orthogonal_sliders =
            (board.pieces(Piece::Rook) | board.pieces(Piece::Queen)) & board.colors(us);

        // Same "sniper" trick cozy-chess itself uses for `Board::pinned()`
        // (see its `update_slider_blockers`/set_state ray scan), just aimed
        // at the enemy king with our own sliders instead of the mover's own
        // king with the opponent's.
        let mut discovery_blockers = BitBoard::EMPTY;
        for sniper in (get_bishop_rays(enemy_king) & our_diagonal_sliders)
            | (get_rook_rays(enemy_king) & our_orthogonal_sliders)
        {
            let between = get_between_rays(sniper, enemy_king) & occupied;
            if between.len() == 1 {
                discovery_blockers |= between;
            }
        }

        Self {
            enemy_king,
            occupied,
            pawn: get_pawn_attacks(enemy_king, !us),
            knight: get_knight_moves(enemy_king),
            discovery_blockers,
        }
    }
}

/// Whether `mv`, played by `board`'s side to move, gives check. Matches
/// `!played(board, mv).checkers().is_empty()` exactly — see the exactness
/// test in `search::moves::tests` and the `debug_assert!` at each call site.
///
/// Castling and en passant fall back to making the move and reading the
/// result directly: both are rare, and each can produce a check through a
/// square (the rook's landing square; the vacated en-passant-captured pawn's
/// square) that this fast path doesn't model.
pub(super) fn gives_check(ctx: &CheckContext, board: &Board, mv: Move) -> bool {
    let Some(piece) = board.piece_on(mv.from) else {
        debug_assert!(false, "gives_check called with an illegal move");
        return false;
    };
    let is_castling = piece == Piece::King && (mv.from.file() as i8 - mv.to.file() as i8).abs() > 1;
    let is_en_passant =
        piece == Piece::Pawn && mv.from.file() != mv.to.file() && board.piece_on(mv.to).is_none();
    if is_castling || is_en_passant {
        let mut after = board.clone();
        after.play_unchecked(mv);
        return !after.checkers().is_empty();
    }

    // A slider's own attack pattern depends on occupancy, and the mover's
    // `from` square is about to empty out, so it must be excluded here even
    // though the move hasn't actually been played yet. Leapers (pawn/knight)
    // need no such correction since their attacks aren't occupancy-dependent.
    let occupied_after = ctx.occupied & !mv.from.bitboard();
    let direct = match mv.promotion.unwrap_or(piece) {
        Piece::Pawn => ctx.pawn.has(mv.to),
        Piece::Knight => ctx.knight.has(mv.to),
        Piece::King => false,
        Piece::Bishop => get_bishop_moves(ctx.enemy_king, occupied_after).has(mv.to),
        Piece::Rook => get_rook_moves(ctx.enemy_king, occupied_after).has(mv.to),
        Piece::Queen => (get_bishop_moves(ctx.enemy_king, occupied_after)
            | get_rook_moves(ctx.enemy_king, occupied_after))
        .has(mv.to),
    };
    if direct {
        return true;
    }

    // Discovered check: `from` was the sole blocker on some friendly
    // slider's line to the enemy king. Moving away opens that line unless
    // the destination is still on the very same line — the piece can only
    // reach squares strictly between the sniper and the king while sliding
    // along it (it can't jump the sniper, a friendly piece, or the king
    // itself), so staying aligned always means staying between them.
    ctx.discovery_blockers.has(mv.from) && !get_line_rays(mv.from, mv.to).has(ctx.enemy_king)
}

#[cfg(test)]
mod tests {
    use cozy_chess::Board;

    use super::*;
    use crate::search::moves::legal_moves;

    fn reference_gives_check(board: &Board, mv: Move) -> bool {
        let mut after = board.clone();
        after.play_unchecked(mv);
        !after.checkers().is_empty()
    }

    fn assert_matches_reference(fen: &str) {
        let board = fen.parse::<Board>().unwrap();
        let ctx = CheckContext::new(&board);
        for mv in legal_moves(&board) {
            assert_eq!(
                gives_check(&ctx, &board, mv),
                reference_gives_check(&board, mv),
                "{fen}: {mv}",
            );
        }
    }

    #[test]
    fn matches_full_recomputation_across_move_types() {
        for fen in [
            // Standard perft positions (see `standard_perft_positions`),
            // covering ordinary quiet/capture play.
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/2pP4/1p2P3/2N2N2/PPQBBPPP/R3K2R w KQkq - 0 1",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
            // Castling, en passant, promotion (see
            // `castling_and_en_passant_examples_are_present`).
            "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1",
            "4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 2",
            "1r2k3/P6p/8/8/8/8/8/4K3 w - - 0 1",
            // A promotion needing the occupancy-after-move correction: the
            // pawn's own vacated square (b7) lies between the new queen
            // (b8) and the enemy king (b1). A plain slider can never need
            // this correction (a rook/bishop/queen already sitting on the
            // same line with a clear path to the king would already be
            // checking it in the parent position, which is illegal), but a
            // promoting pawn's attack pattern changes at the moment of
            // promotion, so this case is reachable.
            "8/1P6/8/8/8/8/8/1k2K3 w - - 0 1",
            // A discovered check: the knight on d2 currently blocks the
            // rook on d1 from the king on d8; every knight move leaves the
            // d-file (file offset is always +-1 or +-2), uncovering it.
            "3k4/8/8/8/8/8/3N4/3R2K1 w - - 0 1",
            // The blocker is a pawn that can push straight ahead, staying
            // on the d-file (`get_line_rays(from, to)` still has the king),
            // so the check must stay hidden — unlike a rook/queen blocker,
            // a pawn's own attacks don't cover the file it can slide along,
            // so it can sit between the sniper and the king without itself
            // already checking it.
            "3k4/8/8/8/3P4/8/8/3R2K1 w - - 0 1",
        ] {
            assert_matches_reference(fen);
        }
    }

    /// `negamax`/`quiescence` each carry a `debug_assert!` comparing the
    /// fast path against `!played(mv).checkers().is_empty()` at every move
    /// they consider. Driving a real (debug-build) search across varied,
    /// tactically dense positions exercises many more move/occupancy
    /// combinations than the hand-picked FENs above, and would panic on any
    /// divergence instead of silently mis-pruning.
    #[test]
    fn real_searches_never_trip_the_exactness_assertion() {
        use cozy_chess::Board;

        use crate::search::SearchCore;

        for fen in [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "r3k2r/p1ppqpb1/bn2pnp1/2pP4/1p2P3/2N2N2/PPQBBPPP/R3K2R w KQkq - 0 1",
            "r1bq1rk1/pp2bppp/2n1pn2/2pp4/3P4/2P1PN2/PP1NBPPP/R2Q1RK1 w - - 2 9",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
        ] {
            let board = fen.parse::<Board>().unwrap();
            let mut search = SearchCore::new();
            search.set_position(&board, &[]);
            search.analyze_depth(&board, 6, 1, 10_000.0);
        }
    }
}
