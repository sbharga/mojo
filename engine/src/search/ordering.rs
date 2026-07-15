//! Staged move ordering with incremental selection, plus the quiet-move
//! cutoff bookkeeping that feeds killers, history, and countermoves.

use arrayvec::ArrayVec;
use cozy_chess::{Board, Move};

use crate::eval::piece_value;

use super::moves::{
    captured_value, encode_move, is_capture, legal_moves, quiet_moves, tactical_moves,
};
use super::see::static_exchange;
use super::{MAX_MOVES, MAX_PLY, SearchCore};

type ScoredMoves = ArrayVec<(i32, Move), MAX_MOVES>;
type ScoredTacticalMoves = ArrayVec<(i32, i32, Move), MAX_MOVES>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Tt,
    Tactical,
    Special,
    Quiet,
    Done,
}

pub(crate) struct MovePicker<'a> {
    board: &'a Board,
    tt_move: Option<Move>,
    excluded: &'a [Move],
    special: [Option<Move>; 3],
    special_index: usize,
    stage: Stage,
    scored: ScoredMoves,
}

impl<'a> MovePicker<'a> {
    pub(crate) fn new(
        board: &'a Board,
        tt_move: Option<Move>,
        killers: [Option<Move>; 2],
        countermove: Option<Move>,
        excluded: &'a [Move],
    ) -> Self {
        Self {
            board,
            tt_move,
            excluded,
            special: [killers[0], killers[1], countermove],
            special_index: 0,
            stage: Stage::Tt,
            scored: ArrayVec::new(),
        }
    }

    pub(crate) fn next(&mut self, search: &SearchCore) -> Option<Move> {
        loop {
            match self.stage {
                Stage::Tt => {
                    self.stage = Stage::Tactical;
                    if let Some(mv) = self.tt_move
                        && !self.excluded.contains(&mv)
                    {
                        return Some(mv);
                    }
                }
                Stage::Tactical => {
                    if self.scored.is_empty() {
                        self.scored.extend(
                            tactical_moves(self.board)
                                .into_iter()
                                .filter(|mv| Some(*mv) != self.tt_move)
                                .filter(|mv| !self.excluded.contains(mv))
                                .map(|mv| (tactical_score(self.board, mv), mv)),
                        );
                    }
                    if let Some(mv) = select_best(&mut self.scored) {
                        if self.scored.is_empty() {
                            self.stage = Stage::Special;
                        }
                        return Some(mv);
                    }
                    self.stage = Stage::Special;
                }
                Stage::Special => {
                    while let Some(candidate) = self.special.get(self.special_index).copied() {
                        self.special_index += 1;
                        if let Some(mv) = candidate
                            && Some(mv) != self.tt_move
                            && !self.special[..self.special_index - 1].contains(&Some(mv))
                            && !self.excluded.contains(&mv)
                            && !is_capture(self.board, mv)
                            && mv.promotion.is_none()
                            && self.board.is_legal(mv)
                        {
                            return Some(mv);
                        }
                    }
                    self.stage = Stage::Quiet;
                }
                Stage::Quiet => {
                    if self.scored.is_empty() {
                        self.scored.extend(
                            quiet_moves(self.board)
                                .into_iter()
                                .filter(|mv| Some(*mv) != self.tt_move)
                                .filter(|mv| !self.special.contains(&Some(*mv)))
                                .filter(|mv| !self.excluded.contains(mv))
                                .map(|mv| (search.history[mv.from as usize][mv.to as usize], mv)),
                        );
                    }
                    if let Some(mv) = select_best(&mut self.scored) {
                        if self.scored.is_empty() {
                            self.stage = Stage::Done;
                        }
                        return Some(mv);
                    }
                    self.stage = Stage::Done;
                }
                Stage::Done => return None,
            }
        }
    }
}

pub(crate) struct QuiescencePicker {
    scored: ScoredTacticalMoves,
}

