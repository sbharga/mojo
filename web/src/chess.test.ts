import { describe, expect, it } from "vitest";
import { Chess } from "chess.js";

describe("browser game state", () => {
  it("exports and reloads a legal PGN main line", () => {
    const game = new Chess();
    game.move("e4");
    game.move("e5");
    game.move("Nf3");
    const restored = new Chess();
    restored.loadPgn(game.pgn());
    expect(restored.fen()).toBe(game.fen());
    expect(restored.history()).toEqual(["e4", "e5", "Nf3"]);
  });

  it("accepts en passant from a FEN position", () => {
    const game = new Chess(
      "rnbqkbnr/ppp1pppp/8/3pP3/8/8/PPP2PPP/RNBQKBNR w KQkq d6 0 3",
    );
    expect(game.move({ from: "e5", to: "d6" }).san).toBe("exd6");
  });
});
