use cozy_chess::{
    BitBoard, Board, Color, File, Piece, Rank, Square, get_bishop_moves, get_king_moves,
    get_knight_moves, get_pawn_attacks, get_rook_moves,
};

use crate::eval_tuned::DELTAS;

#[cfg(feature = "tuning")]
pub(crate) const PARAMETER_COUNT: usize = 855;
const MG_VALUE_START: usize = 0;
const EG_VALUE_START: usize = 6;
const MG_PST_START: usize = 12;
const EG_PST_START: usize = 396;
const DOUBLED_MG: usize = 780;
const DOUBLED_EG: usize = 781;
const ISOLATED_MG: usize = 782;
const ISOLATED_EG: usize = 783;
const BISHOP_PAIR_MG: usize = 784;
const BISHOP_PAIR_EG: usize = 785;
const ROOK_OPEN_MG: usize = 786;
const ROOK_OPEN_EG: usize = 787;
const ROOK_SEMI_OPEN_MG: usize = 788;
const ROOK_SEMI_OPEN_EG: usize = 789;
const TEMPO: usize = 790;
const MOP_EDGE: usize = 791;
const MOP_KING: usize = 792;
const ROOK_CONFINEMENT: usize = 793;
const BN_CORNER: usize = 794;
const MOBILITY_START: usize = 795;
const KING_ATTACK_CURVE_START: usize = 799;
const KING_ATTACK_CURVE_LEN: usize = 32;
const MG_PASSER_START: usize = 831;
const EG_PASSER_START: usize = 839;
const PASSER_OWN_KING: usize = 847;
const PASSER_ENEMY_KING: usize = 848;
const PAWN_THREAT_MG: usize = 849;
const PAWN_THREAT_EG: usize = 850;
const MINOR_MAJOR_THREAT_MG: usize = 851;
const MINOR_MAJOR_THREAT_EG: usize = 852;
const HANGING_THREAT_MG: usize = 853;
const HANGING_THREAT_EG: usize = 854;

const fn tuned(base: i32, parameter: usize) -> i32 {
    base + DELTAS[parameter] as i32
}

type PackedScore = i32;

const fn pack(mg: i32, eg: i32) -> PackedScore {
    (eg << 16) + mg
}

fn mg_value(score: PackedScore) -> i32 {
    i32::from(score as i16)
}

fn eg_value(score: PackedScore) -> i32 {
    i32::from((score.wrapping_add(0x8000) >> 16) as i16)
}

// Tapered PeSTO-style piece values (indexed by `Piece as usize`), used only
// by `evaluate` below. `piece_value` (a separate, phase-independent scale)
// is used by search.rs for SEE/move-ordering/capture pruning, where a single
// canonical per-piece value is wanted for comparing exchange sequences
// rather than a positionally-tapered one. The two scales are intentionally
// different and not meant to be kept in sync with each other.
const MG_VALUE: [i32; 6] = [82, 337, 365, 477, 1025, 0];
const EG_VALUE: [i32; 6] = [94, 281, 297, 512, 936, 0];
const PHASE: [i32; 6] = [0, 1, 1, 2, 4, 0];

// Hand-authored base weights. The generated delta table can tune each one
// without making this compact, readable model into generated source.
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
// Nonlinear king-danger score indexed by accumulated attack units.
const KING_ATTACK_CURVE: [i32; KING_ATTACK_CURVE_LEN] = [
    0, 0, 0, 1, 2, 4, 7, 11, 16, 22, 29, 37, 46, 56, 67, 79, 92, 106, 121, 137, 154, 172, 191, 211,
    232, 254, 277, 301, 326, 352, 379, 407,
];
// Passed-pawn bonus by rank relative to the pawn's own side (0 = own back rank).
const MG_PASSER: [i32; 8] = [0, 5, 10, 20, 35, 60, 100, 0];
const EG_PASSER: [i32; 8] = [0, 10, 20, 40, 70, 120, 200, 0];
// Endgame-only: reward a passer for the defending king being far away and the
// attacking king being close, the classic "king escorts / king races" rule.
const PASSER_OWN_KING_DIST_WEIGHT_EG: i32 = 5;
const PASSER_ENEMY_KING_DIST_WEIGHT_EG: i32 = 10;
const PAWN_THREAT_BONUS_MG: i32 = 12;
const PAWN_THREAT_BONUS_EG: i32 = 18;
const MINOR_MAJOR_THREAT_BONUS_MG: i32 = 16;
const MINOR_MAJOR_THREAT_BONUS_EG: i32 = 12;
const HANGING_THREAT_BONUS_MG: i32 = 20;
const HANGING_THREAT_BONUS_EG: i32 = 24;
const KPK_WIN_SCORE: i32 = 10_000;

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

const PACKED_VALUE: [PackedScore; 6] = build_packed_values();
const PACKED_PST: [[PackedScore; 64]; 6] = build_packed_pst();

const fn build_packed_values() -> [PackedScore; 6] {
    let mut values = [0; 6];
    let mut piece = 0;
    while piece < 6 {
        values[piece] = pack(
            tuned(MG_VALUE[piece], MG_VALUE_START + piece),
            tuned(EG_VALUE[piece], EG_VALUE_START + piece),
        );
        piece += 1;
    }
    values
}

