import { Chess } from 'chess.js'
import { describe, expect, it } from 'vitest'
import { groupMoveHistory } from './moveHistory'

describe('groupMoveHistory', () => {
  it('groups a normal game into full moves', () => {
    const game = new Chess()
    game.move('e4')
    game.move('e5')
    expect(groupMoveHistory(game.history({ verbose: true })).map((row) => ({
      number: row.number,
      white: row.white?.san,
      black: row.black?.san,
      whitePly: row.whitePly,
      blackPly: row.blackPly,
    }))).toEqual([{ number: 1, white: 'e4', black: 'e5', whitePly: 1, blackPly: 2 }])
  })

  it('keeps a black first move in the black column with its FEN move number', () => {
    const game = new Chess('8/8/8/8/8/4k3/8/4K3 b - - 0 12')
    game.move('Kf3')
    const [row] = groupMoveHistory(game.history({ verbose: true }))
    expect({ number: row.number, white: row.white, black: row.black?.san, blackPly: row.blackPly })
      .toEqual({ number: 12, white: undefined, black: 'Kf3', blackPly: 1 })
  })
})
