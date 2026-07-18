// @vitest-environment jsdom

import { act, cleanup, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import Root from './Root'

const comparison = vi.hoisted(() => ({
  onRunningChange: null as ((running: boolean) => void) | null,
}))

vi.mock('./App', () => ({ default: () => <div>Play page</div> }))
vi.mock('./comparison/ComparePage', () => ({
  ComparePage: ({ onRunningChange }: { onRunningChange: (running: boolean) => void }) => {
    comparison.onRunningChange = onRunningChange
    return <div>Compare page</div>
  },
}))

beforeEach(() => {
  comparison.onRunningChange = null
  window.history.replaceState(null, '', '#/compare')
})

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

describe('Root comparison routing', () => {
  it('blocks hash navigation while a match is running when the user declines', () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false)
    render(<Root />)
    comparison.onRunningChange?.(true)

    act(() => {
      window.location.hash = '#/'
      window.dispatchEvent(new HashChangeEvent('hashchange'))
    })

    expect(screen.getByText('Compare page')).toBeTruthy()
    expect(window.location.hash).toBe('#/compare')
  })

  it('leaves the comparison after confirmation', () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true)
    render(<Root />)
    comparison.onRunningChange?.(true)

    act(() => {
      window.location.hash = '#/'
      window.dispatchEvent(new HashChangeEvent('hashchange'))
    })

    expect(screen.getByText('Play page')).toBeTruthy()
  })
})
