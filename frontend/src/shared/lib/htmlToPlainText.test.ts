import { describe, expect, it } from 'vitest'
import { htmlToPlainText } from './htmlToPlainText'

describe('htmlToPlainText', () => {
  it('turns paragraphs into blank-line separated blocks', () => {
    const text = htmlToPlainText('<p>Guten Tag,</p><p>auf dem Konto *2753:</p><p>850,00 EUR</p>')

    expect(text).toBe('Guten Tag,\n\nauf dem Konto *2753:\n\n850,00 EUR')
  })

  it('keeps <br> as a single line break', () => {
    expect(htmlToPlainText('<div>Mit freundlichen Grüßen<br>Ihre Sparkasse</div>')).toBe(
      'Mit freundlichen Grüßen\nIhre Sparkasse',
    )
  })

  it('collapses the insignificant newlines of the markup', () => {
    const text = htmlToPlainText('<p>\n  one\n  two\n</p>')

    expect(text).toBe('one two')
  })

  it('decodes entities and non-breaking spaces', () => {
    expect(htmlToPlainText('<p>Fisch &amp; Chips&nbsp;GmbH &lt;shop&gt;</p>')).toBe('Fisch & Chips GmbH <shop>')
  })

  it('drops script and style content instead of printing it', () => {
    const text = htmlToPlainText('<style>p{color:red}</style><script>alert(1)</script><p>Hallo</p>')

    expect(text).toBe('Hallo')
  })

  it('separates table cells and breaks rows', () => {
    const text = htmlToPlainText('<table><tr><td>Betrag</td><td>850,00</td></tr><tr><td>Saldo</td></tr></table>')

    expect(text).toBe('Betrag 850,00\nSaldo')
  })

  it('preserves the layout of preformatted blocks', () => {
    expect(htmlToPlainText('<pre>a\n  b</pre>')).toBe('a\n  b')
  })

  it('renders list items on their own lines', () => {
    expect(htmlToPlainText('<ul><li>eins</li><li>zwei</li></ul>')).toBe('eins\nzwei')
  })
})