impl QuiescencePicker {
    pub(crate) fn new(board: &Board, in_check: bool, search: &SearchCore, ply: usize) -> Self {
        let moves = if in_check {
            legal_moves(board)
        } else {
            tactical_moves(board)
        };
        let scored = moves
            .into_iter()
            .map(|mv| {
                let see = if is_capture(board, mv) {
                    static_exchange(board, mv)
                } else {
                    0
                };
                let score = if in_check && !is_capture(board, mv) && mv.promotion.is_none() {
                    if ply < MAX_PLY && search.killers[ply].contains(&Some(mv)) {
                        900_000
                    } else {
                        search.history[mv.from as usize][mv.to as usize]
                    }
                } else {
                    tactical_score_with_see(board, mv, see)
                };
                (score, see, mv)
            })
            .collect();
        Self { scored }
    }

    pub(crate) fn next(&mut self) -> Option<(Move, i32)> {
        let index = best_index(&self.scored, |(score, ..)| *score)?;
        let (_, see, mv) = self.scored.swap_remove(index);
        Some((mv, see))
    }
}

fn select_best(scored: &mut ScoredMoves) -> Option<Move> {
    let index = best_index(scored, |(score, _)| *score)?;
    Some(scored.swap_remove(index).1)
}

fn best_index<T>(items: &[T], score: impl Fn(&T) -> i32) -> Option<usize> {
    let mut best = 0;
    let first = items.first()?;
    let mut best_score = score(first);
    for (index, item) in items.iter().enumerate().skip(1) {
        let candidate = score(item);
        if candidate > best_score {
            best = index;
            best_score = candidate;
        }
    }
    Some(best)
}

fn tactical_score(board: &Board, mv: Move) -> i32 {
    let see = if is_capture(board, mv) {
        static_exchange(board, mv)
    } else {
        0
    };
    tactical_score_with_see(board, mv, see)
}

fn tactical_score_with_see(board: &Board, mv: Move, see: i32) -> i32 {
    if is_capture(board, mv) {
        1_000_000 + see * 32 + captured_value(board, mv)
    } else {
        950_000 + mv.promotion.map_or(0, piece_value)
    }
}

impl SearchCore {
    pub(crate) fn record_quiet_cutoff(
        &mut self,
        mv: Move,
        depth: i16,
        ply: usize,
        prev_move: Option<Move>,
    ) {
        if ply < MAX_PLY {
            if self.killers[ply][0] != Some(mv) {
                self.killers[ply][1] = self.killers[ply][0];
                self.killers[ply][0] = Some(mv);
            }
            let bonus = i32::from(depth).pow(2).min(2048);
            let value = &mut self.history[mv.from as usize][mv.to as usize];
            *value += bonus - (*value * bonus / 16_384);
        }
        if let Some(prev) = prev_move {
            self.countermove[prev.from as usize][prev.to as usize] = encode_move(mv);
        }
    }
}

#[cfg(test)]
mod tests {
    use cozy_chess::Board;

    use super::{MovePicker, SearchCore};
    use crate::search::moves::{encode_move, legal_moves};

    #[test]
    fn countermove_table_populates_after_a_quiet_cutoff() {
        let board = Board::default();
        let mut search = SearchCore::new();
        search.set_position(&board, &[]);
        search.analyze_depth(&board, 4, 1, 10_000.0);
        assert!(search.countermove.iter().flatten().any(|&entry| entry != 0));
    }

    #[test]
    fn tt_move_is_returned_before_generating_other_stages() {
        let board = Board::default();
        let tt_move = "e2e4".parse().unwrap();
        let search = SearchCore::new();
        let mut picker = MovePicker::new(&board, Some(tt_move), [None; 2], None, &[]);

        assert_eq!(picker.next(&search), Some(tt_move));
        assert!(picker.scored.is_empty());
        assert_eq!(picker.stage, super::Stage::Tactical);
    }

    #[test]
    fn move_picker_returns_every_legal_move_once() {
        let board = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
            .parse::<Board>()
            .unwrap();
        let search = SearchCore::new();
        let tt_move = "e2a6".parse().unwrap();
        let killers = [Some("e1h1".parse().unwrap()), Some("e1a1".parse().unwrap())];
        let mut picker = MovePicker::new(&board, Some(tt_move), killers, None, &[]);
        let mut picked = Vec::new();
        while let Some(mv) = picker.next(&search) {
            picked.push(encode_move(mv));
        }
        let mut legal: Vec<_> = legal_moves(&board).into_iter().map(encode_move).collect();
        picked.sort_unstable();
        legal.sort_unstable();

        assert_eq!(picked, legal);
    }
}
