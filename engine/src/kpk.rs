//! Compute-at-initialization king-and-pawn-versus-king bitbase.

use std::sync::OnceLock;

use cozy_chess::{Board, Color, Piece, Rank, Square, get_king_moves, get_pawn_attacks};

const PAWN_FILES: usize = 4;
const PAWN_RANKS: usize = 6;
const PAWN_SQUARES: usize = PAWN_FILES * PAWN_RANKS;
const STATE_COUNT: usize = 2 * PAWN_SQUARES * 64 * 64;
const BITBASE_BYTES: usize = STATE_COUNT / 8;

static KPK: OnceLock<KpkBitbase> = OnceLock::new();

pub(crate) fn initialize() {
    let _ = KPK.get_or_init(KpkBitbase::generate);
}

/// Returns whether the pawn side wins, or `None` outside exact KPK material.
pub(crate) fn probe(board: &Board) -> Option<bool> {
    normalized(board).map(|state| KPK.get_or_init(KpkBitbase::generate).wins(state))
}

struct KpkBitbase {
    wins: Box<[u8]>,
}

#[derive(Clone, Copy)]
struct State {
    side: Color,
    pawn: Square,
    pawn_king: Square,
    defending_king: Square,
}

impl KpkBitbase {
    fn generate() -> Self {
        let mut wins = vec![false; STATE_COUNT];
        loop {
            let mut changed = false;
            for index in 0..STATE_COUNT {
                if wins[index] {
                    continue;
                }
                let state = decode(index);
                if valid(state) && is_winning(state, &wins) {
                    wins[index] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let mut packed = vec![0_u8; BITBASE_BYTES];
        for (index, won) in wins.into_iter().enumerate() {
            if won {
                packed[index / 8] |= 1 << (index % 8);
            }
        }
        Self {
            wins: packed.into_boxed_slice(),
        }
    }

    fn wins(&self, state: State) -> bool {
        let index = encode(state);
        self.wins[index / 8] & (1 << (index % 8)) != 0
    }
}

fn is_winning(state: State, wins: &[bool]) -> bool {
    match state.side {
        Color::White => white_can_force_win(state, wins),
        Color::Black => black_cannot_avoid_win(state, wins),
    }
}

fn white_can_force_win(state: State, wins: &[bool]) -> bool {
    for to in get_king_moves(state.pawn_king) {
        let child = State {
            side: Color::Black,
            pawn_king: to,
            ..state
        };
        if valid(child) && wins[encode(child)] {
            return true;
        }
    }

    let one_rank = state.pawn.rank() as usize + 1;
    let one = Square::new(state.pawn.file(), Rank::ALL[one_rank]);
    if one != state.pawn_king && one != state.defending_king {
        if one.rank() == Rank::Eighth {
            if king_distance(state.defending_king, one) > 1
                || king_distance(state.pawn_king, one) == 1
            {
                return true;
            }
        } else {
            let child = State {
                side: Color::Black,
                pawn: one,
                ..state
            };
            if valid(child) && wins[encode(child)] {
                return true;
            }
            if state.pawn.rank() == Rank::Second {
                let two = Square::new(state.pawn.file(), Rank::Fourth);
                let child = State {
                    side: Color::Black,
                    pawn: two,
                    ..state
                };
                if two != state.pawn_king
                    && two != state.defending_king
                    && valid(child)
                    && wins[encode(child)]
                {
                    return true;
                }
            }
        }
    }
    false
}

fn black_cannot_avoid_win(state: State, wins: &[bool]) -> bool {
    let mut legal_moves = 0;
    for to in get_king_moves(state.defending_king) {
        if to == state.pawn_king || king_distance(to, state.pawn_king) <= 1 {
            continue;
        }
        if to == state.pawn {
            // Capturing an unprotected pawn is an immediate draw.
            if king_distance(to, state.pawn_king) > 1 {
                return false;
            }
            continue;
        }
        if get_pawn_attacks(state.pawn, Color::White).has(to) {
            continue;
        }
        legal_moves += 1;
        let child = State {
            side: Color::White,
            defending_king: to,
            ..state
        };
        if !valid(child) || !wins[encode(child)] {
            return false;
        }
    }
    if legal_moves == 0 {
        // No legal move is a win only when the pawn is delivering mate;
        // otherwise it is stalemate.
        get_pawn_attacks(state.pawn, Color::White).has(state.defending_king)
    } else {
        true
    }
}

fn valid(state: State) -> bool {
    (state.pawn.file() as usize) < PAWN_FILES
        && (Rank::Second..=Rank::Seventh).contains(&state.pawn.rank())
        && state.pawn != state.pawn_king
        && state.pawn != state.defending_king
        && state.pawn_king != state.defending_king
        && king_distance(state.pawn_king, state.defending_king) > 1
        && !(state.side == Color::White
            && get_pawn_attacks(state.pawn, Color::White).has(state.defending_king))
}

fn encode(state: State) -> usize {
    let pawn_index = state.pawn.file() as usize * PAWN_RANKS + state.pawn.rank() as usize - 1;
    (((state.side as usize * PAWN_SQUARES + pawn_index) * 64 + state.pawn_king as usize) * 64)
        + state.defending_king as usize
}

fn decode(mut index: usize) -> State {
    let defending_king = Square::ALL[index % 64];
    index /= 64;
    let pawn_king = Square::ALL[index % 64];
    index /= 64;
    let pawn_index = index % PAWN_SQUARES;
    let side = Color::ALL[index / PAWN_SQUARES];
    let pawn = Square::new(
        cozy_chess::File::ALL[pawn_index / PAWN_RANKS],
        Rank::ALL[pawn_index % PAWN_RANKS + 1],
    );
    State {
        side,
        pawn,
        pawn_king,
        defending_king,
    }
}

fn normalized(board: &Board) -> Option<State> {
    if board.occupied().len() != 3 || board.pieces(Piece::Pawn).len() != 1 {
        return None;
    }
    let pawn = board.pieces(Piece::Pawn).into_iter().next()?;
    let pawn_color = board.color_on(pawn)?;
    let mut state = State {
        side: if board.side_to_move() == pawn_color {
            Color::White
        } else {
            Color::Black
        },
        pawn: orient(pawn, pawn_color),
        pawn_king: orient(board.king(pawn_color), pawn_color),
        defending_king: orient(board.king(!pawn_color), pawn_color),
    };
    if state.pawn.file() as usize >= PAWN_FILES {
        state.pawn = mirror_file(state.pawn);
        state.pawn_king = mirror_file(state.pawn_king);
        state.defending_king = mirror_file(state.defending_king);
    }
    valid(state).then_some(state)
}

fn orient(square: Square, pawn_color: Color) -> Square {
    Square::ALL[if pawn_color == Color::White {
        square as usize
    } else {
        square as usize ^ 56
    }]
}

fn mirror_file(square: Square) -> Square {
    Square::ALL[square as usize ^ 7]
}

fn king_distance(a: Square, b: Square) -> i32 {
    ((a.file() as i32 - b.file() as i32).abs()).max((a.rank() as i32 - b.rank() as i32).abs())
}

#[cfg(test)]
mod tests {
    use crate::{eval::evaluate, search::SearchCore};

    use super::*;

    #[test]
    fn generated_table_has_the_promised_24_kib_footprint() {
        let bitbase = KpkBitbase::generate();
        assert_eq!(bitbase.wins.len(), 24 * 1024);
    }

    #[test]
    fn distinguishes_safe_promotion_from_rook_pawn_stalemate() {
        let win = "8/kPK5/8/8/8/8/8/8 w - - 0 1".parse::<Board>().unwrap();
        let draw = "k7/P7/1K6/8/8/8/8/8 b - - 0 1".parse::<Board>().unwrap();
        assert_eq!(probe(&win), Some(true));
        assert_eq!(probe(&draw), Some(false));
        assert!(evaluate(&win) > 9_000);
        assert_eq!(evaluate(&draw), 0);
    }

    #[test]
    fn normalization_handles_black_pawns_and_both_board_wings() {
        let white = "8/kPK5/8/8/8/8/8/8 w - - 0 1".parse::<Board>().unwrap();
        let mirrored = "8/5KPk/8/8/8/8/8/8 w - - 0 1".parse::<Board>().unwrap();
        let black = "8/8/8/8/8/8/Kpk5/8 b - - 0 1".parse::<Board>().unwrap();
        assert_eq!(probe(&white), probe(&mirrored));
        assert_eq!(probe(&white), probe(&black));
    }

    #[test]
    fn search_returns_an_exact_draw_for_a_nonterminal_rook_pawn_fortress() {
        let board = "k7/8/PK6/8/8/8/8/8 w - - 0 1".parse::<Board>().unwrap();
        assert_eq!(probe(&board), Some(false));
        let mut search = SearchCore::new();
        search.set_position(&board, &[]);
        let result = search.analyze_depth(&board, 4, 1, 5_000.0);
        assert_eq!(result.lines[0].score, 0);
    }
}
