import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { RecipientChips } from './RecipientChips'

type SearchResult = {
  data: Array<{
    id: string
    display_name: string | null
    email: string
    source: 'contact' | 'sender'
  }>
}

const recipientSearch = vi.fn<(query: string, mailboxId?: string) => SearchResult>(() => ({ data: [] }))

vi.mock('@/shared/hooks/useContacts', () => ({
  useRecipientSuggestions: (query: string, mailboxId?: string) => recipientSearch(query, mailboxId),
}))

describe('RecipientChips recipient autocomplete', () => {
  it('passes mailbox context and selects a suggestion with the keyboard', async () => {
    recipientSearch.mockImplementation((query: string) => ({
      data: query.length >= 2 ? [{
        id: 'contact-1',
        display_name: 'Ada Lovelace',
        email: 'ada@example.test',
        source: 'contact',
      }] : [],
    }))
    const onChange = vi.fn()
    const user = userEvent.setup()
    render(<RecipientChips label="To" value={[]} onChange={onChange} mailboxId="mailbox-1" />)

    const input = screen.getByRole('combobox')
    await user.type(input, 'ad')
    expect(recipientSearch).toHaveBeenLastCalledWith('ad', 'mailbox-1')
    expect(screen.getByRole('option', { name: /Ada Lovelace/ })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByText('Contact')).toBeInTheDocument()

    await user.keyboard('{Enter}')
    expect(onChange).toHaveBeenCalledWith(['Ada Lovelace <ada@example.test>'])
  })

  it('labels and selects an address learned from a known sender', async () => {
    recipientSearch.mockReturnValue({
      data: [{
        id: 'sender:sender@example.test',
        display_name: 'Known Sender',
        email: 'sender@example.test',
        source: 'sender',
      }],
    })
    const onChange = vi.fn()
    const user = userEvent.setup()
    render(<RecipientChips label="To" value={[]} onChange={onChange} />)

    await user.type(screen.getByRole('combobox'), 's')
    expect(screen.getByText('Known sender')).toBeInTheDocument()
    await user.keyboard('{Enter}')

    expect(onChange).toHaveBeenCalledWith(['Known Sender <sender@example.test>'])
  })
})
