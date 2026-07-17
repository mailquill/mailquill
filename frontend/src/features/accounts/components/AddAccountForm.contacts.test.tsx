import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from '@/shared/api'
import type { Account } from '@/shared/types'
import { AddAccountForm } from './AddAccountForm'

const discovered = {
  source: 'provider' as const,
  provider: 'Example Mail',
  oauth: false,
  imapHost: 'imap.example.test',
  imapPort: 993,
  imapSecurity: 'ssl' as const,
  imapUser: 'person@example.test',
  smtpHost: 'smtp.example.test',
  smtpPort: 587,
  smtpSecurity: 'starttls' as const,
  smtpUser: 'person@example.test',
  carddavUrl: 'https://dav.example.test/addressbooks/',
  caldavUrl: 'https://dav.example.test/calendars/',
}

const createdAccount = {
  id: 'mailbox-1',
  display_name: 'person@example.test',
  primary_email: 'person@example.test',
  contacts: { source_id: 'source-1', provider: 'cardav', state: 'pending', reason: null, enabled: true, last_synced_at: null, cache_retained: true },
} as Account

let createError: unknown = null
let completeCreate = true
const createMutate = vi.fn((data: unknown, options?: { onSuccess?: (account: Account) => void }) => {
  if (completeCreate) options?.onSuccess?.(createdAccount)
  return data
})

vi.mock('@/shared/hooks/useAccounts', () => ({
  useCreateAccount: () => ({ mutate: createMutate, isPending: false, error: createError }),
}))

vi.mock('@/shared/lib/serverDiscovery', async (importOriginal) => {
  const original = await importOriginal<typeof import('@/shared/lib/serverDiscovery')>()
  return {
    ...original,
    discoverServersAsync: vi.fn(async () => discovered),
    discoverServersForProvider: vi.fn(() => discovered),
  }
})

async function reachCapabilityReview(user: ReturnType<typeof userEvent.setup>, container: HTMLElement) {
  await user.type(screen.getByPlaceholderText('name@example.com'), 'person@example.test')
  await user.click(screen.getByRole('button', { name: /Continue/ }))
  await screen.findByText(/Example Mail/)
  const password = container.querySelector<HTMLInputElement>('input[type="password"]')
  expect(password).not.toBeNull()
  await user.type(password!, 'secret')
  await user.click(screen.getByRole('button', { name: /Continue/ }))
  await screen.findByText('Sync contacts')
}

describe('first-time mailbox contact setup', () => {
  beforeEach(() => {
    createError = null
    completeCreate = true
    createMutate.mockClear()
  })

  it('opts in by default and reports non-blocking initial sync', async () => {
    const user = userEvent.setup()
    const { container } = render(<AddAccountForm onCancel={vi.fn()} onCreated={vi.fn()} />)
    await reachCapabilityReview(user, container)

    expect(screen.getByRole('checkbox', { name: /Sync contacts/ })).toBeChecked()
    await user.click(screen.getByRole('button', { name: 'Add account' }))
    expect(createMutate.mock.calls.at(-1)?.[0]).toEqual(expect.objectContaining({ contacts_enabled: true }))
    expect(await screen.findByText(/first contact sync is starting/i)).toBeInTheDocument()
  })

  it('allows contact sync to be skipped without blocking mailbox creation', async () => {
    const user = userEvent.setup()
    const { container } = render(<AddAccountForm onCancel={vi.fn()} onCreated={vi.fn()} />)
    await reachCapabilityReview(user, container)

    await user.click(screen.getByRole('checkbox', { name: /Sync contacts/ }))
    expect(screen.getByText(/mailbox will be created without contacts/i)).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Add account' }))
    expect(createMutate.mock.calls.at(-1)?.[0]).toEqual(expect.objectContaining({ contacts_enabled: false }))
    expect(await screen.findByText(/Contact sync was skipped/i)).toBeInTheDocument()
  })

  it('keeps the setup draft and reuses the explicit TLS trust recovery', async () => {
    completeCreate = false
    createError = new ApiError(422, JSON.stringify({
      code: 'tls_untrusted',
      cert: {
        host: 'imap.example.test',
        port: 993,
        fingerprint_sha256: 'aabb',
        der_base64: 'trusted-cert',
      },
    }))
    const user = userEvent.setup()
    const { container } = render(<AddAccountForm onCancel={vi.fn()} onCreated={vi.fn()} />)
    await reachCapabilityReview(user, container)

    await user.click(screen.getByRole('button', { name: /Trust certificate/i }))
    await waitFor(() => expect(createMutate.mock.calls.at(-1)?.[0]).toEqual(expect.objectContaining({
      primary_email: 'person@example.test',
      imap_tls_cert: 'trusted-cert',
    })))
  })
})
