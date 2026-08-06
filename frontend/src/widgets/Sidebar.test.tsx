import { fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import i18n from '@/shared/i18n'
import { useModuleNav } from '@/shared/hooks/useModuleNav'
import { useUiPrefs } from '@/shared/hooks/useUiPrefs'
import type { Account, Folder } from '@/shared/types'
import { Sidebar } from './Sidebar'

const sidebarMocks = vi.hoisted(() => ({
  accounts: [] as Account[],
  folders: {} as Record<string, Folder[]>,
}))

vi.mock('@/shared/hooks/useAccounts', () => ({
  useAccounts: () => ({ data: sidebarMocks.accounts }),
  useFolders: (accountId: string) => ({ data: sidebarMocks.folders[accountId] ?? [] }),
  useSyncStatus: () => ({ data: undefined }),
  useReorderAccounts: () => ({ mutate: vi.fn() }),
}))

vi.mock('@/shared/hooks/useMessages', () => ({
  useMoveMessage: () => ({ mutate: vi.fn() }),
  useUnifiedCounts: () => ({ data: {} }),
}))

vi.mock('@/shared/hooks/useContacts', () => ({
  useContactAccounts: () => ({ data: [] }),
  useContactGroups: () => ({
    data: [{ id: 'family', account_id: 'source', book_id: 'book', name: 'Family', remote_id: 'contactGroups/family', member_count: 2 }],
  }),
  useContacts: () => ({ data: [] }),
}))

describe('Sidebar', () => {
  beforeEach(async () => {
    localStorage.clear()
    sidebarMocks.accounts = []
    sidebarMocks.folders = {}
    useUiPrefs.setState({
      sidebarWidth: 240,
      collapsedMailboxIds: [],
      expandedFoldersByMailbox: {},
    })
    useModuleNav.setState({ contactGroup: 'all' })
    await i18n.changeLanguage('en')
  })

  it('shows groups with counts and selects them for contact filtering', () => {
    render(<MemoryRouter initialEntries={['/mail/contacts']}><Sidebar onCompose={vi.fn()} /></MemoryRouter>)

    expect(screen.getByText('Groups')).toBeInTheDocument()
    expect(screen.getByText('Family')).toBeInTheDocument()
    expect(screen.getByTitle('2 group members')).toHaveTextContent('2')

    fireEvent.click(screen.getByText('Family'))

    expect(useModuleNav.getState().contactGroup).toBe('group:family')
  })

  it('keeps mailbox and folder expansion in persistent UI preferences', () => {
    sidebarMocks.accounts = [
      {
        id: 'work',
        display_name: 'Work mailbox',
        primary_email: 'work@example.test',
        imap_host: 'imap.example.test',
        imap_port: 993,
        imap_auth_scheme: 'plain',
        smtp_host: 'smtp.example.test',
        smtp_port: 465,
        smtp_auth_scheme: 'plain',
        body_sync_mode: 'lazy',
        sync_interval_secs: 300,
        sync_mode: 'idle',
        provider_kind: 'imap',
        created_at: '2026-01-01T00:00:00Z',
        sign_by_default: false,
        contacts: null,
      },
    ]
    sidebarMocks.folders.work = [
      {
        id: 'projects',
        account_id: 'work',
        name: 'Projects',
        full_path: 'Projects',
        folder_name: 'Projects',
        folder_name_server: 'Projects',
        folder_type: 'CUSTOM',
        unread_count: 0,
      },
      {
        id: 'client',
        account_id: 'work',
        name: 'Client',
        full_path: 'Projects/Client',
        folder_name: 'Client',
        folder_name_server: 'Client',
        folder_type: 'CUSTOM',
        unread_count: 0,
      },
    ]

    const { unmount } = render(
      <MemoryRouter initialEntries={['/mail/unified']}>
        <Sidebar onCompose={vi.fn()} />
      </MemoryRouter>,
    )

    expect(screen.queryByText('Client')).not.toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Expand folder Projects' }))
    expect(screen.getByText('Client')).toBeInTheDocument()
    expect(useUiPrefs.getState().expandedFoldersByMailbox.work).toEqual(['Projects'])

    fireEvent.click(screen.getByRole('button', { name: 'Collapse mailbox Work mailbox' }))
    expect(screen.queryByText('Projects')).not.toBeInTheDocument()
    expect(useUiPrefs.getState().collapsedMailboxIds).toEqual(['work'])

    unmount()
    render(
      <MemoryRouter initialEntries={['/mail/unified']}>
        <Sidebar onCompose={vi.fn()} />
      </MemoryRouter>,
    )
    expect(screen.queryByText('Projects')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Expand mailbox Work mailbox' }))
    expect(screen.getByText('Client')).toBeInTheDocument()
  })
})
