//! Transposition table entry layout and mate-score adjustment for storage.

use super::{MATE_SCORE, MAX_PLY};

pub(crate) const TT_BUCKET_SIZE: usize = 4;
pub(crate) const TT_BUCKETS: usize = 1 << 15;
pub(crate) const TT_ENTRIES: usize = TT_BUCKETS * TT_BUCKET_SIZE;

const BOUND_MASK: u8 = 0b11;
const GENERATION_SHIFT: u8 = 2;
const GENERATION_MASK: u8 = 0b11_1111;
const NO_STATIC_EVAL: i16 = i16::MIN;

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
    static_eval: i16,
    pub(crate) depth: i8,
    metadata: u8,
}

#[derive(Debug, Clone, Copy, Default)]
#[repr(C, align(64))]
pub(crate) struct TTBucket(pub(crate) [TTEntry; TT_BUCKET_SIZE]);

impl Default for TTEntry {
    fn default() -> Self {
        Self {
            key: 0,
            best: 0,
            score: 0,
            static_eval: NO_STATIC_EVAL,
            depth: -1,
            metadata: 0,
        }
    }
}

impl TTEntry {
    pub(crate) fn new(
        key: u64,
        best: u16,
        score: i16,
        static_eval: Option<i32>,
        depth: i8,
        bound: Bound,
        generation: u8,
    ) -> Self {
        Self {
            key,
            best,
            score,
            static_eval: static_eval.map_or(NO_STATIC_EVAL, clamp_i16),
            depth,
            metadata: ((generation & GENERATION_MASK) << GENERATION_SHIFT) | bound as u8,
        }
    }

    pub(crate) fn bound(self) -> Bound {
        match self.metadata & BOUND_MASK {
            1 => Bound::Exact,
            2 => Bound::Lower,
            3 => Bound::Upper,
            _ => Bound::Empty,
        }
    }

    pub(crate) fn generation(self) -> u8 {
        self.metadata >> GENERATION_SHIFT
    }

    pub(crate) fn static_eval(self) -> Option<i32> {
        (self.static_eval != NO_STATIC_EVAL).then_some(i32::from(self.static_eval))
    }
}

fn clamp_i16(value: i32) -> i16 {
    value.clamp(i32::from(i16::MIN) + 1, i32::from(i16::MAX)) as i16
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
        assert_eq!(std::mem::size_of::<TTBucket>(), 64);
        assert_eq!(std::mem::align_of::<TTBucket>(), 64);
    }

    #[test]
    fn store_keeps_deeper_entry_on_same_key_shallow_rewrite() {
        let mut search = SearchCore::new();
        let key = 0x1234_5678_9abc_def0;
        let mv = "e2e4".parse::<Move>().unwrap();
        search.store(key, 10, 0, Bound::Exact, Some(mv), Some(25));
        search.store(key, 2, 0, Bound::Exact, Some(mv), Some(50));
        assert_eq!(search.probe(key).unwrap().depth, 10);
        assert_eq!(search.probe(key).unwrap().static_eval(), Some(25));
    }

    #[test]
    fn bucket_keeps_colliding_entries() {
        let mut search = SearchCore::new();
        let mv = "e2e4".parse::<Move>().unwrap();
        for offset in 0..4 {
            let key = 7 + offset * super::TT_BUCKETS as u64;
            search.store(key, offset as i16, 0, Bound::Exact, Some(mv), None);
        }
        for offset in 0..4 {
            let key = 7 + offset * super::TT_BUCKETS as u64;
            assert!(search.probe(key).is_some());
        }
    }
}
