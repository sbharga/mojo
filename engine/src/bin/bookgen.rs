use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;

use cozy_chess::{Board, File, Move, Piece, Square};

const MAGIC: &[u8; 8] = b"MOJOBK01";
const HEADER_BYTES: usize = 48;
const RECORD_BYTES: usize = 12;

#[derive(Default)]
struct Position {
    fen: String,
    replies: BTreeMap<u16, Reply>,
}

struct Reply {
    notation: String,
    weight: u16,
}

fn main() {
    let arguments = env::args().collect::<Vec<_>>();
    if arguments.len() != 7 {
        eprintln!(
            "Usage: bookgen <lines.txt> <book.bin> <validation.tsv> <source-sha256> <max-plies> <max-records>"
        );
        std::process::exit(2);
    }
    let source_hash = decode_hash(&arguments[4]);
    let max_plies = arguments[5].parse::<usize>().expect("numeric max plies");
    let max_records = arguments[6].parse::<usize>().expect("numeric max records");
    let lines = fs::read_to_string(&arguments[1]).expect("read opening lines");
    let mut positions = HashMap::<u64, Position>::new();

    for (line_index, line) in lines.lines().enumerate() {
        let mut board = Board::default();
        for notation in line.split_whitespace().take(max_plies) {
            let mv = legal_uci_move(&board, notation).unwrap_or_else(|| {
                panic!("illegal move {notation} in opening line {}", line_index + 1)
            });
            let encoded = encode_move(mv);
            let position = positions.entry(board.hash()).or_default();
            if position.fen.is_empty() {
                position.fen = board.to_string();
            }
            let reply = position.replies.entry(encoded).or_insert_with(|| Reply {
                notation: notation.to_owned(),
                weight: 0,
            });
            reply.weight = reply.weight.saturating_add(1);
            board.play(mv);
        }
    }

    let mut ranked = positions.into_iter().collect::<Vec<_>>();
    ranked.sort_by(|(left_key, left), (right_key, right)| {
        total_weight(right)
            .cmp(&total_weight(left))
            .then_with(|| left_key.cmp(right_key))
    });
    let mut selected = Vec::<(u64, u16, Reply, String)>::with_capacity(max_records);
    for (key, position) in ranked {
        let mut replies = position.replies.into_iter().collect::<Vec<_>>();
        replies.sort_by(|(left_move, left), (right_move, right)| {
            right
                .weight
                .cmp(&left.weight)
                .then_with(|| left_move.cmp(right_move))
        });
        for (encoded, reply) in replies.into_iter().take(3) {
            if selected.len() == max_records {
                break;
            }
            selected.push((key, encoded, reply, position.fen.clone()));
        }
        if selected.len() == max_records {
            break;
        }
    }
    selected.sort_by_key(|(key, encoded, _, _)| (*key, *encoded));

    let mut binary = Vec::with_capacity(HEADER_BYTES + selected.len() * RECORD_BYTES);
    binary.extend_from_slice(MAGIC);
    binary.extend_from_slice(&source_hash);
    binary.extend_from_slice(&(max_plies as u32).to_le_bytes());
    binary.extend_from_slice(&(selected.len() as u32).to_le_bytes());
    let mut validation = String::from("fen\tmove\tweight\n");
    for (key, encoded, reply, fen) in &selected {
        binary.extend_from_slice(&key.to_le_bytes());
        binary.extend_from_slice(&encoded.to_le_bytes());
        binary.extend_from_slice(&reply.weight.to_le_bytes());
        validation.push_str(&format!("{fen}\t{}\t{}\n", reply.notation, reply.weight));
    }
    fs::write(&arguments[2], &binary).expect("write book");
    fs::write(&arguments[3], validation).expect("write validation records");
    println!(
        "generated {} replies in {} bytes from {} positions",
        selected.len(),
        binary.len(),
        selected
            .iter()
            .map(|record| record.0)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    );
}

fn total_weight(position: &Position) -> u32 {
    position
        .replies
        .values()
        .map(|reply| u32::from(reply.weight))
        .sum()
}

fn legal_uci_move(board: &Board, notation: &str) -> Option<Move> {
    let mut found = None;
    board.generate_moves(|moves| {
        for mv in moves {
            if uci_move(board, mv) == notation {
                found = Some(mv);
                return true;
            }
        }
        false
    });
    found
}

fn uci_move(board: &Board, mv: Move) -> String {
    let to = if board.piece_on(mv.from) == Some(Piece::King)
        && mv.from.file() == File::E
        && matches!(mv.to.file(), File::A | File::H)
    {
        Square::new(
            if mv.to.file() == File::A {
                File::C
            } else {
                File::G
            },
            mv.from.rank(),
        )
    } else {
        mv.to
    };
    let promotion = match mv.promotion {
        Some(Piece::Knight) => "n",
        Some(Piece::Bishop) => "b",
        Some(Piece::Rook) => "r",
        Some(Piece::Queen) => "q",
        _ => "",
    };
    format!("{}{to}{promotion}", mv.from)
}

fn encode_move(mv: Move) -> u16 {
    let promotion = match mv.promotion {
        None => 0,
        Some(Piece::Knight) => 1,
        Some(Piece::Bishop) => 2,
        Some(Piece::Rook) => 3,
        Some(Piece::Queen) => 4,
        Some(_) => panic!("invalid promotion piece"),
    };
    mv.from as u16 | (mv.to as u16) << 6 | promotion << 12
}

fn decode_hash(value: &str) -> [u8; 32] {
    assert_eq!(value.len(), 64, "source hash must contain 64 hex digits");
    let mut result = [0; 32];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .expect("source hash contains invalid hex");
    }
    result
}
