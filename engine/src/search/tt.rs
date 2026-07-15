//! Transposition table entry layout and mate-score adjustment for storage.

use super::{MATE_SCORE, MAX_PLY};

pub(crate) const TT_ENTRIES: usize = 1 << 17;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Bound {
    Empty,
    Exact,
    Lower,
    Upper,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub(crate) struct TTEntry {
    pub(crate) key: u64,
    pub(crate) best: u16,
    pub(crate) score: i16,
    pub(crate) depth: i8,
    pub(crate) bound: Bound,
    pub(crate) generation: u8,
    pub(crate) padding: u8,
}

impl Default for TTEntry {
    fn default() -> Self {
        Self {
            key: 0,
            best: 0,
            score: 0,
            depth: -1,
            bound: Bound::Empty,
            generation: 0,
            padding: 0,
        }
    }
}

pub(crate) fn score_to_tt(score: i32, ply: usize) -> i32 {
    if score >= MATE_SCORE - MAX_PLY as i32 {
        score + ply as i32
    } else if score <= -MATE_SCORE + MAX_PLY as i32 {
        score - ply as i32
    } else {
        score
    }
}

pub(crate) fn score_from_tt(score: i32, ply: usize) -> i32 {
    if score >= MATE_SCORE - MAX_PLY as i32 {
        score - ply as i32
    } else if score <= -MATE_SCORE + MAX_PLY as i32 {
        score + ply as i32
    } else {
        score
    }
}

#[cfg(test)]
mod tests {
    use cozy_chess::Move;

    use super::*;
    use crate::search::SearchCore;

    #[test]
    fn tt_entry_stays_compact() {
        assert_eq!(std::mem::size_of::<TTEntry>(), 16);
    }

    #[test]
    fn store_keeps_deeper_entry_on_same_key_shallow_rewrite() {
        let mut search = SearchCore::new();
        let key = 0x1234_5678_9abc_def0;
        let mv = "e2e4".parse::<Move>().unwrap();
        search.store(key, 10, 0, Bound::Exact, mv);
        search.store(key, 2, 0, Bound::Exact, mv);
        assert_eq!(search.probe(key).unwrap().depth, 10);
    }
}
