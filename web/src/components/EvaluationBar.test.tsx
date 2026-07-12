// @vitest-environment jsdom

import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { EvaluationBar } from './EvaluationBar'

afterEach(cleanup)

describe('EvaluationBar', () => {
  it('announces which side has a forced mate', () => {
    const { rerender } = render(<EvaluationBar scoreCp={null} mateIn={3} />)
    expect(screen.getByRole('img', { name: 'White mates in 3' })).toBeTruthy()
    expect(screen.getByRole('img', { name: 'White mates in 3' }).firstElementChild).toHaveProperty('style.height', '0%')
    rerender(<EvaluationBar scoreCp={null} mateIn={-2} />)
    expect(screen.getByRole('img', { name: 'Black mates in 2' })).toBeTruthy()
    expect(screen.getByRole('img', { name: 'Black mates in 2' }).firstElementChild).toHaveProperty('style.height', '100%')
  })

  it('announces centipawn evaluations and unavailable analysis', () => {
    const { rerender } = render(<EvaluationBar scoreCp={125} />)
    expect(screen.getByRole('img', { name: 'White evaluation +1.3' })).toBeTruthy()
    rerender(<EvaluationBar scoreCp={null} />)
    expect(screen.getByRole('img', { name: 'Evaluation unavailable' })).toBeTruthy()
  })

  it('shows the winner after checkmate and no label after a draw', () => {
    const { rerender } = render(<EvaluationBar scoreCp={null} result="black" />)
    expect(screen.getByRole('img', { name: 'Black won' }).textContent).toBe('Black')
    expect(screen.getByRole('img', { name: 'Black won' }).firstElementChild).toHaveProperty('style.height', '100%')

    rerender(<EvaluationBar scoreCp={null} result="draw" />)
    expect(screen.getByRole('img', { name: 'Game drawn' }).textContent).toBe('')
  })
})
