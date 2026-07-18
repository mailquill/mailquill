import { fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import i18n from '@/shared/i18n'
import { useModuleNav } from '@/shared/hooks/useModuleNav'
import { Sidebar } from './Sidebar'

vi.mock('@/shared/hooks/useAccounts', () => ({
  useAccounts: () => ({ data: [] }),
}))

vi.mock('@/shared/hooks/useContacts', () => ({
  useContactAccounts: () => ({ data: [] }),
  useContactGroups: () => ({
    data: [{ id: 'family', account_id: 'source', book_id: 'book', name: 'Family', remote_id: 'contactGroups/family', member_count: 2 }],
  }),
  useContacts: () => ({ data: [] }),
}))

vi.mock('@/shared/hooks/useUiPrefs', () => ({
  useUiPrefs: (selector: (state: { sidebarWidth: number }) => unknown) => selector({ sidebarWidth: 240 }),
}))

describe('Sidebar contact groups', () => {
  beforeEach(async () => {
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
})
