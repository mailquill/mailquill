import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { RecipientChips } from './RecipientChips'

type SearchResult = {
  data: Array<{
    id: string
    display_name: string
    emails: Array<{ value: string; label: string; primary: boolean }>
  }>
}

const contactSearch = vi.fn<(query: string, mailboxId?: string) => SearchResult>(() => ({ data: [] }))

vi.mock('@/shared/hooks/useContacts', () => ({
  useContactSearch: (query: string, mailboxId?: string) => contactSearch(query, mailboxId),
}))

describe('RecipientChips contact autocomplete', () => {
  it('passes mailbox context and selects a suggestion with the keyboard', async () => {
    contactSearch.mockImplementation((query: string) => ({
      data: query.length >= 2 ? [{
        id: 'contact-1',
        display_name: 'Ada Lovelace',
        emails: [{ value: 'ada@example.test', label: 'work', primary: true }],
      }] : [],
    }))
    const onChange = vi.fn()
    const user = userEvent.setup()
    render(<RecipientChips label="To" value={[]} onChange={onChange} mailboxId="mailbox-1" />)

    const input = screen.getByRole('combobox')
    await user.type(input, 'ad')
    expect(contactSearch).toHaveBeenLastCalledWith('ad', 'mailbox-1')
    expect(screen.getByRole('option', { name: /Ada Lovelace/ })).toHaveAttribute('aria-selected', 'true')

    await user.keyboard('{Enter}')
    expect(onChange).toHaveBeenCalledWith(['Ada Lovelace <ada@example.test>'])
  })
})
