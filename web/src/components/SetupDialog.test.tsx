// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { SetupDialog } from './SetupDialog'

afterEach(cleanup)

describe('SetupDialog', () => {
  it('submits edited input with an accessible dialog name', async () => {
    const user = userEvent.setup()
    const submit = vi.fn()
    render(<SetupDialog title="Load FEN position" initialValue="old" onClose={vi.fn()} onSubmit={submit} />)

    expect(screen.getByRole('dialog', { name: 'Load FEN position' })).toBeTruthy()
    const field = screen.getByRole('textbox', { name: 'Load FEN position' })
    await user.clear(field)
    await user.type(field, 'new position')
    await user.click(screen.getByRole('button', { name: 'Load' }))
    expect(submit).toHaveBeenCalledWith('new position')
  })

  it('presents exports as read-only and closes on Escape', async () => {
    const user = userEvent.setup()
    const close = vi.fn()
    render(<SetupDialog title="Export PGN" initialValue="1. e4" onClose={close} onSubmit={close} submitLabel="Close" readOnly />)

    expect(screen.getByRole('textbox', { name: 'Export PGN' })).toHaveProperty('readOnly', true)
    expect(screen.queryByRole('button', { name: 'Cancel' })).toBeNull()
    await user.keyboard('{Escape}')
    expect(close).toHaveBeenCalledOnce()
  })

  it('closes when the backdrop itself is pressed', async () => {
    const user = userEvent.setup()
    const close = vi.fn()
    const { container } = render(<SetupDialog title="Load PGN game" initialValue="" onClose={close} onSubmit={vi.fn()} />)

    const backdrop = container.querySelector('.modal-backdrop')
    expect(backdrop).not.toBeNull()
    await user.click(backdrop!)
    expect(close).toHaveBeenCalledOnce()
  })
})
