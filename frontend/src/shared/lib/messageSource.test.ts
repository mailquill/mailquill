import { describe, expect, it } from 'vitest'
import { messageHtml } from './messageSource'
import type { Message } from '@/shared/types'

function textMessage(body: string): Message {
  return {
    id: 'm1',
    account_id: 'a1',
    from_addr: 'Kontowecker <noreply@kontowecker.de>',
    to_addrs: 'fg@example.org',
    subject: 'Ihr Umsatzwecker',
    internal_date: '2026-09-02T10:15:00Z',
    is_read: false,
    is_flagged: false,
    body_text: body,
  } as Message
}

describe('messageHtml', () => {
  it('keeps the paragraphs of a CRLF plain-text body', () => {
    const html = messageHtml(textMessage('Guten Tag,\r\n\r\nauf dem Konto *2753:\r\n\r\n850,00 EUR'))

    expect(html.match(/<p /g)).toHaveLength(3)
    expect(html).toContain('>Guten Tag,<')
  })

  it('keeps single line breaks inside a paragraph', () => {
    const html = messageHtml(textMessage('Mit freundlichen Grüßen\r\nIhre Sparkasse'))

    expect(html).toContain('Mit freundlichen Grüßen<br>Ihre Sparkasse')
  })

  it('escapes markup before adding line breaks', () => {
    const html = messageHtml(textMessage('<b>a</b>\nb'))

    expect(html).toContain('&lt;b&gt;a&lt;/b&gt;<br>b')
  })

  it('prefers the message HTML part when present', () => {
    const message = { ...textMessage('plain'), body_html: '<p>rich</p>' } as Message

    expect(messageHtml(message)).toBe('<p>rich</p>')
  })
})
