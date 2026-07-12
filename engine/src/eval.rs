use cozy_chess::{
    Board, Color, File, Piece, Rank, Square, get_bishop_moves, get_king_moves, get_knight_moves,
    get_rook_moves,
};

// Tapered PeSTO-style piece values (indexed by `Piece as usize`), used only
// by `evaluate` below. `piece_value` (a separate, phase-independent scale)
// is used by search.rs for SEE/move-ordering/capture pruning, where a single
// canonical per-piece value is wanted for comparing exchange sequences
// rather than a positionally-tapered one. The two scales are intentionally
// different and not meant to be kept in sync with each other.
const MG_VALUE: [i32; 6] = [82, 337, 365, 477, 1025, 0];
const EG_VALUE: [i32; 6] = [94, 281, 297, 512, 936, 0];
const PHASE: [i32; 6] = [0, 1, 1, 2, 4, 0];

// --- Positional bonus/penalty constants (values intentionally unchanged
// during cleanup; retuning these is a strength-tuning decision that can't
// be validated without a self-play/SPRT harness, which this repo lacks) ---
const DOUBLED_PAWN_PENALTY_MG: i32 = 12;
const DOUBLED_PAWN_PENALTY_EG: i32 = 18;
const ISOLATED_PAWN_PENALTY_MG: i32 = 10;
const ISOLATED_PAWN_PENALTY_EG: i32 = 14;
const BISHOP_PAIR_BONUS_MG: i32 = 30;
const BISHOP_PAIR_BONUS_EG: i32 = 45;
const ROOK_OPEN_FILE_BONUS_MG: i32 = 20;
const ROOK_OPEN_FILE_BONUS_EG: i32 = 10;
const ROOK_SEMI_OPEN_FILE_BONUS_MG: i32 = 10;
const ROOK_SEMI_OPEN_FILE_BONUS_EG: i32 = 5;
const KING_SHIELD_PAWN_BONUS_MG: i32 = 9;
const TEMPO_BONUS: i32 = 10;
// Bare-king mating guidance. These terms give shallow searches a useful
// gradient in endings where material alone cannot distinguish progress.
const MOP_UP_EDGE_WEIGHT: i32 = 35;
const MOP_UP_KING_WEIGHT: i32 = 20;
const ROOK_CONFINEMENT_WEIGHT: i32 = 12;
const BN_CORNER_WEIGHT: i32 = 45;
// Mobility bonus per reachable square, by piece (knight/bishop/rook/queen).
const KNIGHT_MOBILITY_WEIGHT: i32 = 4;
const BISHOP_MOBILITY_WEIGHT: i32 = 4;
const ROOK_MOBILITY_WEIGHT: i32 = 2;
const QUEEN_MOBILITY_WEIGHT: i32 = 1;
// King-zone attacker bonus per square a piece attacks within the enemy king's
// own square plus its 8 neighbors (tropism), by piece.
const KING_ZONE_KNIGHT_WEIGHT_MG: i32 = 3;
const KING_ZONE_BISHOP_WEIGHT_MG: i32 = 3;
const KING_ZONE_ROOK_WEIGHT_MG: i32 = 2;
const KING_ZONE_QUEEN_WEIGHT_MG: i32 = 4;
// Passed-pawn bonus by rank relative to the pawn's own side (0 = own back rank).
const MG_PASSER: [i32; 8] = [0, 5, 10, 20, 35, 60, 100, 0];
const EG_PASSER: [i32; 8] = [0, 10, 20, 40, 70, 120, 200, 0];
// Endgame-only: reward a passer for the defending king being far away and the
// attacking king being close, the classic "king escorts / king races" rule.
const PASSER_OWN_KING_DIST_WEIGHT_EG: i32 = 5;
const PASSER_ENEMY_KING_DIST_WEIGHT_EG: i32 = 10;

