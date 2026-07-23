import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Account, Message } from '@/shared/types'
import { ComposeDialog } from './ComposeDialog'

let accounts: Account[] = []
const mutateSend = vi.fn()
const mutateSave = vi.fn()

const sourceMessage: Message = {
  id: 'message-1',
  account_id: 'mailbox-1',
  folder_id: 'folder-1',
  uid: 1,
  message_id_header: '<message-1@example.test>',
  thread_id: 'thread-1',
  in_reply_to: null,
  references: null,
  list_id: null,
  subject: 'Reservation confirmation',
  from_addr: 'Reservations <reservations@example.test>',
  to_addrs: 'sender@example.test',
  cc_addrs: '',
  snippet: 'Thank you for your reservation.',
  date: '2026-07-22T10:00:00Z',
  internal_date: '2026-07-22T10:00:00Z',
  is_read: true,
  is_flagged: false,
  is_deleted: false,
  body_text: 'Thank you for your reservation.',
  draft_to: [],
}

vi.mock('@/shared/hooks/useAccounts', () => ({
  useAccounts: () => ({ data: accounts }),
}))

vi.mock('@/shared/hooks/useMessages', () => ({
  useSendMessage: () => ({ mutate: mutateSend, isPending: false, error: null }),
  useSaveDraft: () => ({ mutateAsync: mutateSave, isPending: false, error: null }),
}))

vi.mock('@/shared/hooks/useOnlineStatus', () => ({
  useOnlineStatus: () => true,
}))

vi.mock('@/shared/hooks/usePgp', () => ({
  usePgpKeys: () => ({ data: [] }),
}))

vi.mock('@/shared/hooks/useContacts', () => ({
  useRecipientSuggestions: () => ({ data: [] }),
}))

describe('ComposeDialog account bootstrap', () => {
  beforeEach(() => {
    accounts = []
    mutateSend.mockReset()
    mutateSave.mockReset()
  })

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

  it('uses the sender as reply recipient when non-draft details contain an empty draft recipient list', () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(
      <QueryClientProvider client={queryClient}>
        <ComposeDialog
          open
          initialState={{ mode: 'reply', sourceMessage }}
          onClose={vi.fn()}
        />
      </QueryClientProvider>,
    )

    expect(screen.getByText('reservations@example.test')).toBeInTheDocument()
  })

  it('reports queue acceptance before closing the composer', async () => {
    accounts = [{
      id: 'mailbox-1', display_name: 'Work', primary_email: 'sender@example.test',
      imap_host: 'imap.example.test', imap_port: 993, imap_auth_scheme: 'password',
      smtp_host: 'smtp.example.test', smtp_port: 465, smtp_auth_scheme: 'password',
      body_sync_mode: 'full', sync_interval_secs: 60, sync_mode: 'poll', provider_kind: 'imap',
      created_at: '2026-07-18T00:00:00Z', sign_by_default: false, contacts: null,
    }]
    const onClose = vi.fn()
    const onSendQueued = vi.fn()
    mutateSend.mockImplementation((_: unknown, options: { onSuccess?: (data: { send_id: string }) => void }) => {
      options.onSuccess?.({ send_id: 'send-1' })
    })
    const user = userEvent.setup()
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(
      <QueryClientProvider client={queryClient}>
        <ComposeDialog
          open
          initialState={{ mode: 'new' }}
          onClose={onClose}
          onSendQueued={onSendQueued}
        />
      </QueryClientProvider>,
    )

    await user.type(screen.getByRole('combobox', { name: 'To' }), 'recipient@example.test{Enter}')
    await user.type(screen.getByRole('textbox', { name: 'Subject' }), 'Invoice')
    await user.click(screen.getByRole('button', { name: 'Send' }))

    expect(onSendQueued).toHaveBeenCalledWith({ sendId: 'send-1', subject: 'Invoice' })
    expect(onClose).toHaveBeenCalledOnce()
  })

  it('saves a non-empty message as a draft before closing', async () => {
    accounts = [{
      id: 'mailbox-1', display_name: 'Work', primary_email: 'sender@example.test',
      imap_host: 'imap.example.test', imap_port: 993, imap_auth_scheme: 'password',
      smtp_host: 'smtp.example.test', smtp_port: 465, smtp_auth_scheme: 'password',
      body_sync_mode: 'full', sync_interval_secs: 60, sync_mode: 'poll', provider_kind: 'imap',
      created_at: '2026-07-18T00:00:00Z', sign_by_default: false, contacts: null,
    }]
    mutateSave.mockResolvedValue({ id: 'draft-1' })
    const onClose = vi.fn()
    const user = userEvent.setup()
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    render(
      <QueryClientProvider client={queryClient}>
        <ComposeDialog open initialState={{ mode: 'new' }} onClose={onClose} />
      </QueryClientProvider>,
    )

    await user.type(screen.getByRole('textbox', { name: 'Subject' }), 'Unfinished')
    await user.click(screen.getByRole('button', { name: 'Save draft' }))

    expect(mutateSave).toHaveBeenCalledWith(expect.objectContaining({
      account_id: 'mailbox-1',
      subject: 'Unfinished',
    }))
    expect(onClose).toHaveBeenCalledOnce()
  })
})
