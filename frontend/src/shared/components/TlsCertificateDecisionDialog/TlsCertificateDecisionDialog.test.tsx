import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'
import { TlsCertificateDecisionDialog } from './TlsCertificateDecisionDialog'

it('offers one-time, persistent and deny TLS decisions with certificate context', async () => {
  const user = userEvent.setup()
  const onDecision = vi.fn()
  render(
    <TlsCertificateDecisionDialog
      open
      host="dav.example.test"
      port={443}
      fingerprint="aabb"
      onDecision={onDecision}
    />,
  )

  expect(screen.getByRole('dialog')).toHaveTextContent('dav.example.test:443')
  expect(screen.getByText('AA:BB')).toBeInTheDocument()

  await user.click(screen.getByRole('button', { name: 'Accept once' }))
  await user.click(screen.getByRole('button', { name: 'Always accept for this account' }))
  await user.click(screen.getByRole('button', { name: 'Deny' }))

  expect(onDecision.mock.calls).toEqual([
    ['accept'],
    ['accept_always'],
    ['deny'],
  ])
})
