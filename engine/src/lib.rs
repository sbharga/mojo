//! Mojo's compact, deterministic chess search core.
//!
//! The public Wasm entry point accepts and returns plain data so the worker
//! protocol remains independent of Rust implementation details.

mod eval;
mod eval_tuned;
mod kpk;
mod search;

#[cfg(feature = "tuning")]
pub use eval::tuning;

use cozy_chess::{Board, File, Move, Piece, Square};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use search::{MATE_SCORE, MAX_PLY, SearchCore, SearchLine, fallback};

#[derive(Debug, Clone, Serialize)]
struct PrincipalVariation {
    score_cp: Option<i32>,
    mate_in: Option<i32>,
    moves: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AnalysisResult {
    depth: u8,
    nodes: u64,
    root_node_fraction: f64,
    soft_time_fraction: f64,
    predicted_next_ms: f64,
    ebf_gate_override: bool,
    clock_check_interval: u32,
    elapsed_ms: f64,
    timed_out: bool,
    lines: Vec<PrincipalVariation>,
}

/// A reusable engine instance. Search heuristics and its fixed-size
/// transposition table survive iterative depths and adjacent positions.
#[wasm_bindgen]
pub struct Engine {
    search: SearchCore,
    board: Option<Board>,
}

#[wasm_bindgen]
impl Engine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            search: SearchCore::new(),
            board: None,
        }
    }

    /// Sets the root and its preceding game positions.
    ///
    /// # Errors
    /// Returns an error when the root or any prior FEN is invalid.
    pub fn set_position(&mut self, fen: &str, prior_fens: JsValue) -> Result<(), JsValue> {
        let board = parse_board(fen)?;
        let prior_strings: Vec<String> = serde_wasm_bindgen::from_value(prior_fens)
            .map_err(|error| JsValue::from_str(&format!("Invalid position history: {error}")))?;
        let prior = prior_strings
            .iter()
            .map(|prior_fen| parse_board(prior_fen))
            .collect::<Result<Vec<_>, _>>()?;
        self.search.set_position(&board, &prior);
        self.board = Some(board);
        Ok(())
    }

    /// Searches one iterative-deepening step while retaining earlier search state.
    ///
    /// # Errors
    /// Returns an error if no position has been set or serialization fails.
    pub fn analyze_depth(
        &mut self,
        depth: u8,
        multi_pv: u8,
        time_limit_ms: f64,
    ) -> Result<JsValue, JsValue> {
        let board = self
            .board
            .as_ref()
            .ok_or_else(|| JsValue::from_str("No position has been set"))?;
        run_analysis(&mut self.search, board, depth, multi_pv, time_limit_ms)
    }

    /// Installs an optional shared cancellation watermark supplied by the worker.
    #[cfg(target_arch = "wasm32")]
    pub fn set_stop_flag(&mut self, stop_flag: js_sys::Int32Array) {
        self.search.set_stop_flag(stop_flag);
    }

    /// Identifies the request currently being searched for watermark comparison.
    #[cfg(target_arch = "wasm32")]
    pub fn set_stop_request(&mut self, request_id: i32) {
        self.search.set_stop_request(request_id);
    }

    /// Returns the best static one-ply fallback for the current position.
    pub fn fallback_move(&self) -> Option<String> {
        let board = self.board.as_ref()?;
        fallback(board).map(|mv| uci_move(board, mv))
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

/// Compatibility entry point for consumers that do not retain an `Engine`.
#[wasm_bindgen]
pub fn analyze_step(
    fen: &str,
    depth: u8,
    multi_pv: u8,
    time_limit_ms: f64,
) -> Result<JsValue, JsValue> {
    let board = parse_board(fen)?;
    let mut search = SearchCore::new();
    search.set_position(&board, &[]);
    run_analysis(&mut search, &board, depth, multi_pv, time_limit_ms)
}

fn run_analysis(
    search: &mut SearchCore,
    board: &Board,
    depth: u8,
    multi_pv: u8,
    time_limit_ms: f64,
) -> Result<JsValue, JsValue> {
    let start = now_ms();
    let result = search.analyze_depth(board, i16::from(depth.max(1)), multi_pv, time_limit_ms);
    serialize_result(AnalysisResult {
        depth,
        nodes: result.nodes,
        root_node_fraction: result.root_node_fraction,
        soft_time_fraction: result.soft_time_fraction,
        predicted_next_ms: result.predicted_next_ms,
        ebf_gate_override: result.ebf_gate_override,
        clock_check_interval: result.clock_check_interval,
        elapsed_ms: now_ms() - start,
        timed_out: result.timed_out,
        lines: result
            .lines
            .into_iter()
            .map(|line| score_to_line(board, line))
            .collect(),
    })
}

/// Picks the best immediate legal move when a bounded search cannot finish.
///
/// # Errors
/// Returns an error if `fen` is invalid.
#[wasm_bindgen]
pub fn fallback_move(fen: &str) -> Result<Option<String>, JsValue> {
    let board = parse_board(fen)?;
    Ok(fallback(&board).map(|mv| uci_move(&board, mv)))
}

#[wasm_bindgen]
pub fn engine_name() -> String {
    "Mojo 0.2".to_owned()
}

fn parse_board(fen: &str) -> Result<Board, JsValue> {
    fen.parse::<Board>()
        .map_err(|error| JsValue::from_str(&format!("Invalid FEN: {error}")))
}

fn serialize_result(result: AnalysisResult) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(&result).map_err(|error| JsValue::from_str(&error.to_string()))
}