// Compact PeSTO-style piece-square model. Tables are indexed a8..h1.
#[rustfmt::skip]
const MG_PST: [[i16; 64]; 6] = [
    [0,0,0,0,0,0,0,0,98,134,61,95,68,126,34,-11,-6,7,26,31,65,56,25,-20,-14,13,6,21,23,12,17,-23,-27,-2,-5,12,17,6,10,-25,-26,-4,-4,-10,3,3,33,-12,-35,-1,-20,-23,-15,24,38,-22,0,0,0,0,0,0,0,0],
    [-167,-89,-34,-49,61,-97,-15,-107,-73,-41,72,36,23,62,7,-17,-47,60,37,65,84,129,73,44,-9,17,19,53,37,69,18,22,-13,4,16,13,28,19,21,-8,-23,-9,12,10,19,17,25,-16,-29,-53,-12,-3,-1,18,-14,-19,-105,-21,-58,-33,-17,-28,-19,-23],
    [-29,4,-82,-37,-25,-42,7,-8,-26,16,-18,-13,30,59,18,-47,-16,37,43,40,35,50,37,-2,-4,5,19,50,37,37,7,-2,-6,13,13,26,34,12,10,4,0,15,15,15,14,27,18,10,4,15,16,0,7,21,33,1,-33,-3,-14,-21,-13,-12,-39,-21],
    [32,42,32,51,63,9,31,43,27,32,58,62,80,67,26,44,-5,19,26,36,17,45,61,16,-24,-11,7,26,24,35,-8,-20,-36,-26,-12,-1,9,-7,6,-23,-45,-25,-16,-17,3,0,-5,-33,-44,-16,-20,-9,-1,11,-6,-71,-19,-13,1,17,16,7,-37,-26],
    [-28,0,29,12,59,44,43,45,-24,-39,-5,1,-16,57,28,54,-13,-17,7,8,29,56,47,57,-27,-27,-16,-16,-1,17,-2,1,-9,-26,-9,-10,-2,-4,3,-3,-14,2,-11,-2,-5,2,14,5,-35,-8,11,2,8,15,-3,1,-1,-18,-9,10,-15,-25,-31,-50],
    [-65,23,16,-15,-56,-34,2,13,29,-1,-20,-7,-8,-4,-38,-29,-9,24,2,-16,-20,6,22,-22,-17,-20,-12,-27,-30,-25,-14,-36,-49,-1,-27,-39,-46,-44,-33,-51,-14,-14,-22,-46,-44,-30,-15,-27,1,7,-8,-64,-43,-16,9,8,-15,36,12,-54,8,-28,24,14],
];

#[rustfmt::skip]
const EG_PST: [[i16; 64]; 6] = [
    [0,0,0,0,0,0,0,0,178,173,158,134,147,132,165,187,94,100,85,67,56,53,82,84,32,24,13,5,-2,4,17,17,13,9,-3,-7,-7,-8,3,-1,4,7,-6,1,0,-5,-1,-8,13,8,8,10,13,0,2,-7,0,0,0,0,0,0,0,0],
    [-58,-38,-13,-28,-31,-27,-63,-99,-25,-8,-25,-2,-9,-25,-24,-52,-24,-20,10,9,-1,-9,-19,-41,-17,3,22,22,22,11,8,-18,-18,-6,16,25,16,17,4,-18,-23,-3,-1,15,10,-3,-20,-22,-42,-20,-10,-5,-2,-20,-23,-44,-29,-51,-23,-15,-22,-18,-50,-64],
    [-14,-21,-11,-8,-7,-9,-17,-24,-8,-4,7,-12,-3,-13,-4,-14,2,-8,0,-1,-2,6,0,4,-3,9,12,9,14,10,3,2,-6,3,13,19,7,10,-3,-9,-12,-3,8,10,13,3,-7,-15,-14,-18,-7,-1,4,-9,-15,-27,-23,-9,-23,-5,-9,-16,-5,-17],
    [13,10,18,15,12,12,8,5,11,13,13,11,-3,3,8,3,7,7,7,5,4,-3,-5,-3,4,3,13,1,2,1,-1,2,3,5,8,4,-5,-6,-8,-11,-4,0,-5,-1,-7,-12,-8,-16,-6,-6,0,2,-9,-9,-11,-3,-9,2,3,-1,-5,-13,4,-20],
    [-9,22,22,27,27,19,10,20,-17,20,32,41,58,25,30,0,-20,6,9,49,47,35,19,9,3,22,24,45,57,40,57,36,-18,28,19,47,31,34,39,23,-16,-27,15,6,9,17,10,5,-22,-23,-30,-16,-16,-23,-36,-32,-33,-28,-22,-43,-5,-32,-20,-41],
    [-74,-35,-18,-18,-11,15,4,-17,-12,17,14,17,17,38,23,11,10,17,23,15,20,45,44,13,-8,22,24,27,26,33,26,3,-18,-4,21,24,27,23,9,-11,-19,-3,11,21,23,16,7,-9,-27,-11,4,13,14,4,-5,-17,-53,-34,-21,-11,-28,-14,-24,-43],
];

