import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Account, SyncStatus } from '@/shared/types'
import { AccountCard } from './SettingsPage'

const updateMutate = vi.fn((_variables: unknown, options?: { onSuccess?: () => void }) => options?.onSuccess?.())
const { oauthRedirect } = vi.hoisted(() => ({ oauthRedirect: vi.fn() }))
let syncStatus: Partial<SyncStatus> | undefined

vi.mock('@/shared/hooks/useAccounts', () => ({
  useUpdateAccount: () => ({ mutate: updateMutate, isPending: false, isError: false, error: null }),
  useDeleteAccount: () => ({ mutate: vi.fn(), isPending: false }),
  useFolders: () => ({ data: [] }),
  useSetFolderSync: () => ({ mutate: vi.fn() }),
  useSetAllFoldersSync: () => ({ mutate: vi.fn() }),
  useSyncStatus: () => ({ data: syncStatus }),
  useEnableMailboxContacts: () => ({ mutate: vi.fn(), isPending: false }),
  useDisableMailboxContacts: () => ({ mutate: vi.fn(), isPending: false }),
  useDiscoverMailboxContacts: () => ({ mutate: vi.fn(), isPending: false, data: null, error: null }),
}))

vi.mock('@/shared/hooks/useContacts', () => ({
  useSyncContactAccount: () => ({ mutate: vi.fn(), isPending: false }),
}))

// Keep oauthProviderFromStatus's real parsing logic (it's pure), only stub
// the actual browser redirect.
vi.mock('@/shared/lib/oauth', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/shared/lib/oauth')>()
  return { ...actual, startOAuthRedirect: oauthRedirect }
})

const account = {
  id: 'mailbox-1',
  display_name: 'Work',
  primary_email: 'work@example.test',
  imap_host: 'imap.example.test',
  imap_port: 993,
  imap_auth_scheme: 'plain',
  smtp_host: 'smtp.example.test',
  smtp_port: 587,
  smtp_auth_scheme: 'plain',
  body_sync_mode: 'lazy',
  sync_interval_secs: 300,
  sync_mode: 'idle',
  provider_kind: 'imap',
  contacts: null,
} as Account

describe('account credential editing and reauth', () => {
  beforeEach(() => {
    updateMutate.mockClear()
    oauthRedirect.mockClear()
    syncStatus = undefined
  })

  it('leaves password fields blank and omits them from the save payload when untouched', async () => {
    const user = userEvent.setup()
    render(<AccountCard account={account} />)

    await user.click(screen.getByRole('button', { name: /Work/ }))
    const smtpPassword = screen.getByLabelText('SMTP password') as HTMLInputElement
    expect(smtpPassword.value).toBe('')
    expect(smtpPassword.type).toBe('password')

    await user.click(screen.getByRole('button', { name: 'Save changes' }))

    expect(updateMutate).toHaveBeenCalled()
    const [{ data }] = updateMutate.mock.calls[0] as [{ data: Record<string, unknown> }]
    expect(data).not.toHaveProperty('imap_password')
    expect(data).not.toHaveProperty('smtp_password')
  })

  it('sends a typed password and clears the field again once saved', async () => {
    const user = userEvent.setup()
    render(<AccountCard account={account} />)

    await user.click(screen.getByRole('button', { name: /Work/ }))
    const smtpPassword = screen.getByLabelText('SMTP password') as HTMLInputElement
    await user.type(smtpPassword, 'new-app-password')
    await user.click(screen.getByRole('button', { name: 'Save changes' }))

    const [{ data }] = updateMutate.mock.calls[0] as [{ data: Record<string, unknown> }]
    expect(data.smtp_password).toBe('new-app-password')
    expect(data).not.toHaveProperty('imap_password')
    // updateMutate's mock invokes onSuccess synchronously, which must clear
    // the typed password back out of the form.
    await waitFor(() => expect(smtpPassword.value).toBe(''))
  })

  it('shows a credentials error banner and expands+focuses the password field on demand', async () => {
    syncStatus = { state: 'error', error: '535 5.7.8 Username and Password not accepted' }
    const user = userEvent.setup()
    render(<AccountCard account={account} />)

    expect(screen.getByText('535 5.7.8 Username and Password not accepted')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Update credentials' }))

    const smtpPassword = await screen.findByLabelText('SMTP password')
    await waitFor(() => expect(smtpPassword).toHaveFocus())
  })

  it('shows a reconnect banner and starts the right OAuth flow for reauth_required', async () => {
    syncStatus = { state: 'reauth_required', error: 'oauth_reauthentication_required:google' }
    const user = userEvent.setup()
    render(<AccountCard account={account} />)

    await user.click(screen.getByRole('button', { name: 'Reconnect' }))
    expect(oauthRedirect).toHaveBeenCalledWith('google', 'mailbox-1')
  })

  it('auto-expands and focuses credentials when opened via autoFocusCredentials', async () => {
    render(<AccountCard account={account} autoFocusCredentials />)

    const smtpPassword = await screen.findByLabelText('SMTP password')
    await waitFor(() => expect(smtpPassword).toHaveFocus())
  })
})