const fn build_packed_pst() -> [[PackedScore; 64]; 6] {
    let mut table = [[0; 64]; 6];
    let mut piece = 0;
    while piece < 6 {
        let mut square = 0;
        while square < 64 {
            table[piece][square] = pack(
                tuned(
                    MG_PST[piece][square] as i32,
                    MG_PST_START + piece * 64 + square,
                ),
                tuned(
                    EG_PST[piece][square] as i32,
                    EG_PST_START + piece * 64 + square,
                ),
            );
            square += 1;
        }
        piece += 1;
    }
    table
}

// Simple, phase-independent piece values for exchange-sequence comparison
// (SEE, capture ordering, capture-value pruning in search.rs). Deliberately
// separate from `MG_VALUE`/`EG_VALUE` above; see the comment there.
pub(crate) fn piece_value(piece: Piece) -> i32 {
    [100, 320, 330, 500, 900, 20_000][piece as usize]
}

pub(crate) fn evaluate(board: &Board) -> i32 {
    evaluate_with_pawns(board, pawn_structure(board))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PawnStructure {
    score: PackedScore,
    passers: [BitBoard; 2],
}

pub(crate) fn pawn_structure_key(board: &Board) -> u64 {
    let mut key = 0_u64;
    for color in [Color::White, Color::Black] {
        for square in board.colored_pieces(color, Piece::Pawn) {
            key ^= splitmix64(square as u64 + 64 * color as u64 + 1);
        }
    }
    key
}

const fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

pub(crate) fn pawn_structure(board: &Board) -> PawnStructure {
    let mut structure = PawnStructure::default();
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
                structure.score -= sign
                    * (count - 1)
                    * pack(
                        tuned(DOUBLED_PAWN_PENALTY_MG, DOUBLED_MG),
                        tuned(DOUBLED_PAWN_PENALTY_EG, DOUBLED_EG),
                    );
            }
        }
        for pawn in pawns {
            let file = pawn.file() as i32;
            let rank = pawn.rank() as i32;
            let isolated = (file == 0 || file_counts[(file - 1) as usize] == 0)
                && (file == 7 || file_counts[(file + 1) as usize] == 0);
            if isolated {
                structure.score -= sign
                    * pack(
                        tuned(ISOLATED_PAWN_PENALTY_MG, ISOLATED_MG),
                        tuned(ISOLATED_PAWN_PENALTY_EG, ISOLATED_EG),
                    );
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
                structure.passers[color as usize] |= pawn.bitboard();
                let relative_rank = if color == Color::White {
                    rank
                } else {
                    7 - rank
                } as usize;
                structure.score += sign
                    * pack(
                        tuned(MG_PASSER[relative_rank], MG_PASSER_START + relative_rank),
                        tuned(EG_PASSER[relative_rank], EG_PASSER_START + relative_rank),
                    );
            }
        }
    }
    structure
}

