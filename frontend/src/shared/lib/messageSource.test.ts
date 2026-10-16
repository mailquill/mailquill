import { describe, expect, it } from 'vitest'
import { messageHeaders, messageHtml, messagePlain, messageRawEml } from './messageSource'
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

describe('messagePlain', () => {
  it('prefers the plain-text part verbatim', () => {
    expect(messagePlain(textMessage('Zeile 1\r\nZeile 2'))).toBe('Zeile 1\r\nZeile 2')
  })

  it('keeps the line structure of an HTML-only message', () => {
    const message = {
      ...textMessage(''),
      body_text: null,
      body_html: '<p>Guten Tag,</p><div>Mit freundlichen Grüßen<br>Ihre Sparkasse</div>',
    } as Message

    expect(messagePlain(message)).toBe('Guten Tag,\n\nMit freundlichen Grüßen\nIhre Sparkasse')
  })
})

describe('message date headers', () => {
  it('prefers a valid message date and formats timestamps with a matching UTC offset', () => {
    const message = { ...textMessage('body'), date: '2026-09-01T23:59:58-04:00' }
    const headers = Object.fromEntries(messageHeaders(message))

    expect(headers.Date).toBe('Wed, 02 Sep 2026 03:59:58 +0000')
    expect(headers.Received).toContain('Wed, 02 Sep 2026 04:00:02 +0000')
    expect(headers['DKIM-Signature']).toContain(`t=${Date.parse(message.date) / 1000};`)
  })

  it.each([null, '', 'not a date', '2026-99-99T00:00:00Z', '+999999-01-01T00:00:00Z'])(
    'falls back to the internal date when the message date is %j',
    (date) => {
      const message = { ...textMessage('body'), date }
      const headers = Object.fromEntries(messageHeaders(message))

      expect(headers.Date).toBe('Wed, 02 Sep 2026 10:15:00 +0000')
      expect(headers.Received).toContain('Wed, 02 Sep 2026 10:15:04 +0000')
      expect(headers['DKIM-Signature']).toContain(`t=${Date.parse(message.internal_date) / 1000};`)
      expect(messageRawEml(message)).toContain(`Date: ${headers.Date}`)
    },
  )

  it.each([
    { date: 'not a date', internal_date: 'also invalid' },
    { date: '', internal_date: '' },
    { date: null, internal_date: 'invalid' },
  ])('keeps headers and raw export usable without valid dates: %j', (dates) => {
    const message = { ...textMessage('The message body'), ...dates }
    const headers = Object.fromEntries(messageHeaders(message))
    const raw = messageRawEml(message)

    expect(headers).not.toHaveProperty('Date')
    expect(headers['DKIM-Signature']).not.toMatch(/\bt=/)
    expect(headers.From).toBe('Kontowecker <noreply@kontowecker.de>')
    expect(raw).toContain('The message body')
    expect(raw).not.toMatch(/NaN|Invalid Date|undefined/)
    expect(messageRawEml(message)).toBe(raw)
  })

  it('uses a valid message date even when the internal date is invalid', () => {
    const message = {
      ...textMessage('body'),
      date: '2026-09-02T10:15:00Z',
      internal_date: 'invalid',
    }

    expect(Object.fromEntries(messageHeaders(message)).Date).toBe('Wed, 02 Sep 2026 10:15:00 +0000')
  })

  it('handles delivery delay overflow at the maximum supported date', () => {
    const message = { ...textMessage('body'), date: '+275760-09-13T00:00:00.000Z' }
    const headers = Object.fromEntries(messageHeaders(message))

    expect(headers.Received).toContain(headers.Date)
    expect(messageRawEml(message)).not.toMatch(/NaN|Invalid Date/)
  })
})
