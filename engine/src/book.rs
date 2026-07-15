use cozy_chess::{Board, Move, Piece, Square};

const BOOK: &[u8] = include_bytes!("../book.bin");
const HEADER_BYTES: usize = 48;
const RECORD_BYTES: usize = 12;

fn record_count() -> usize {
    u32::from_le_bytes(BOOK[44..48].try_into().expect("book count header")) as usize
}

fn record(index: usize) -> (u64, Move, u16) {
    let offset = HEADER_BYTES + index * RECORD_BYTES;
    let key = u64::from_le_bytes(BOOK[offset..offset + 8].try_into().expect("book key"));
    let encoded = u16::from_le_bytes(BOOK[offset + 8..offset + 10].try_into().expect("book move"));
    let weight = u16::from_le_bytes(
        BOOK[offset + 10..offset + 12]
            .try_into()
            .expect("book weight"),
    );
    (key, decode_move(encoded), weight)
}

fn decode_move(encoded: u16) -> Move {
    let promotion = match encoded >> 12 {
        1 => Some(Piece::Knight),
        2 => Some(Piece::Bishop),
        3 => Some(Piece::Rook),
        4 => Some(Piece::Queen),
        _ => None,
    };
    Move {
        from: Square::index(usize::from(encoded & 0x3f)),
        to: Square::index(usize::from((encoded >> 6) & 0x3f)),
        promotion,
    }
}

pub(crate) fn book_move(board: &Board, seed: u32) -> Option<Move> {
    let key = board.hash();
    let count = record_count();
    let mut left = 0;
    let mut right = count;
    while left < right {
        let middle = left + (right - left) / 2;
        if record(middle).0 < key {
            left = middle + 1;
        } else {
            right = middle;
        }
    }
    if left == count || record(left).0 != key {
        return None;
    }

    let first = left;
    let mut total = 0_u32;
    while left < count {
        let (candidate_key, _, weight) = record(left);
        if candidate_key != key {
            break;
        }
        total += u32::from(weight);
        left += 1;
    }
    let mixed = seed ^ key as u32 ^ (key >> 32) as u32;
    let mut choice = mixed.wrapping_mul(0x9e37_79b9).rotate_left(13) % total;
    for index in first..left {
        let (_, mv, weight) = record(index);
        if choice < u32::from(weight) {
            return board.is_legal(mv).then_some(mv);
        }
        choice -= u32::from(weight);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_book_is_bounded_and_every_reply_is_legal() {
        assert_eq!(&BOOK[..8], b"MOJOBK01");
        assert!((8 * 1024..=32 * 1024).contains(&BOOK.len()));
        assert_eq!(BOOK.len(), HEADER_BYTES + record_count() * RECORD_BYTES);

        let validation = include_str!("../book-validation.tsv");
        let mut validated = 0;
        for line in validation.lines().skip(1) {
            let mut fields = line.split('\t');
            let fen = fields.next().expect("validation FEN");
            let notation = fields.next().expect("validation move");
            let expected_weight = fields
                .next()
                .expect("validation weight")
                .parse::<u16>()
                .expect("numeric validation weight");
            let board = fen.parse::<Board>().expect("valid validation FEN");
            let expected = crate::search::legal_moves(&board)
                .into_iter()
                .find(|&mv| crate::uci_move(&board, mv) == notation)
                .expect("book reply remains legal");
            let matching = (0..record_count()).any(|index| {
                let (key, mv, weight) = record(index);
                key == board.hash() && mv == expected && weight == expected_weight
            });
            assert!(matching, "missing book record for {fen} {notation}");
            validated += 1;
        }
        assert_eq!(validated, record_count());

        let start = Board::default();
        let replies = (0..128)
            .filter_map(|seed| book_move(&start, seed))
            .collect::<std::collections::HashSet<_>>();
        assert!(
            replies.len() >= 2,
            "weighted seeds should vary opening play"
        );
    }
}
