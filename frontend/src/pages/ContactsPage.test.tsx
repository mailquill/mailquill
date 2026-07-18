import { render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import i18n from '@/shared/i18n'
import { ContactsPage } from './ContactsPage'

const useContacts = vi.fn()
let contactGroup = 'all'

vi.mock('@/shared/hooks/useContacts', () => ({
  useContactAccounts: () => ({ data: [] }),
  useContactBooks: () => ({ data: [] }),
  useContactGroups: () => ({
    data: [{ id: 'family', account_id: 'source', book_id: 'book', name: 'Family', remote_id: 'contactGroups/family', member_count: 2 }],
  }),
  useContacts: (...args: unknown[]) => useContacts(...args),
  useCreateContact: () => ({ mutate: vi.fn(), isPending: false }),
  useCreateContactAccount: () => ({ mutate: vi.fn(), isPending: false }),
  useDeleteContact: () => ({ mutate: vi.fn(), isPending: false }),
  useDeleteContactAccount: () => ({ mutate: vi.fn(), isPending: false }),
  useSyncContactAccount: () => ({ mutate: vi.fn(), isPending: false }),
  useUpdateContact: () => ({ mutate: vi.fn(), isPending: false }),
}))

vi.mock('@/shared/hooks/useAccounts', () => ({
  useAccounts: () => ({ data: [] }),
  useDiscoverMailboxContacts: () => ({ mutate: vi.fn(), isPending: false }),
  useEnableMailboxContacts: () => ({ mutate: vi.fn(), isPending: false }),
}))

vi.mock('@/shared/hooks/useModuleNav', () => ({
  useModuleNav: () => ({ contactGroup }),
}))

vi.mock('react-router-dom', async (importOriginal) => ({
  ...await importOriginal<typeof import('react-router-dom')>(),
  useOutletContext: () => ({ openCompose: vi.fn() }),
}))

describe('ContactsPage groups', () => {
  beforeEach(() => {
    useContacts.mockReset()
    useContacts.mockReturnValue({ data: [], isLoading: false })
    contactGroup = 'all'
  })

  it('applies a group selected in the sidebar', () => {
    contactGroup = 'group:family'
    render(<MemoryRouter><ContactsPage /></MemoryRouter>)

    expect(useContacts).toHaveBeenLastCalledWith(undefined, '', undefined, undefined, 'family')
  })

  it('contains localized copy for the contacts group controls', () => {
    expect(i18n.t('contacts.allGroups', { lng: 'de' })).toBe('Alle Kontakte')
    expect(i18n.t('contacts.groupMembers', { lng: 'en', count: 2 })).toBe('2 group members')
  })

  it('keeps source status and mailbox book filters out of the overview', () => {
    render(<MemoryRouter><ContactsPage /></MemoryRouter>)

    expect(screen.queryByRole('combobox', { name: i18n.t('contacts.filterMailbox') })).not.toBeInTheDocument()
    expect(screen.queryByRole('combobox', { name: i18n.t('contacts.filterBook') })).not.toBeInTheDocument()
    expect(screen.queryByText(i18n.t('contacts.noAccounts'))).not.toBeInTheDocument()
  })

  it('shows the provider of the selected contact', () => {
    useContacts.mockReturnValue({
      data: [{
        id: 'contact-1', account_id: 'source-1', uid: 'people/1', display_name: 'Alex Bücken',
        given_name: 'Alex', family_name: 'Bücken', org: null, title: null, emails: [], phones: [],
        addresses: [], notes: null, photo_blob_key: null, raw_vcard: null, synced_at: null,
        book_id: 'book-1', remote_version: null, photo_reference: null, photo_version: null,
        photo_content_type: null, source_email_account_id: 'mailbox-1', source_provider: 'google',
        source_state: 'idle', source_enabled: true, source_writable: false, groups: [],
      }],
      isLoading: false,
    })

    render(<MemoryRouter><ContactsPage /></MemoryRouter>)

    expect(screen.getByText('Google')).toBeInTheDocument()
  })
})