pub(crate) fn evaluate_with_pawns(board: &Board, pawn_structure: PawnStructure) -> i32 {
    let mut score: PackedScore = 0;
    let mut phase = 0;
    let occupied = board.occupied();
    let king_zone = [
        get_king_moves(board.king(Color::White)) | board.king(Color::White).bitboard(),
        get_king_moves(board.king(Color::Black)) | board.king(Color::Black).bitboard(),
    ];
    let pawn_attacks = [
        pawn_attacks(board, Color::White),
        pawn_attacks(board, Color::Black),
    ];
    let mut all_attacks = pawn_attacks;
    let mut minor_attacks = [BitBoard::EMPTY; 2];
    let mut king_attack_units = [0_i32; 2];
    let mut king_attackers = [0_i32; 2];
    let mut king_zone_attacks = [BitBoard::EMPTY; 2];

    for piece in Piece::ALL {
        for color in [Color::White, Color::Black] {
            let sign = if color == Color::White { 1 } else { -1 };
            for square in board.colored_pieces(color, piece) {
                let index = if color == Color::White {
                    (square as usize) ^ 56
                } else {
                    square as usize
                };
                let piece_index = piece as usize;
                score += sign * (PACKED_VALUE[piece_index] + PACKED_PST[piece_index][index]);
                phase += PHASE[piece as usize];

                let own = board.colors(color);
                let raw_attacks = match piece {
                    Piece::Pawn => get_pawn_attacks(square, color),
                    Piece::Knight => get_knight_moves(square),
                    Piece::Bishop => get_bishop_moves(square, occupied),
                    Piece::Rook => get_rook_moves(square, occupied),
                    Piece::Queen => {
                        get_bishop_moves(square, occupied) | get_rook_moves(square, occupied)
                    }
                    Piece::King => get_king_moves(square),
                };
                all_attacks[color as usize] |= raw_attacks;
                if matches!(piece, Piece::Knight | Piece::Bishop) {
                    minor_attacks[color as usize] |= raw_attacks;
                }
                if matches!(piece, Piece::Pawn | Piece::King) {
                    continue;
                }
                let (mobility_weight, mobility_parameter) = match piece {
                    Piece::Knight => (KNIGHT_MOBILITY_WEIGHT, MOBILITY_START),
                    Piece::Bishop => (BISHOP_MOBILITY_WEIGHT, MOBILITY_START + 1),
                    Piece::Rook => (ROOK_MOBILITY_WEIGHT, MOBILITY_START + 2),
                    Piece::Queen => (QUEEN_MOBILITY_WEIGHT, MOBILITY_START + 3),
                    _ => unreachable!("only sliding and leaping pieces have attacks"),
                };
                let mobility = safe_mobility(raw_attacks, own, pawn_attacks[!color as usize]);
                let mobility_weight = tuned(mobility_weight, mobility_parameter);
                score += sign * mobility * pack(mobility_weight, mobility_weight);

                let attack_units = match piece {
                    Piece::Knight | Piece::Bishop => 2,
                    Piece::Rook => 3,
                    Piece::Queen => 5,
                    _ => unreachable!("only sliding and leaping pieces have attacks"),
                };
                let zone_hits = raw_attacks & king_zone[!color as usize];
                if !zone_hits.is_empty() {
                    king_attack_units[color as usize] += attack_units;
                    king_attackers[color as usize] += 1;
                    king_zone_attacks[color as usize] |= zone_hits;
                }
            }
        }
    }

    for color in [Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        let enemy = board.colors(!color);
        let enemy_majors =
            board.colored_pieces(!color, Piece::Rook) | board.colored_pieces(!color, Piece::Queen);
        let [pawn_targets, minor_major_targets, hanging_targets] = threat_counts(
            pawn_attacks[color as usize],
            minor_attacks[color as usize],
            all_attacks[color as usize],
            all_attacks[!color as usize],
            enemy,
            enemy_majors,
        );
        let undefended_zone =
            (king_zone_attacks[color as usize] & !all_attacks[!color as usize]).len() as i32;
        let shield = king_shield_pawns(board, !color);
        let units = king_danger_units(
            king_attack_units[color as usize],
            king_attackers[color as usize],
            undefended_zone,
            shield,
        );
        score += sign
            * pack(
                tuned(KING_ATTACK_CURVE[units], KING_ATTACK_CURVE_START + units)
                    + pawn_targets * tuned(PAWN_THREAT_BONUS_MG, PAWN_THREAT_MG)
                    + minor_major_targets
                        * tuned(MINOR_MAJOR_THREAT_BONUS_MG, MINOR_MAJOR_THREAT_MG)
                    + hanging_targets * tuned(HANGING_THREAT_BONUS_MG, HANGING_THREAT_MG),
                pawn_targets * tuned(PAWN_THREAT_BONUS_EG, PAWN_THREAT_EG)
                    + minor_major_targets
                        * tuned(MINOR_MAJOR_THREAT_BONUS_EG, MINOR_MAJOR_THREAT_EG)
                    + hanging_targets * tuned(HANGING_THREAT_BONUS_EG, HANGING_THREAT_EG),
            );
    }

    score += pawn_structure.score;

    for color in [Color::White, Color::Black] {
        let sign = if color == Color::White { 1 } else { -1 };
        let pawns = board.colored_pieces(color, Piece::Pawn);
        let enemy_pawns = board.colored_pieces(!color, Piece::Pawn);
        let mut file_counts = [0_i32; 8];
        for pawn in pawns {
            file_counts[pawn.file() as usize] += 1;
        }
        for pawn in pawn_structure.passers[color as usize] {
            let own_king_dist = chebyshev_distance(board.king(color), pawn);
            let enemy_king_dist = chebyshev_distance(board.king(!color), pawn);
            score += sign
                * pack(
                    0,
                    enemy_king_dist * tuned(PASSER_ENEMY_KING_DIST_WEIGHT_EG, PASSER_ENEMY_KING)
                        - own_king_dist * tuned(PASSER_OWN_KING_DIST_WEIGHT_EG, PASSER_OWN_KING),
                );
        }

        if board.colored_pieces(color, Piece::Bishop).len() >= 2 {
            score += sign
                * pack(
                    tuned(BISHOP_PAIR_BONUS_MG, BISHOP_PAIR_MG),
                    tuned(BISHOP_PAIR_BONUS_EG, BISHOP_PAIR_EG),
                );
        }
        for rook in board.colored_pieces(color, Piece::Rook) {
            let file = rook.file() as usize;
            if file_counts[file] == 0 {
                let enemy_on_file = enemy_pawns
                    .into_iter()
                    .any(|pawn| pawn.file() as usize == file);
                score += sign
                    * if enemy_on_file {
                        pack(
                            tuned(ROOK_SEMI_OPEN_FILE_BONUS_MG, ROOK_SEMI_OPEN_MG),
                            tuned(ROOK_SEMI_OPEN_FILE_BONUS_EG, ROOK_SEMI_OPEN_EG),
                        )
                    } else {
                        pack(
                            tuned(ROOK_OPEN_FILE_BONUS_MG, ROOK_OPEN_MG),
                            tuned(ROOK_OPEN_FILE_BONUS_EG, ROOK_OPEN_EG),
                        )
                    };
            }
        }
    }

    // Once one side has only a king, reward the winning side for taking away
    // space and walking its king in. Without this, most K+R/K+Q moves have
    // almost identical material scores until mate happens to enter the search
    // horizon. Bishop-and-knight is special: its king must be driven to a
    // corner controlled by the bishop, not merely to any edge.
    score += pack(
        0,
        bare_king_mating_bonus(board, Color::White) - bare_king_mating_bonus(board, Color::Black),
    );

    phase = phase.min(24);
    let mg = mg_value(score);
    let eg = eg_value(score);
    let white_score = (mg * phase + eg * (24 - phase)) / 24;
    let mut relative_score = if board.side_to_move() == Color::White {
        white_score + tuned(TEMPO_BONUS, TEMPO)
    } else {
        -white_score + tuned(TEMPO_BONUS, TEMPO)
    };
    match crate::kpk::probe(board) {
        Some(false) => return 0,
        Some(true) => {
            let pawn = board
                .pieces(Piece::Pawn)
                .into_iter()
                .next()
                .expect("KPK contains one pawn");
            let pawn_to_move = board.color_on(pawn) == Some(board.side_to_move());
            relative_score += if pawn_to_move {
                KPK_WIN_SCORE
            } else {
                -KPK_WIN_SCORE
            };
        }
        None => {}
    }
    relative_score * halfmove_scale(board) / 256
}

