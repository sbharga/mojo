// @vitest-environment jsdom

import { Chess } from 'chess.js'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MoveHistory } from './MoveHistory'

afterEach(cleanup)

describe('MoveHistory', () => {
  it('navigates to a selected ply in a black-to-move imported game', async () => {
    const user = userEvent.setup()
    const game = new Chess('8/8/8/8/8/4k3/8/4K3 b - - 0 12')
    game.move('Kf3')
    const navigate = vi.fn()
    render(<MoveHistory history={game.history({ verbose: true })} currentPly={0} onNavigate={navigate} />)

    expect(screen.getByText('12.')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Go to 12... Kf3' }))
    expect(navigate).toHaveBeenCalledWith(1)
  })

  it('provides start, previous, next, and latest controls', async () => {
    const user = userEvent.setup()
    const game = new Chess()
    game.move('e4')
    game.move('e5')
    const navigate = vi.fn()
    render(<MoveHistory history={game.history({ verbose: true })} currentPly={1} onNavigate={navigate} />)

    await user.click(screen.getByRole('button', { name: 'Go to start' }))
    await user.click(screen.getByRole('button', { name: 'Previous move' }))
    await user.click(screen.getByRole('button', { name: 'Next move' }))
    await user.click(screen.getByRole('button', { name: 'Go to latest' }))
    expect(navigate.mock.calls).toEqual([[0], [0], [2], [2]])
  })
})
