import { Chess } from 'chess.js'
import { describe, expect, it } from 'vitest'
import { computeMaterial } from './material'

describe('computeMaterial', () => {
  it('reports no captures at the start of a game', () => {
    const game = new Chess()
    game.move('e4')
    game.move('e5')
    expect(computeMaterial(game.history({ verbose: true }), 2)).toEqual({
      capturedByWhite: [],
      capturedByBlack: [],
      advantage: 0,
    })
  })

  it('tallies captured pieces and advantage for the side ahead', () => {
    const game = new Chess()
    ;['e4', 'd5', 'exd5', 'Qxd5'].forEach((san) => game.move(san))
    const history = game.history({ verbose: true })
    expect(computeMaterial(history, history.length)).toEqual({
      capturedByWhite: ['p'],
      capturedByBlack: ['p'],
      advantage: 0,
    })
  })

  it('stops counting captures beyond the requested ply', () => {
    const game = new Chess()
    ;['e4', 'd5', 'exd5'].forEach((san) => game.move(san))
    const history = game.history({ verbose: true })
    expect(computeMaterial(history, 2)).toEqual({ capturedByWhite: [], capturedByBlack: [], advantage: 0 })
    expect(computeMaterial(history, 3)).toEqual({ capturedByWhite: ['p'], capturedByBlack: [], advantage: 1 })
  })
})