fn halfmove_scale(board: &Board) -> i32 {
    256 - 2 * i32::from(board.halfmove_clock().min(100))
}

fn pawn_attacks(board: &Board, color: Color) -> BitBoard {
    board
        .colored_pieces(color, Piece::Pawn)
        .into_iter()
        .fold(BitBoard::EMPTY, |attacks, pawn| {
            attacks | get_pawn_attacks(pawn, color)
        })
}

fn king_shield_pawns(board: &Board, color: Color) -> i32 {
    let king = board.king(color);
    let shield_rank = king.rank() as i32 + if color == Color::White { 1 } else { -1 };
    if !(0..8).contains(&shield_rank) {
        return 0;
    }
    (-1..=1)
        .filter_map(|file_delta| {
            let file = king.file() as i32 + file_delta;
            (0..8)
                .contains(&file)
                .then(|| Square::new(File::ALL[file as usize], Rank::ALL[shield_rank as usize]))
        })
        .filter(|&square| {
            board.piece_on(square) == Some(Piece::Pawn) && board.color_on(square) == Some(color)
        })
        .count() as i32
}

fn safe_mobility(raw_attacks: BitBoard, own: BitBoard, enemy_pawn_attacks: BitBoard) -> i32 {
    (raw_attacks & !own & !enemy_pawn_attacks).len() as i32
}

fn threat_counts(
    pawn_attacks: BitBoard,
    minor_attacks: BitBoard,
    all_attacks: BitBoard,
    enemy_defenses: BitBoard,
    enemy: BitBoard,
    enemy_majors: BitBoard,
) -> [i32; 3] {
    [
        (pawn_attacks & enemy).len() as i32,
        (minor_attacks & enemy_majors).len() as i32,
        (all_attacks & enemy & !enemy_defenses).len() as i32,
    ]
}