// Simple, phase-independent piece values for exchange-sequence comparison
// (SEE, capture ordering, capture-value pruning in search.rs). Deliberately
// separate from `MG_VALUE`/`EG_VALUE` above; see the comment there.
pub(crate) fn piece_value(piece: Piece) -> i32 {
    [100, 320, 330, 500, 900, 20_000][piece as usize]
}

pub(crate) fn evaluate(board: &Board) -> i32 {
    let mut mg = 0;
    let mut eg = 0;
    let mut phase = 0;
    let occupied = board.occupied();
    let king_zone = [
        get_king_moves(board.king(Color::White)) | board.king(Color::White).bitboard(),
        get_king_moves(board.king(Color::Black)) | board.king(Color::Black).bitboard(),
    ];

    for piece in Piece::ALL {
        for color in [Color::White, Color::Black] {
            let sign = if color == Color::White { 1 } else { -1 };
            for square in board.colored_pieces(color, piece) {
                let index = if color == Color::White {
                    (square as usize) ^ 56
                } else {
                    square as usize
                };
                mg += sign * (MG_VALUE[piece as usize] + i32::from(MG_PST[piece as usize][index]));
                eg += sign * (EG_VALUE[piece as usize] + i32::from(EG_PST[piece as usize][index]));
                phase += PHASE[piece as usize];

                let own = board.colors(color);
                let raw_attacks = match piece {
                    Piece::Knight => Some(get_knight_moves(square)),
                    Piece::Bishop => Some(get_bishop_moves(square, occupied)),
                    Piece::Rook => Some(get_rook_moves(square, occupied)),
                    Piece::Queen => {
                        Some(get_bishop_moves(square, occupied) | get_rook_moves(square, occupied))
                    }
                    _ => None,
                };
                if let Some(raw_attacks) = raw_attacks {
                    let mobility_weight = match piece {
                        Piece::Knight => KNIGHT_MOBILITY_WEIGHT,
                        Piece::Bishop => BISHOP_MOBILITY_WEIGHT,
                        Piece::Rook => ROOK_MOBILITY_WEIGHT,
                        Piece::Queen => QUEEN_MOBILITY_WEIGHT,
                        _ => 0,
                    };
                    let mobility = (raw_attacks & !own).len() as i32 * mobility_weight;
                    mg += sign * mobility;
                    eg += sign * mobility;

                    let king_zone_weight = match piece {
                        Piece::Knight => KING_ZONE_KNIGHT_WEIGHT_MG,
                        Piece::Bishop => KING_ZONE_BISHOP_WEIGHT_MG,
                        Piece::Rook => KING_ZONE_ROOK_WEIGHT_MG,
                        Piece::Queen => KING_ZONE_QUEEN_WEIGHT_MG,
                        _ => 0,
                    };
                    let zone_hits = (raw_attacks & king_zone[!color as usize]).len() as i32;
                    mg += sign * zone_hits * king_zone_weight;
                }
            }
        }
    }

    for color in [Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        let pawns = board.colored_pieces(color, Piece::Pawn);
        let enemy_pawns = board.colored_pieces(!color, Piece::Pawn);
        let mut file_counts = [0_i32; 8];
        for pawn in pawns {
            file_counts[pawn.file() as usize] += 1;
        }
        for count in file_counts {
            if count > 1 {
                mg -= sign * (count - 1) * DOUBLED_PAWN_PENALTY_MG;
                eg -= sign * (count - 1) * DOUBLED_PAWN_PENALTY_EG;
            }
        }
        for pawn in pawns {
            let file = pawn.file() as i32;
            let rank = pawn.rank() as i32;
            let isolated = (file == 0 || file_counts[(file - 1) as usize] == 0)
                && (file == 7 || file_counts[(file + 1) as usize] == 0);
            if isolated {
                mg -= sign * ISOLATED_PAWN_PENALTY_MG;
                eg -= sign * ISOLATED_PAWN_PENALTY_EG;
            }
            let passed = enemy_pawns.into_iter().all(|enemy| {
                let close_file = ((enemy.file() as i32) - file).abs() <= 1;
                let ahead = if color == Color::White {
                    (enemy.rank() as i32) > rank
                } else {
                    (enemy.rank() as i32) < rank
                };
                !(close_file && ahead)
            });
            if passed {
                let relative_rank = if color == Color::White {
                    rank
                } else {
                    7 - rank
                } as usize;
                mg += sign * MG_PASSER[relative_rank];
                eg += sign * EG_PASSER[relative_rank];
                let own_king_dist = chebyshev_distance(board.king(color), pawn);
                let enemy_king_dist = chebyshev_distance(board.king(!color), pawn);
                eg += sign
                    * (enemy_king_dist * PASSER_ENEMY_KING_DIST_WEIGHT_EG
                        - own_king_dist * PASSER_OWN_KING_DIST_WEIGHT_EG);
            }
        }

        if board.colored_pieces(color, Piece::Bishop).len() >= 2 {
            mg += sign * BISHOP_PAIR_BONUS_MG;
            eg += sign * BISHOP_PAIR_BONUS_EG;
        }
        for rook in board.colored_pieces(color, Piece::Rook) {
            let file = rook.file() as usize;
            if file_counts[file] == 0 {
                let enemy_on_file = enemy_pawns
                    .into_iter()
                    .any(|pawn| pawn.file() as usize == file);
                mg += sign
                    * if enemy_on_file {
                        ROOK_SEMI_OPEN_FILE_BONUS_MG
                    } else {
                        ROOK_OPEN_FILE_BONUS_MG
                    };
                eg += sign
                    * if enemy_on_file {
                        ROOK_SEMI_OPEN_FILE_BONUS_EG
                    } else {
                        ROOK_OPEN_FILE_BONUS_EG
                    };
            }
        }

        let king = board.king(color);
        let king_rank = king.rank() as i32;
        let shield_rank = king_rank + if color == Color::White { 1 } else { -1 };
        if (0..8).contains(&shield_rank) {
            for file_delta in -1..=1 {
                let file = king.file() as i32 + file_delta;
                if (0..8).contains(&file) {
                    let square =
                        Square::new(File::ALL[file as usize], Rank::ALL[shield_rank as usize]);
                    if board.piece_on(square) == Some(Piece::Pawn)
                        && board.color_on(square) == Some(color)
                    {
                        mg += sign * KING_SHIELD_PAWN_BONUS_MG;
                    }
                }
            }
        }
    }

    // Once one side has only a king, reward the winning side for taking away
    // space and walking its king in. Without this, most K+R/K+Q moves have
    // almost identical material scores until mate happens to enter the search
    // horizon. Bishop-and-knight is special: its king must be driven to a
    // corner controlled by the bishop, not merely to any edge.
    eg += bare_king_mating_bonus(board, Color::White);
    eg -= bare_king_mating_bonus(board, Color::Black);

    phase = phase.min(24);
    let white_score = (mg * phase + eg * (24 - phase)) / 24;
    if board.side_to_move() == Color::White {
        white_score + TEMPO_BONUS
    } else {
        -white_score + TEMPO_BONUS
    }
}

