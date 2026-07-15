//! Staged move ordering with incremental selection, plus the quiet-move
//! cutoff bookkeeping that feeds killers, history, and countermoves.

use arrayvec::ArrayVec;
use cozy_chess::{Board, Move, Piece};

use crate::eval::piece_value;

use super::moves::{
    captured_value, encode_move, is_capture, legal_moves, quiet_moves, tactical_moves,
};
use super::see::static_exchange;
use super::{MAX_MOVES, MAX_PLY, SearchCore};

const PIECE_TO_SQUARE: usize = 6 * 64;
pub(crate) const CONTINUATION_HISTORY_ENTRIES: usize = PIECE_TO_SQUARE * PIECE_TO_SQUARE;
pub(crate) const CAPTURE_HISTORY_ENTRIES: usize = 12 * 64 * 6;
const HISTORY_LIMIT: i32 = 16_384;
const HISTORY_BONUS_CAP: i32 = 2_048;

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
    prev_move: Option<Move>,
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
        prev_move: Option<Move>,
        excluded: &'a [Move],
    ) -> Self {
        Self {
            board,
            tt_move,
            excluded,
            special: [killers[0], killers[1], countermove],
            prev_move,
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
                                .map(|mv| (tactical_score(self.board, mv, search), mv)),
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
                                .map(|mv| {
                                    (
                                        search.quiet_history_score(self.board, mv, self.prev_move),
                                        mv,
                                    )
                                }),
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
                    tactical_score_with_see(board, mv, see, search.capture_history_score(board, mv))
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

fn tactical_score(board: &Board, mv: Move, search: &SearchCore) -> i32 {
    let see = if is_capture(board, mv) {
        static_exchange(board, mv)
    } else {
        0
    };
    tactical_score_with_see(board, mv, see, search.capture_history_score(board, mv))
}

fn tactical_score_with_see(board: &Board, mv: Move, see: i32, capture_history: i32) -> i32 {
    if is_capture(board, mv) {
        let see_class = match see.cmp(&0) {
            std::cmp::Ordering::Greater => 200_000,
            std::cmp::Ordering::Equal => 100_000,
            std::cmp::Ordering::Less => 0,
        };
        1_000_000
            + see_class
            + capture_history
            + captured_value(board, mv)
            + mv.promotion.map_or(0, piece_value)
    } else {
        950_000 + mv.promotion.map_or(0, piece_value)
    }
}

impl SearchCore {
    pub(crate) fn capture_history_score(&self, board: &Board, mv: Move) -> i32 {
        capture_history_index(board, mv).map_or(0, |index| i32::from(self.capture_history[index]))
    }

    pub(crate) fn record_capture_cutoff(
        &mut self,
        board: &Board,
        mv: Move,
        depth: i16,
        searched_captures: &[Move],
    ) {
        let bonus = i32::from(depth).pow(2).min(HISTORY_BONUS_CAP);
        self.update_capture_history(board, mv, bonus);
        for &failed in searched_captures {
            if failed != mv {
                self.update_capture_history(board, failed, -bonus);
            }
        }
    }

    fn update_capture_history(&mut self, board: &Board, mv: Move, bonus: i32) {
        if let Some(index) = capture_history_index(board, mv) {
            update_continuation(&mut self.capture_history[index], bonus);
        }
    }

    pub(crate) fn quiet_history_score(
        &self,
        board: &Board,
        mv: Move,
        prev_move: Option<Move>,
    ) -> i32 {
        self.history[mv.from as usize][mv.to as usize]
            + continuation_index(board, prev_move, mv)
                .map_or(0, |index| i32::from(self.continuation_history[index]))
    }

    pub(crate) fn record_quiet_cutoff(
        &mut self,
        board: &Board,
        mv: Move,
        depth: i16,
        ply: usize,
        prev_move: Option<Move>,
        searched_quiets: &[Move],
    ) {
        if ply < MAX_PLY && self.killers[ply][0] != Some(mv) {
            self.killers[ply][1] = self.killers[ply][0];
            self.killers[ply][0] = Some(mv);
        }
        let bonus = i32::from(depth).pow(2).min(HISTORY_BONUS_CAP);
        self.update_quiet_history(board, mv, prev_move, bonus);
        for &failed in searched_quiets {
            if failed != mv {
                self.update_quiet_history(board, failed, prev_move, -bonus);
            }
        }
        if let Some(prev) = prev_move {
            self.countermove[prev.from as usize][prev.to as usize] = encode_move(mv);
        }
    }

    fn update_quiet_history(
        &mut self,
        board: &Board,
        mv: Move,
        prev_move: Option<Move>,
        bonus: i32,
    ) {
        update_history(&mut self.history[mv.from as usize][mv.to as usize], bonus);
        if let Some(index) = continuation_index(board, prev_move, mv) {
            update_continuation(&mut self.continuation_history[index], bonus);
        }
    }
}

fn continuation_index(board: &Board, prev_move: Option<Move>, mv: Move) -> Option<usize> {
    let previous = prev_move?;
    let previous_piece = board.piece_on(previous.to).or_else(|| {
        // cozy-chess encodes castling as king-to-rook. After the move the
        // rook square is empty, so recover the moved piece from that unique
        // encoding instead of dropping the continuation sample.
        (previous.from.rank() == previous.to.rank()
            && (previous.from.file() as i8 - previous.to.file() as i8).abs() > 1)
            .then_some(Piece::King)
    })? as usize;
    let candidate_piece = board.piece_on(mv.from)? as usize;
    let previous_index = previous_piece * 64 + previous.to as usize;
    let candidate_index = candidate_piece * 64 + mv.to as usize;
    Some(previous_index * PIECE_TO_SQUARE + candidate_index)
}

fn capture_history_index(board: &Board, mv: Move) -> Option<usize> {
    if !is_capture(board, mv) {
        return None;
    }
    let color_piece = board.side_to_move() as usize * 6 + board.piece_on(mv.from)? as usize;
    let captured = board.piece_on(mv.to).unwrap_or(Piece::Pawn) as usize;
    Some((color_piece * 64 + mv.to as usize) * 6 + captured)
}

fn update_history(value: &mut i32, bonus: i32) {
    let bonus = bonus.clamp(-HISTORY_BONUS_CAP, HISTORY_BONUS_CAP);
    *value += bonus - *value * bonus.abs() / HISTORY_LIMIT;
    *value = (*value).clamp(-HISTORY_LIMIT, HISTORY_LIMIT);
}

fn update_continuation(value: &mut i16, bonus: i32) {
    let mut widened = i32::from(*value);
    update_history(&mut widened, bonus);
    *value = widened as i16;
}

#[cfg(test)]
mod tests {
    use cozy_chess::Board;

    use super::{
        CAPTURE_HISTORY_ENTRIES, CONTINUATION_HISTORY_ENTRIES, MovePicker, SearchCore,
        capture_history_index, continuation_index,
    };
    use crate::search::moves::{encode_move, legal_moves, played};

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
        let mut picker = MovePicker::new(&board, Some(tt_move), [None; 2], None, None, &[]);

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
        let mut picker = MovePicker::new(&board, Some(tt_move), killers, None, None, &[]);
        let mut picked = Vec::new();
        while let Some(mv) = picker.next(&search) {
            picked.push(encode_move(mv));
        }
        let mut legal: Vec<_> = legal_moves(&board).into_iter().map(encode_move).collect();
        picked.sort_unstable();
        legal.sort_unstable();

        assert_eq!(picked, legal);
    }

    #[test]
    fn continuation_history_uses_288_kib_and_rewards_the_cutoff_reply() {
        assert_eq!(
            CONTINUATION_HISTORY_ENTRIES * std::mem::size_of::<i16>(),
            288 * 1024
        );

        let root = Board::default();
        let previous = "e2e4".parse().unwrap();
        let board = played(&root, previous);
        let failed = "d7d5".parse().unwrap();
        let cutoff = "e7e5".parse().unwrap();
        let mut search = SearchCore::new();
        search.set_position(&board, &[root]);
        search.record_quiet_cutoff(&board, cutoff, 8, 1, Some(previous), &[failed, cutoff]);

        let cutoff_index = continuation_index(&board, Some(previous), cutoff).unwrap();
        let failed_index = continuation_index(&board, Some(previous), failed).unwrap();
        assert!(search.continuation_history[cutoff_index] > 0);
        assert!(search.continuation_history[failed_index] < 0);
        assert!(search.quiet_history_score(&board, cutoff, Some(previous)) > 0);
        assert!(search.quiet_history_score(&board, failed, Some(previous)) < 0);

        let before_decay = search.continuation_history[cutoff_index];
        search.set_position(&played(&board, cutoff), &[]);
        assert_eq!(search.continuation_history[cutoff_index], before_decay / 2);
    }

    #[test]
    fn capture_history_uses_9_kib_and_penalizes_failed_captures() {
        assert_eq!(
            CAPTURE_HISTORY_ENTRIES * std::mem::size_of::<i16>(),
            9 * 1024
        );
        let board = "4k3/8/8/2p1p3/3Q4/8/8/4K3 w - - 0 1"
            .parse::<Board>()
            .unwrap();
        let failed = "d4c5".parse().unwrap();
        let cutoff = "d4e5".parse().unwrap();
        let mut search = SearchCore::new();
        search.set_position(&board, &[]);
        search.record_capture_cutoff(&board, cutoff, 8, &[failed, cutoff]);

        let cutoff_index = capture_history_index(&board, cutoff).unwrap();
        let failed_index = capture_history_index(&board, failed).unwrap();
        assert!(search.capture_history[cutoff_index] > 0);
        assert!(search.capture_history[failed_index] < 0);
        assert!(search.capture_history_score(&board, cutoff) > 0);
        assert!(search.capture_history_score(&board, failed) < 0);

        let before_decay = search.capture_history[cutoff_index];
        search.set_position(&played(&board, cutoff), &[]);
        assert_eq!(search.capture_history[cutoff_index], before_decay / 2);
    }
}
