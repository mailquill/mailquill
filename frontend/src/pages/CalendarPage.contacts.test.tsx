import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { describe, expect, it, vi } from 'vitest'
import { CalendarPage } from './CalendarPage'

const recipientSearch = vi.fn((query: string, mailboxId?: string) => ({
  data: query.length >= 2
    ? [{ id: 'contact-1', display_name: 'Ada Lovelace', email: 'ada@example.test', source: 'contact' as const }]
    : [],
  mailboxId,
}))

vi.mock('@/shared/hooks/useContacts', async (importOriginal) => ({
  ...await importOriginal<typeof import('@/shared/hooks/useContacts')>(),
  useRecipientSuggestions: (query: string, mailboxId?: string) => recipientSearch(query, mailboxId),
}))

vi.mock('@/shared/hooks/useAccounts', () => ({
  useAccounts: () => ({ data: [] }),
}))

vi.mock('@/shared/hooks/useCalendar', () => ({
  useCalendarAccounts: () => ({ data: [] }),
  useCalendars: () => ({ data: [{ id: 'calendar-1', name: 'Work', account_id: 'mailbox-1', color: '#000000' }] }),
  useCreateCalendarAccount: () => ({ mutate: vi.fn(), reset: vi.fn(), isPending: false, error: null }),
  useCreateCalendar: () => ({ mutateAsync: vi.fn() }),
  useCreateEvent: () => ({ mutate: vi.fn(), isPending: false }),
  useDeleteCalendarAccount: () => ({ mutate: vi.fn(), isPending: false }),
  useUpdateEvent: () => ({ mutate: vi.fn(), isPending: false }),
  useDeleteEvent: () => ({ mutate: vi.fn(), isPending: false }),
  useEvents: () => ({ data: [] }),
}))

vi.mock('@/shared/hooks/useModuleNav', () => ({
  useModuleNav: () => ({ hiddenCalendars: [] }),
}))

vi.mock('@/widgets/DavSyncButton', () => ({ DavSyncButton: () => null }))

describe('CalendarPage contact autocomplete', () => {
  it('ranks and selects attendees using the default calendar mailbox', async () => {
    const user = userEvent.setup()
    render(<MemoryRouter initialEntries={['/mail/calendar?new=1']}><CalendarPage /></MemoryRouter>)

    const attendeeInput = screen.getByRole('combobox', { name: 'Attendees' })
    await user.type(attendeeInput, 'ad')
    expect(recipientSearch).toHaveBeenLastCalledWith('ad', 'mailbox-1')
    await user.keyboard('{Enter}')

    expect(screen.getByText(/Ada Lovelace <ada@example\.test>/)).toBeInTheDocument()
  })
})
