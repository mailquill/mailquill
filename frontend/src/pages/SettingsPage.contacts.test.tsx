import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { Account } from '@/shared/types'
import { ContactCapabilityRow } from './SettingsPage'

const disableMutate = vi.fn()
const enableMutate = vi.fn()
const discoveredBooks = [
  { remote_id: 'default', display_name: 'Personal', parent_remote_id: null, is_default: true, is_writable: true },
  { remote_id: 'team', display_name: 'Team', parent_remote_id: null, is_default: false, is_writable: true },
]
const discoverMutate = vi.fn((_variables: { accountId: string; selectedBookRemoteIds?: string[] }, options?: { onSuccess?: (data: { books: typeof discoveredBooks }) => void }) => {
  options?.onSuccess?.({ books: discoveredBooks })
})

vi.mock('@/shared/hooks/useAccounts', () => ({
  useDisableMailboxContacts: () => ({ mutate: disableMutate, isPending: false }),
  useEnableMailboxContacts: () => ({ mutate: enableMutate, isPending: false }),
  useDiscoverMailboxContacts: () => ({ mutate: discoverMutate, isPending: false, data: { source_id: 'source-1', books: discoveredBooks }, error: null }),
  useUpdateAccount: () => ({ mutate: vi.fn(), isPending: false }),
  useAccounts: () => ({ data: [] }),
  useDeleteAccount: () => ({ mutate: vi.fn(), isPending: false }),
  useFolders: () => ({ data: [] }),
  useSetFolderSync: () => ({ mutate: vi.fn() }),
}))

vi.mock('@/shared/hooks/useContacts', () => ({
  useSyncContactAccount: () => ({ mutate: vi.fn(), isPending: false }),
}))

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
  contacts: {
    source_id: 'source-1',
    provider: 'cardav',
    state: 'idle',
    reason: null,
    enabled: true,
    last_synced_at: null,
    cache_retained: true,
  },
} as Account

describe('mailbox Contacts capability controls', () => {
  it('defaults disablement to retaining a read-only local cache', async () => {
    const user = userEvent.setup()
    render(<ContactCapabilityRow account={account} />)

    await user.click(screen.getByRole('button', { name: 'Disable' }))
    expect(screen.getByRole('radio', { name: /Keep downloaded contacts/ })).toBeChecked()

    await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Disable' }))
    expect(disableMutate).toHaveBeenCalledWith(
      { accountId: 'mailbox-1', keepDownloadedContacts: true },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    )
  })

  it('discovers stored-credential CardDAV books and enables the selected books', async () => {
    const user = userEvent.setup()
    render(<ContactCapabilityRow account={{
      ...account,
      contacts: { ...account.contacts!, state: 'disabled', enabled: false },
    }} />)

    await user.click(screen.getByRole('button', { name: 'Enable contacts' }))
    const dialog = screen.getByRole('dialog')
    expect(within(dialog).getByRole('checkbox', { name: /Personal/ })).toBeChecked()
    expect(within(dialog).getByRole('checkbox', { name: /Team/ })).not.toBeChecked()

    await user.click(within(dialog).getByRole('button', { name: 'Enable selected books' }))
    expect(discoverMutate).toHaveBeenLastCalledWith(
      { accountId: 'mailbox-1', selectedBookRemoteIds: ['default'] },
      expect.objectContaining({ onSuccess: expect.any(Function) }),
    )
    expect(enableMutate).toHaveBeenCalledWith('mailbox-1', expect.objectContaining({ onSuccess: expect.any(Function) }))
  })

  it('shows disabled provider API guidance and retries without another consent redirect', async () => {
    const user = userEvent.setup()
    render(<ContactCapabilityRow account={{
      ...account,
      contacts: {
        ...account.contacts!,
        provider: 'google',
        state: 'unavailable',
        reason: 'provider_configuration_required',
      },
    }} />)

    expect(screen.getByRole('alert')).toHaveTextContent(/Google contacts API is not enabled/i)
    expect(screen.getByRole('link', { name: 'Open People API' })).toHaveAttribute(
      'href',
      'https://console.cloud.google.com/apis/library/people.googleapis.com',
    )
    await user.click(screen.getByRole('button', { name: 'Try again' }))
    expect(enableMutate).toHaveBeenCalledWith('mailbox-1')
  })
})