fn king_danger_units(base_units: i32, attackers: i32, undefended: i32, shield: i32) -> usize {
    (base_units + 2 * attackers.saturating_sub(1) + 2 * undefended - 2 * shield)
        .clamp(0, (KING_ATTACK_CURVE_LEN - 1) as i32) as usize
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
    let mut bonus = (7 - king_distance) * tuned(MOP_UP_KING_WEIGHT, MOP_KING);

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
        bonus += (7 - corner_distance) * tuned(BN_CORNER_WEIGHT, BN_CORNER);
    } else {
        let edge_distance = (defending_king.file() as i32)
            .min(7 - defending_king.file() as i32)
            .min(defending_king.rank() as i32)
            .min(7 - defending_king.rank() as i32);
        bonus += (3 - edge_distance) * tuned(MOP_UP_EDGE_WEIGHT, MOP_EDGE);

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
            bonus +=
                (7 - file_box.min(rank_box)) * tuned(ROOK_CONFINEMENT_WEIGHT, ROOK_CONFINEMENT);
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

#[cfg(test)]
mod evaluation_tests {
    use super::*;

    #[test]
    fn packed_scores_round_trip_and_add_without_cross_half_carry() {
        for (mg, eg) in [(-300, 700), (300, -700), (-1, -1), (0, 0)] {
            let score = pack(mg, eg);
            assert_eq!((mg_value(score), eg_value(score)), (mg, eg));
        }
        let sum = pack(-125, 250) + pack(75, -50);
        assert_eq!((mg_value(sum), eg_value(sum)), (-50, 200));
        assert_eq!(
            std::mem::size_of_val(&PACKED_PST),
            std::mem::size_of_val(&MG_PST) + std::mem::size_of_val(&EG_PST)
        );
    }

    #[test]
    fn halfmove_clock_damps_the_complete_evaluation() {
        let fresh = "7k/8/8/8/8/8/Q7/6K1 w - - 0 1".parse::<Board>().unwrap();
        let stale = "7k/8/8/8/8/8/Q7/6K1 w - - 80 41".parse::<Board>().unwrap();
        assert_eq!(
            evaluate(&stale),
            evaluate(&fresh) * halfmove_scale(&stale) / 256
        );
        assert!(evaluate(&stale).abs() < evaluate(&fresh).abs());
    }

    #[test]
    fn safe_mobility_excludes_own_and_enemy_pawn_controlled_squares() {
        let raw = Square::A1.bitboard() | Square::B1.bitboard() | Square::C1.bitboard();
        assert_eq!(
            safe_mobility(raw, Square::A1.bitboard(), Square::B1.bitboard()),
            1
        );
    }

    #[test]
    fn threat_categories_count_distinct_tactical_signals() {
        assert_eq!(
            threat_counts(
                Square::A1.bitboard(),
                Square::B1.bitboard(),
                Square::A1.bitboard() | Square::B1.bitboard() | Square::C1.bitboard(),
                Square::B1.bitboard(),
                Square::A1.bitboard() | Square::B1.bitboard() | Square::C1.bitboard(),
                Square::B1.bitboard(),
            ),
            [1, 1, 2]
        );
    }

    #[test]
    fn king_danger_compounds_attackers_and_is_reduced_by_the_pawn_shield() {
        let lone_attacker = king_danger_units(2, 1, 0, 0);
        let coordinated_attack = king_danger_units(9, 3, 2, 0);
        let shielded_attack = king_danger_units(9, 3, 2, 3);
        assert!(KING_ATTACK_CURVE[coordinated_attack] > 3 * KING_ATTACK_CURVE[lone_attacker]);
        assert!(KING_ATTACK_CURVE[shielded_attack] < KING_ATTACK_CURVE[coordinated_attack]);
    }
}

#[cfg(feature = "tuning")]
pub mod tuning {
    use cozy_chess::{BitBoard, Board, Color, Piece, Rank, Square};

    use super::*;

    pub const PARAMETER_COUNT: usize = super::PARAMETER_COUNT;

    /// Linear coefficients for one position, split so tapered interpolation
    /// remains identical to the production evaluator.
    pub struct LinearFeatures {
        mg: Box<[i32; PARAMETER_COUNT]>,
        eg: Box<[i32; PARAMETER_COUNT]>,
        direct: Box<[i32; PARAMETER_COUNT]>,
        phase: i32,
        perspective: i32,
        rule_scale: i32,
        offset: i32,
        forced_draw: bool,
    }

    impl LinearFeatures {
        pub fn value(&self, weights: &[f64; PARAMETER_COUNT]) -> f64 {
            if self.forced_draw {
                return 0.0;
            }
            let mg = dot_f64(&self.mg, weights);
            let eg = dot_f64(&self.eg, weights);
            let direct = dot_f64(&self.direct, weights);
            (self.perspective as f64 * (mg * self.phase as f64 + eg * (24 - self.phase) as f64)
                / 24.0
                + direct
                + f64::from(self.offset))
                * self.rule_scale as f64
                / 256.0
        }

        pub fn add_gradient(&self, gradient: &mut [f64; PARAMETER_COUNT], scale: f64) {
            if self.forced_draw {
                return;
            }
            let scale = scale * self.rule_scale as f64 / 256.0;
            let mg_scale = scale * self.perspective as f64 * self.phase as f64 / 24.0;
            let eg_scale = scale * self.perspective as f64 * (24 - self.phase) as f64 / 24.0;
            for (index, value) in gradient.iter_mut().enumerate() {
                *value += mg_scale * self.mg[index] as f64
                    + eg_scale * self.eg[index] as f64
                    + scale * self.direct[index] as f64;
            }
        }

        #[cfg(test)]
        fn integer_value(&self, weights: &[i32; PARAMETER_COUNT]) -> i32 {
            if self.forced_draw {
                return 0;
            }
            let mg = dot_i32(&self.mg, weights);
            let eg = dot_i32(&self.eg, weights);
            (self.perspective * (mg * self.phase + eg * (24 - self.phase)) / 24
                + dot_i32(&self.direct, weights)
                + self.offset)
                * self.rule_scale
                / 256
        }
    }

    fn dot_f64(coefficients: &[i32; PARAMETER_COUNT], weights: &[f64; PARAMETER_COUNT]) -> f64 {
        coefficients
            .iter()
            .zip(weights)
            .map(|(&coefficient, &weight)| f64::from(coefficient) * weight)
            .sum()
    }

    #[cfg(test)]
    fn dot_i32(coefficients: &[i32; PARAMETER_COUNT], weights: &[i32; PARAMETER_COUNT]) -> i32 {
        coefficients
            .iter()
            .zip(weights)
            .map(|(&coefficient, &weight)| coefficient * weight)
            .sum()
    }

    pub fn base_weights() -> [f64; PARAMETER_COUNT] {
        base_weights_i32().map(f64::from)
    }

    pub fn current_weights() -> [f64; PARAMETER_COUNT] {
        let mut weights = base_weights();
        for (weight, &delta) in weights.iter_mut().zip(DELTAS.iter()) {
            *weight += f64::from(delta);
        }
        weights
    }

    pub fn tuned_source_hash() -> u64 {
        crate::eval_tuned::SOURCE_HASH
    }

    fn base_weights_i32() -> [i32; PARAMETER_COUNT] {
        let mut weights = [0; PARAMETER_COUNT];
        weights[MG_VALUE_START..EG_VALUE_START].copy_from_slice(&MG_VALUE);
        weights[EG_VALUE_START..MG_PST_START].copy_from_slice(&EG_VALUE);
        for piece in 0..6 {
            for square in 0..64 {
                weights[MG_PST_START + piece * 64 + square] = i32::from(MG_PST[piece][square]);
                weights[EG_PST_START + piece * 64 + square] = i32::from(EG_PST[piece][square]);
            }
        }
        for (index, value) in [
            DOUBLED_PAWN_PENALTY_MG,
            DOUBLED_PAWN_PENALTY_EG,
            ISOLATED_PAWN_PENALTY_MG,
            ISOLATED_PAWN_PENALTY_EG,
            BISHOP_PAIR_BONUS_MG,
            BISHOP_PAIR_BONUS_EG,
            ROOK_OPEN_FILE_BONUS_MG,
            ROOK_OPEN_FILE_BONUS_EG,
            ROOK_SEMI_OPEN_FILE_BONUS_MG,
            ROOK_SEMI_OPEN_FILE_BONUS_EG,
            TEMPO_BONUS,
            MOP_UP_EDGE_WEIGHT,
            MOP_UP_KING_WEIGHT,
            ROOK_CONFINEMENT_WEIGHT,
            BN_CORNER_WEIGHT,
            KNIGHT_MOBILITY_WEIGHT,
            BISHOP_MOBILITY_WEIGHT,
            ROOK_MOBILITY_WEIGHT,
            QUEEN_MOBILITY_WEIGHT,
        ]
        .into_iter()
        .enumerate()
        {
            weights[DOUBLED_MG + index] = value;
        }
        weights[KING_ATTACK_CURVE_START..MG_PASSER_START].copy_from_slice(&KING_ATTACK_CURVE);
        weights[MG_PASSER_START..EG_PASSER_START].copy_from_slice(&MG_PASSER);
        weights[EG_PASSER_START..PASSER_OWN_KING].copy_from_slice(&EG_PASSER);
        weights[PASSER_OWN_KING] = PASSER_OWN_KING_DIST_WEIGHT_EG;
        weights[PASSER_ENEMY_KING] = PASSER_ENEMY_KING_DIST_WEIGHT_EG;
        weights[PAWN_THREAT_MG] = PAWN_THREAT_BONUS_MG;
        weights[PAWN_THREAT_EG] = PAWN_THREAT_BONUS_EG;
        weights[MINOR_MAJOR_THREAT_MG] = MINOR_MAJOR_THREAT_BONUS_MG;
        weights[MINOR_MAJOR_THREAT_EG] = MINOR_MAJOR_THREAT_BONUS_EG;
        weights[HANGING_THREAT_MG] = HANGING_THREAT_BONUS_MG;
        weights[HANGING_THREAT_EG] = HANGING_THREAT_BONUS_EG;
        weights
    }

    pub fn extract(board: &Board) -> LinearFeatures {
        let mut mg = Box::new([0; PARAMETER_COUNT]);
        let mut eg = Box::new([0; PARAMETER_COUNT]);
        let mut direct = Box::new([0; PARAMETER_COUNT]);
        let occupied = board.occupied();
        let king_zone = [
            get_king_moves(board.king(Color::White)) | board.king(Color::White).bitboard(),
            get_king_moves(board.king(Color::Black)) | board.king(Color::Black).bitboard(),
        ];
        let pawn_attacks = [
            super::pawn_attacks(board, Color::White),
            super::pawn_attacks(board, Color::Black),
        ];
        let mut all_attacks = pawn_attacks;
        let mut minor_attacks = [BitBoard::EMPTY; 2];
        let mut king_attack_units = [0_i32; 2];
        let mut king_attackers = [0_i32; 2];
        let mut king_zone_attacks = [BitBoard::EMPTY; 2];
        let mut phase = 0;

        for piece in Piece::ALL {
            let piece_index = piece as usize;
            for color in [Color::White, Color::Black] {
                let sign = color_sign(color);
                for square in board.colored_pieces(color, piece) {
                    let square_index = if color == Color::White {
                        (square as usize) ^ 56
                    } else {
                        square as usize
                    };
                    mg[MG_VALUE_START + piece_index] += sign;
                    eg[EG_VALUE_START + piece_index] += sign;
                    mg[MG_PST_START + piece_index * 64 + square_index] += sign;
                    eg[EG_PST_START + piece_index * 64 + square_index] += sign;
                    phase += PHASE[piece_index];

                    let raw_attacks = match piece {
                        Piece::Pawn => get_pawn_attacks(square, color),
                        Piece::Knight => get_knight_moves(square),
                        Piece::Bishop => get_bishop_moves(square, occupied),
                        Piece::Rook => get_rook_moves(square, occupied),
                        Piece::Queen => {
                            get_bishop_moves(square, occupied) | get_rook_moves(square, occupied)
                        }
                        Piece::King => get_king_moves(square),
                    };
                    all_attacks[color as usize] |= raw_attacks;
                    if matches!(piece, Piece::Knight | Piece::Bishop) {
                        minor_attacks[color as usize] |= raw_attacks;
                    }
                    if !matches!(piece, Piece::Pawn | Piece::King) {
                        let offset = match piece {
                            Piece::Knight => 0,
                            Piece::Bishop => 1,
                            Piece::Rook => 2,
                            Piece::Queen => 3,
                            _ => unreachable!(),
                        };
                        let mobility = super::safe_mobility(
                            raw_attacks,
                            board.colors(color),
                            pawn_attacks[!color as usize],
                        );
                        mg[MOBILITY_START + offset] += sign * mobility;
                        eg[MOBILITY_START + offset] += sign * mobility;
                        let attack_units = match piece {
                            Piece::Knight | Piece::Bishop => 2,
                            Piece::Rook => 3,
                            Piece::Queen => 5,
                            _ => unreachable!(),
                        };
                        let zone_hits = raw_attacks & king_zone[!color as usize];
                        if !zone_hits.is_empty() {
                            king_attack_units[color as usize] += attack_units;
                            king_attackers[color as usize] += 1;
                            king_zone_attacks[color as usize] |= zone_hits;
                        }
                    }
                }
            }
        }

        for color in [Color::White, Color::Black] {
            let sign = color_sign(color);
            let enemy = board.colors(!color);
            let enemy_majors = board.colored_pieces(!color, Piece::Rook)
                | board.colored_pieces(!color, Piece::Queen);
            let [pawn_targets, minor_major_targets, hanging_targets] = super::threat_counts(
                pawn_attacks[color as usize],
                minor_attacks[color as usize],
                all_attacks[color as usize],
                all_attacks[!color as usize],
                enemy,
                enemy_majors,
            );
            let undefended_zone =
                (king_zone_attacks[color as usize] & !all_attacks[!color as usize]).len() as i32;
            let units = super::king_danger_units(
                king_attack_units[color as usize],
                king_attackers[color as usize],
                undefended_zone,
                super::king_shield_pawns(board, !color),
            );
            mg[KING_ATTACK_CURVE_START + units] += sign;
            mg[PAWN_THREAT_MG] += sign * pawn_targets;
            eg[PAWN_THREAT_EG] += sign * pawn_targets;
            mg[MINOR_MAJOR_THREAT_MG] += sign * minor_major_targets;
            eg[MINOR_MAJOR_THREAT_EG] += sign * minor_major_targets;
            mg[HANGING_THREAT_MG] += sign * hanging_targets;
            eg[HANGING_THREAT_EG] += sign * hanging_targets;
        }

        for color in [Color::White, Color::Black] {
            add_color_features(board, color, &mut mg, &mut eg);
            add_bare_king_features(board, color, color_sign(color), &mut eg);
        }
        direct[TEMPO] = 1;

        let kpk = crate::kpk::probe(board);
        let offset = if kpk == Some(true) {
            let pawn = board
                .pieces(Piece::Pawn)
                .into_iter()
                .next()
                .expect("KPK contains one pawn");
            if board.color_on(pawn) == Some(board.side_to_move()) {
                KPK_WIN_SCORE
            } else {
                -KPK_WIN_SCORE
            }
        } else {
            0
        };
        LinearFeatures {
            mg,
            eg,
            direct,
            phase: phase.min(24),
            perspective: color_sign(board.side_to_move()),
            rule_scale: super::halfmove_scale(board),
            offset,
            forced_draw: kpk == Some(false),
        }
    }

    fn add_color_features(
        board: &Board,
        color: Color,
        mg: &mut [i32; PARAMETER_COUNT],
        eg: &mut [i32; PARAMETER_COUNT],
    ) {
        let sign = color_sign(color);
        let pawns = board.colored_pieces(color, Piece::Pawn);
        let enemy_pawns = board.colored_pieces(!color, Piece::Pawn);
        let mut file_counts = [0_i32; 8];
        for pawn in pawns {
            file_counts[pawn.file() as usize] += 1;
        }
        for count in file_counts {
            if count > 1 {
                mg[DOUBLED_MG] -= sign * (count - 1);
                eg[DOUBLED_EG] -= sign * (count - 1);
            }
        }
        for pawn in pawns {
            let file = pawn.file() as i32;
            let rank = pawn.rank() as i32;
            let isolated = (file == 0 || file_counts[(file - 1) as usize] == 0)
                && (file == 7 || file_counts[(file + 1) as usize] == 0);
            if isolated {
                mg[ISOLATED_MG] -= sign;
                eg[ISOLATED_EG] -= sign;
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
                mg[MG_PASSER_START + relative_rank] += sign;
                eg[EG_PASSER_START + relative_rank] += sign;
                eg[PASSER_ENEMY_KING] += sign * chebyshev_distance(board.king(!color), pawn);
                eg[PASSER_OWN_KING] -= sign * chebyshev_distance(board.king(color), pawn);
            }
        }
        if board.colored_pieces(color, Piece::Bishop).len() >= 2 {
            mg[BISHOP_PAIR_MG] += sign;
            eg[BISHOP_PAIR_EG] += sign;
        }
        for rook in board.colored_pieces(color, Piece::Rook) {
            let file = rook.file() as usize;
            if file_counts[file] == 0 {
                if enemy_pawns
                    .into_iter()
                    .any(|pawn| pawn.file() as usize == file)
                {
                    mg[ROOK_SEMI_OPEN_MG] += sign;
                    eg[ROOK_SEMI_OPEN_EG] += sign;
                } else {
                    mg[ROOK_OPEN_MG] += sign;
                    eg[ROOK_OPEN_EG] += sign;
                }
            }
        }
    }

    fn add_bare_king_features(
        board: &Board,
        attacker: Color,
        sign: i32,
        eg: &mut [i32; PARAMETER_COUNT],
    ) {
        let defender = !attacker;
        if board.colors(defender).len() != 1 {
            return;
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
            return;
        }
        let defending_king = board.king(defender);
        eg[MOP_KING] += sign * (7 - chebyshev_distance(board.king(attacker), defending_king));
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
            eg[BN_CORNER] += sign * (7 - corner_distance);
        } else {
            let edge_distance = (defending_king.file() as i32)
                .min(7 - defending_king.file() as i32)
                .min(defending_king.rank() as i32)
                .min(7 - defending_king.rank() as i32);
            eg[MOP_EDGE] += sign * (3 - edge_distance);
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
                eg[ROOK_CONFINEMENT] += sign * (7 - file_box.min(rank_box));
            }
        }
    }

    fn color_sign(color: Color) -> i32 {
        if color == Color::White { 1 } else { -1 }
    }

    /// Texel samples exclude checks and positions with an immediately legal
    /// capture or promotion; these are dominated by short tactical noise.
    pub fn is_quiet(board: &Board) -> bool {
        if !board.checkers().is_empty() {
            return false;
        }
        let enemy = board.colors(!board.side_to_move());
        let en_passant = board.en_passant().map_or(BitBoard::EMPTY, |file| {
            Square::new(file, Rank::Third.relative_to(!board.side_to_move())).bitboard()
        });
        let promotion_rank = Rank::Eighth.relative_to(board.side_to_move()).bitboard();
        let mut tactical = false;
        board.generate_moves(|moves| {
            tactical = moves
                .into_iter()
                .any(|mv| enemy.has(mv.to) || en_passant.has(mv.to) || promotion_rank.has(mv.to));
            tactical
        });
        !tactical
    }

    /// Average each PST square with its file mirror to restore the left-right
    /// symmetry the evaluator relies on. Per-square PST parameters are fit
    /// independently, so an unbalanced corpus yields a file-asymmetric table
    /// that breaks `evaluation_is_symmetric_by_color_and_turn`. Only the PST
    /// blocks are per-square; every other parameter is file-independent.
    pub fn symmetrize(weights: &mut [f64; PARAMETER_COUNT]) {
        for start in [MG_PST_START, EG_PST_START] {
            for piece in 0..6 {
                for square in 0..64 {
                    let file = square % 8;
                    let mirror = square - file + (7 - file);
                    if square < mirror {
                        let mean = (weights[start + piece * 64 + square]
                            + weights[start + piece * 64 + mirror])
                            / 2.0;
                        weights[start + piece * 64 + square] = mean;
                        weights[start + piece * 64 + mirror] = mean;
                    }
                }
            }
        }
    }

    pub fn generated_source(weights: &[f64; PARAMETER_COUNT], source_hash: u64) -> String {
        let base = base_weights();
        let deltas = std::array::from_fn::<i16, PARAMETER_COUNT, _>(|index| {
            (weights[index] - base[index])
                .round()
                .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
        });
        let mut source = format!(
            "//! Generated by `cargo run --features tuning --bin texel`.\n\n\
             #[cfg_attr(not(feature = \"tuning\"), allow(dead_code))]\n\
             pub(crate) const SOURCE_HASH: u64 = 0x{source_hash:016x};\n\
             pub(crate) const DELTAS: [i16; {PARAMETER_COUNT}] = [\n"
        );
        for chunk in deltas.chunks(16) {
            source.push_str("    ");
            for delta in chunk {
                source.push_str(&format!("{delta}, "));
            }
            source.push('\n');
        }
        source.push_str("];\n");
        source
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn extracted_model_exactly_matches_production_evaluation() {
            let mut weights = base_weights_i32();
            for (weight, &delta) in weights.iter_mut().zip(DELTAS.iter()) {
                *weight += i32::from(delta);
            }
            for fen in [
                "startpos",
                "r3k2r/ppp2ppp/2n1bn2/3qp3/3P4/2N1PN2/PPP1BPPP/R2QK2R w KQkq - 4 10",
                "8/2p2pk1/1p1p2p1/p2P3p/P1P1P3/1P3K2/5PP1/8 b - - 0 35",
                "7k/8/8/8/8/8/R7/6K1 w - - 0 1",
                "7k/8/8/8/8/8/4N3/2B3K1 w - - 0 1",
                "8/kPK5/8/8/8/8/8/8 w - - 0 1",
                "k7/P7/1K6/8/8/8/8/8 b - - 0 1",
            ] {
                let board = if fen == "startpos" {
                    Board::default()
                } else {
                    fen.parse().unwrap()
                };
                assert_eq!(
                    extract(&board).integer_value(&weights),
                    evaluate(&board),
                    "{fen}"
                );
            }
        }

        #[test]
        fn generated_source_has_all_parameters() {
            let source = generated_source(&current_weights(), 42);
            assert!(source.contains("SOURCE_HASH: u64 = 0x000000000000002a"));
            assert_eq!(source.matches("0, ").count(), PARAMETER_COUNT);
        }
    }
}
