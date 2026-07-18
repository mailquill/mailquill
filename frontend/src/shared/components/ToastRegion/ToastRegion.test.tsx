import { render } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { ToastRegion } from './ToastRegion'

const toaster = vi.hoisted(() => vi.fn())

vi.mock('sonner', () => ({
  Toaster: (props: Record<string, unknown>) => {
    toaster(props)
    return null
  },
}))

describe('ToastRegion', () => {
  it('configures an accessible notification region in the top-left corner', () => {
    render(<ToastRegion />)

    expect(toaster).toHaveBeenCalledWith(expect.objectContaining({
      position: 'top-left',
      containerAriaLabel: 'Notifications',
      toastOptions: expect.objectContaining({ closeButtonAriaLabel: 'Dismiss notification' }),
    }))
  })
})