fn bare_king_mating_bonus(board: &Board, attacker: Color) -> i32 {
    let defender = !attacker;
    if board.colors(defender).len() != 1 {
        return 0;
    }

    let queens = board.colored_pieces(attacker, Piece::Queen);
    let rooks = board.colored_pieces(attacker, Piece::Rook);
    let bishops = board.colored_pieces(attacker, Piece::Bishop);
    let knights = board.colored_pieces(attacker, Piece::Knight);
    let has_major = !(queens | rooks).is_empty();
    let bishop_and_knight = bishops.len() == 1
        && knights.len() == 1
        && !has_major
        && board.colored_pieces(attacker, Piece::Pawn).is_empty();
    if !has_major && !bishop_and_knight && bishops.len() < 2 {
        return 0;
    }

    let attacking_king = board.king(attacker);
    let defending_king = board.king(defender);
    let king_distance = chebyshev_distance(attacking_king, defending_king);
    let mut bonus = (7 - king_distance) * MOP_UP_KING_WEIGHT;

    if bishop_and_knight {
        let bishop = bishops
            .into_iter()
            .next()
            .expect("one bishop was established");
        let bishop_color = (bishop.file() as i32 + bishop.rank() as i32) & 1;
        let corner_distance = [Square::A1, Square::H1, Square::A8, Square::H8]
            .into_iter()
            .filter(|corner| (corner.file() as i32 + corner.rank() as i32) & 1 == bishop_color)
            .map(|corner| chebyshev_distance(defending_king, corner))
            .min()
            .unwrap_or(7);
        bonus += (7 - corner_distance) * BN_CORNER_WEIGHT;
    } else {
        let edge_distance = (defending_king.file() as i32)
            .min(7 - defending_king.file() as i32)
            .min(defending_king.rank() as i32)
            .min(7 - defending_king.rank() as i32);
        bonus += (3 - edge_distance) * MOP_UP_EDGE_WEIGHT;

        // A rook/queen need not check to make progress: placing it across the
        // king cuts off whole ranks or files. Prefer the smaller resulting box.
        for piece in rooks | queens {
            let file_box = if defending_king.file() < piece.file() {
                piece.file() as i32
            } else if defending_king.file() > piece.file() {
                7 - piece.file() as i32
            } else {
                7
            };
            let rank_box = if defending_king.rank() < piece.rank() {
                piece.rank() as i32
            } else if defending_king.rank() > piece.rank() {
                7 - piece.rank() as i32
            } else {
                7
            };
            bonus += (7 - file_box.min(rank_box)) * ROOK_CONFINEMENT_WEIGHT;
        }
    }
    bonus
}

fn chebyshev_distance(a: Square, b: Square) -> i32 {
    ((a.file() as i32 - b.file() as i32).abs()).max((a.rank() as i32 - b.rank() as i32).abs())
}

pub(crate) fn insufficient_material(board: &Board) -> bool {
    if !(board.pieces(Piece::Pawn) | board.pieces(Piece::Rook) | board.pieces(Piece::Queen))
        .is_empty()
    {
        return false;
    }
    let bishops = board.pieces(Piece::Bishop);
    let knights = board.pieces(Piece::Knight);
    let minor_count = bishops.len() + knights.len();
    if minor_count <= 1 {
        return true;
    }
    if knights.is_empty() {
        // Any number of bishops confined to one square color can never cover
        // both colors around a king. Opposite-colored bishops are not dead:
        // one side's bishop can block a flight square for the other side.
        let mut square_colors = bishops
            .into_iter()
            .map(|square| (square.file() as u8 + square.rank() as u8) % 2);
        if let Some(first) = square_colors.next() {
            return square_colors.all(|color| color == first);
        }
    }
    false
}