fn score_to_line(board: &Board, line: SearchLine) -> PrincipalVariation {
    let mate_in = if line.score.abs() >= MATE_SCORE - MAX_PLY as i32 {
        Some(if line.score > 0 {
            (MATE_SCORE - line.score + 1) / 2
        } else {
            -((MATE_SCORE + line.score + 1) / 2)
        })
    } else {
        None
    };
    PrincipalVariation {
        score_cp: mate_in.is_none().then_some(line.score),
        mate_in,
        moves: {
            let mut position = board.clone();
            line.moves
                .into_iter()
                .map(|mv| {
                    let uci = uci_move(&position, mv);
                    position.play(mv);
                    uci
                })
                .collect()
        },
    }
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
    format!("{}{}{}", mv.from, to, promotion)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn now_ms() -> f64 {
    wasm_now_ms()
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn wasm_now_ms() -> f64;
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn now_ms() -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{evaluate, insufficient_material};
    use crate::search::{legal_moves, played};
    use cozy_chess::Move;

    fn perft(board: &Board, depth: u8) -> u64 {
        if depth == 0 {
            1
        } else {
            legal_moves(board)
                .into_iter()
                .map(|mv| perft(&played(board, mv), depth - 1))
                .sum()
        }
    }

    fn search(fen: &str, depth: u8) -> Vec<SearchLine> {
        let board = fen.parse::<Board>().unwrap();
        let mut search = SearchCore::new();
        search.set_position(&board, &[]);
        search
            .analyze_depth(&board, i16::from(depth), 1, 10_000.0)
            .lines
    }

    #[test]
    fn standard_perft_positions() {
        let start = Board::default();
        assert_eq!(perft(&start, 4), 197_281);

        let kiwipete = "r3k2r/p1ppqpb1/bn2pnp1/2pP4/1p2P3/2N2N2/PPQBBPPP/R3K2R w KQkq - 0 1"
            .parse::<Board>()
            .unwrap();
        assert_eq!(perft(&kiwipete, 3), 85_877);

        let endgame = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"
            .parse::<Board>()
            .unwrap();
        assert_eq!(perft(&endgame, 3), 2_812);
    }

    #[test]
    fn evaluation_is_symmetric_by_color_and_turn() {
        let white = "8/8/8/3N4/8/8/8/K6k w - - 0 1".parse::<Board>().unwrap();
        let black = "k6K/8/8/8/4n3/8/8/8 b - - 0 1".parse::<Board>().unwrap();
        assert_eq!(evaluate(&white), evaluate(&black));
    }

    #[test]
    fn rook_mate_eval_rewards_cutting_off_the_bare_king() {
        let loose = "8/8/8/8/4k3/8/8/R5K1 w - - 0 1".parse::<Board>().unwrap();
        let confined = "8/8/8/8/4k3/8/R7/6K1 w - - 0 1".parse::<Board>().unwrap();
        assert!(evaluate(&confined) > evaluate(&loose));
    }

    #[test]
    fn bishop_knight_eval_prefers_the_bishops_corner() {
        // The c1 bishop controls a1/h8, so h8 should be preferable to h1.
        let right_corner = "7k/8/8/8/8/8/4N3/2B3K1 w - - 0 1".parse::<Board>().unwrap();
        let wrong_corner = "8/8/8/8/8/8/4N2k/2B3K1 w - - 0 1".parse::<Board>().unwrap();
        assert!(evaluate(&right_corner) > evaluate(&wrong_corner));
    }

    #[test]
    fn recognizes_basic_dead_positions() {
        for fen in [
            "8/8/8/8/8/8/8/K6k w - - 0 1",
            "7k/8/8/8/8/8/1N6/K7 w - - 0 1",
            "7k/8/8/8/8/8/1b6/K1B5 w - - 0 1",
            "k7/8/8/8/8/2B5/3B4/7K w - - 0 1",
        ] {
            assert!(insufficient_material(&fen.parse::<Board>().unwrap()));
        }
        for fen in [
            // Two knights cannot force mate, but mating positions exist.
            "7k/5K2/5NN1/8/8/8/8/8 b - - 0 1",
            // Opposite-colored bishops can obstruct one another's king.
            "7k/8/8/8/8/8/2b5/K1B5 w - - 0 1",
            "8/8/7k/8/8/8/1BN5/K7 w - - 0 1",
            "k7/8/8/8/8/2BB4/8/7K w - - 0 1",
        ] {
            assert!(!insufficient_material(&fen.parse::<Board>().unwrap()));
        }
    }

    #[test]
    fn finds_a_forced_mate_without_stopping_in_check() {
        let fen = "7k/5Q2/6K1/8/8/8/8/8 w - - 0 1";
        let lines = search(fen, 2);
        assert_eq!(
            score_to_line(
                &fen.parse::<Board>().unwrap(),
                lines.into_iter().next().unwrap()
            )
            .mate_in,
            Some(1)
        );
    }

    #[test]
    fn fifty_move_position_scores_as_draw() {
        let lines = search("8/8/8/8/8/8/5Q2/K6k w - - 100 80", 2);
        assert!(lines.is_empty());
    }

    #[test]
    fn timeout_does_not_claim_a_completed_iteration() {
        let board = Board::default();
        let mut search = SearchCore::new();
        search.set_position(&board, &[]);
        search.set_node_limit(Some(1));
        let result = search.analyze_depth(&board, 8, 3, 10_000.0);
        assert!(result.timed_out);
    }

    #[test]
    fn recognizes_threefold_repetition_from_game_history() {
        let board = Board::default();
        let mut search = SearchCore::new();
        search.set_position(&board, &[board.clone(), board.clone()]);
        let result = search.analyze_depth(&board, 4, 1, 10_000.0);
        assert!(result.lines.is_empty());
    }

    #[test]
    fn multipv_root_moves_are_distinct() {
        let board = Board::default();
        let mut search = SearchCore::new();
        search.set_position(&board, &[]);
        let result = search.analyze_depth(&board, 3, 3, 10_000.0);
        assert_eq!(result.lines.len(), 3);
        let roots: Vec<_> = result.lines.iter().map(|line| line.moves[0]).collect();
        assert_ne!(roots[0], roots[1]);
        assert_ne!(roots[0], roots[2]);
        assert_ne!(roots[1], roots[2]);
    }

    #[test]
    fn principal_variation_is_legal() {
        let board = Board::default();
        let lines = search(&board.to_string(), 4);
        let mut position = board;
        for mv in &lines[0].moves {
            assert!(position.is_legal(*mv));
            position.play(*mv);
        }
    }

    #[test]
    fn move_strings_remain_uci_compatible() {
        let mv = "e2e4".parse::<Move>().unwrap();
        assert_eq!(mv.to_string(), "e2e4");
    }

    #[test]
    fn castling_uses_standard_uci_king_destinations() {
        let board = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1"
            .parse::<Board>()
            .unwrap();
        assert_eq!(uci_move(&board, "e1h1".parse().unwrap()), "e1g1");
        assert_eq!(uci_move(&board, "e1a1".parse().unwrap()), "e1c1");
    }
}
