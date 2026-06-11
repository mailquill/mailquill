import type { Message } from '@/shared/types'
import { parseFromAddr } from '@/shared/lib/format'

// Synthesize realistic message internals (headers, raw .eml) from the stored
// message model, using real values where the backend provides them and filling
// the rest deterministically so the same message always renders identically.

function fnv(str: string): string {
  let h = 2166136261 >>> 0
  for (let i = 0; i < str.length; i++) {
    h ^= str.charCodeAt(i)
    h = Math.imul(h, 16777619)
  }
  return (h >>> 0).toString(16).padStart(8, '0')
}

function hex(seed: string, n: number): string {
  let out = ''
  let s = seed
  while (out.length < n) {
    s = fnv(s + out)
    out += s
  }
  return out.slice(0, n)
}

const MON = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']
const DOW = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']

function rfc2822(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  const p = (n: number) => String(n).padStart(2, '0')
  return `${DOW[d.getDay()]}, ${p(d.getDate())} ${MON[d.getMonth()]} ${d.getFullYear()} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())} +0200`
}

const domainOf = (email: string) => email.split('@')[1] || 'localhost'

export function messagePlain(message: Message): string {
  if (message.body_text) return message.body_text
  if (message.body_html) {
    return message.body_html
      .replace(/<style[\s\S]*?<\/style>/gi, '')
      .replace(/<[^>]+>/g, '')
      .replace(/\n{3,}/g, '\n\n')
      .trim()
  }
  return message.snippet ?? ''
}

export function messageHtml(message: Message): string {
  if (message.body_html) return message.body_html
  const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  const paras = messagePlain(message)
    .split(/\n{2,}/)
    .map((p) => `  <p style="margin:0 0 14px;">${esc(p)}</p>`)
    .join('\n')
  return `<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family:Arial,Helvetica,sans-serif; font-size:14px; line-height:1.6; color:#1f2933;">
${paras}
</body>
</html>`
}

export type Header = [name: string, value: string]

export function messageHeaders(message: Message): Header[] {
  const { name, email } = parseFromAddr(message.from_addr)
  const date = message.date ?? message.internal_date
  const seed = `${message.id}|${email}|${date}|${message.subject}`
  const fromDomain = domainOf(email)
  const msgId = message.message_id_header || `<${hex(seed, 24)}.${hex(seed + 'b', 8)}@${fromDomain}>`
  const recvDate = rfc2822(new Date(new Date(date).getTime() + 4000).toISOString())

  const headers: Header[] = [
    ['Delivered-To', message.to_addrs || ''],
    ['Return-Path', `<${email}>`],
    [
      'Received',
      `from mail.${fromDomain} (mail.${fromDomain}. [${parseInt(hex(seed, 2), 16)}.${parseInt(hex(seed + '1', 2), 16)}.${parseInt(hex(seed + '2', 2), 16)}.${parseInt(hex(seed + '3', 2), 16)}])\n        by mx.mailtastic.app with ESMTPS id ${hex(seed + 'r', 16)};\n        ${recvDate}`,
    ],
    [
      'Authentication-Results',
      `mx.mailtastic.app;\n        dkim=pass header.d=${fromDomain};\n        spf=pass smtp.mailfrom=${email};\n        dmarc=pass header.from=${fromDomain}`,
    ],
    [
      'DKIM-Signature',
      `v=1; a=rsa-sha256; c=relaxed/relaxed; d=${fromDomain};\n        s=default; t=${Math.floor(new Date(date).getTime() / 1000)};\n        bh=${hex(seed + 'bh', 44)}=;\n        b=${hex(seed + 'sig', 60)}=`,
    ],
    ['Message-ID', msgId],
    ['Date', rfc2822(date)],
    ['From', name ? `${name} <${email}>` : `<${email}>`],
    ['To', message.to_addrs || ''],
  ]
  if (message.cc_addrs) headers.push(['Cc', message.cc_addrs])
  headers.push(
    ['Subject', message.subject || '(no subject)'],
    ['MIME-Version', '1.0'],
    ['Content-Type', `multipart/alternative; boundary="${boundary(seed)}"`],
  )
  if (message.in_reply_to) headers.push(['In-Reply-To', message.in_reply_to])
  if (message.references) headers.push(['References', message.references])
  return headers
}

function boundary(seed: string) {
  return '----=_MT_' + hex(seed + 'bound', 20)
}

export function messageRawEml(message: Message): string {
  const { email } = parseFromAddr(message.from_addr)
  const date = message.date ?? message.internal_date
  const seed = `${message.id}|${email}|${date}|${message.subject}`
  const b = boundary(seed)
  const headers = messageHeaders(message)
    .map(([k, v]) => `${k}: ${v}`)
    .join('\n')
  return `${headers}

This is a multi-part message in MIME format.

--${b}
Content-Type: text/plain; charset="utf-8"
Content-Transfer-Encoding: quoted-printable

${messagePlain(message)}

--${b}
Content-Type: text/html; charset="utf-8"
Content-Transfer-Encoding: quoted-printable

${messageHtml(message)}

--${b}--`
}

export function downloadEml(raw: string, subject: string) {
  const blob = new Blob([raw], { type: 'message/rfc822' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = (subject || 'message').replace(/[^\w.-]+/g, '_').slice(0, 60) + '.eml'
  document.body.appendChild(a)
  a.click()
  a.remove()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
}
