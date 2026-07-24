//! Direct-mapped cache for pawn-only evaluation work.

use crate::eval::{PawnStructure, evaluate_with_pawns, pawn_structure};

use super::{SearchCore, position::SearchPosition};

const PAWN_CACHE_ENTRIES: usize = 1 << 12;

#[derive(Clone, Copy, Default)]
pub(super) struct PawnCacheEntry {
    key: u64,
    structure: PawnStructure,
}

impl SearchCore {
    pub(super) fn raw_evaluate(&mut self, position: &SearchPosition) -> i32 {
        let board = position.board();
        let key = position.pawn_key();
        let index = key as usize & (PAWN_CACHE_ENTRIES - 1);
        let entry = self.pawn_cache[index];
        let structure = if entry.key == key {
            entry.structure
        } else {
            let structure = pawn_structure(board);
            self.pawn_cache[index] = PawnCacheEntry { key, structure };
            structure
        };
        evaluate_with_pawns(board, structure, position.accumulator())
    }
}

pub(super) fn empty_cache() -> Box<[PawnCacheEntry]> {
    vec![PawnCacheEntry::default(); PAWN_CACHE_ENTRIES].into_boxed_slice()
}

#[cfg(test)]
mod tests {
    use cozy_chess::Board;

    use crate::eval::{evaluate, pawn_structure_key};

    use super::*;

    #[test]
    fn cache_is_128_kib_and_matches_uncached_evaluation() {
        assert_eq!(
            PAWN_CACHE_ENTRIES * std::mem::size_of::<PawnCacheEntry>(),
            128 * 1024
        );
        let mut search = SearchCore::new();
        for fen in [
            "r3k2r/ppp2ppp/2n1bn2/3qp3/3P4/2N1PN2/PPP1BPPP/R2QK2R w KQkq - 4 10",
            "8/2p2pk1/1p1p2p1/p2P3p/P1P1P3/1P3K2/5PP1/8 b - - 0 35",
            "8/8/8/8/3P4/8/3K4/7k w - - 0 1",
        ] {
            let board = fen.parse::<Board>().unwrap();
            let position = SearchPosition::from_board(&board);
            assert_eq!(search.raw_evaluate(&position), evaluate(&board));
            assert_eq!(search.raw_evaluate(&position), evaluate(&board));
        }
    }

    #[test]
    fn cached_passers_still_use_current_king_distances() {
        let near = "8/8/8/8/3P4/8/3K4/7k w - - 0 1".parse::<Board>().unwrap();
        let far = "8/8/8/8/3P4/8/8/K6k w - - 0 1".parse::<Board>().unwrap();
        assert_eq!(pawn_structure_key(&near), pawn_structure_key(&far));
        let mut search = SearchCore::new();
        let near_position = SearchPosition::from_board(&near);
        let far_position = SearchPosition::from_board(&far);
        assert_eq!(search.raw_evaluate(&near_position), evaluate(&near));
        assert_eq!(search.raw_evaluate(&far_position), evaluate(&far));
        assert_ne!(
            search.raw_evaluate(&near_position),
            search.raw_evaluate(&far_position)
        );
    }
}
