import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { ComposeDialog } from './ComposeDialog'

vi.mock('@/shared/hooks/useAccounts', () => ({
  useAccounts: () => ({ data: [] }),
}))

vi.mock('@/shared/hooks/useMessages', () => ({
  useSendMessage: () => ({ mutate: vi.fn(), isPending: false, error: null }),
}))

vi.mock('@/shared/hooks/useOnlineStatus', () => ({
  useOnlineStatus: () => true,
}))

vi.mock('@/shared/hooks/usePgp', () => ({
  usePgpKeys: () => ({ data: [] }),
}))

vi.mock('@/shared/hooks/useContacts', () => ({
  useContactSearch: () => ({ data: [] }),
}))

describe('ComposeDialog account bootstrap', () => {
  it('opens safely before mailbox identities have loaded', () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(
      <QueryClientProvider client={queryClient}>
        <ComposeDialog open initialState={{ mode: 'new' }} onClose={vi.fn()} />
      </QueryClientProvider>,
    )

    expect(screen.getByRole('dialog')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled()
  })
})
