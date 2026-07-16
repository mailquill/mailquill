// Client-side mail-server autodiscovery, ported from the Mailquill design.
// 1) known provider by domain, 2) Exchange autodiscover, 3) backend lookup
// (RFC 6186 SRV, Thunderbird ISPDB, MX + reachability probe), 4) autoconfig
// convention (imap./smtp.<domain>). Returns server config plus a source tag.
// Static entries below are validated against the ISPDB; the backend covers
// everything else, so this table only needs the high-traffic providers.

import { apiGet } from '@/shared/api'

export type Security = 'ssl' | 'starttls' | 'none'

export interface ServerConfig {
  imapHost: string
  imapPort: number
  imapSecurity: Security
  imapUser: string
  smtpHost: string
  smtpPort: number
  smtpSecurity: Security
  smtpUser: string
  carddavUrl: string
  caldavUrl: string
}

export type DiscoverSource = 'provider' | 'exchange' | 'dns' | 'autoconfig'

export interface DiscoverResult extends ServerConfig {
  source: DiscoverSource
  provider: string
  oauth: boolean
}

interface ProviderDef {
  name: string
  oauth?: boolean
  imapHost: string
  imapPort: number
  imapSecurity: Security
  smtpHost: string
  smtpPort: number
  smtpSecurity: Security
  carddavUrl: string
  caldavUrl: string
}

export type ProviderPresetId = 'auto' | 'gmail' | 'outlook' | 'yahoo' | 'webde' | 'gmxde' | 'gmxnet' | 'imap'

export interface ProviderPresetOption {
  id: ProviderPresetId
  label: string
}

// Known providers keyed by domain. {email}/{domain} are substituted at lookup.
const SRV_PROVIDERS: Record<string, ProviderDef> = (() => {
  const G: ProviderDef = { name: 'Google', oauth: true, imapHost: 'imap.gmail.com', imapPort: 993, imapSecurity: 'ssl', smtpHost: 'smtp.gmail.com', smtpPort: 465, smtpSecurity: 'ssl', carddavUrl: 'https://www.googleapis.com/carddav/v1/principals/{email}/lists/default/', caldavUrl: 'https://apidata.googleusercontent.com/caldav/v2/{email}/events/' }
  const O: ProviderDef = { name: 'Microsoft / Outlook', oauth: true, imapHost: 'outlook.office365.com', imapPort: 993, imapSecurity: 'ssl', smtpHost: 'smtp.office365.com', smtpPort: 587, smtpSecurity: 'starttls', carddavUrl: 'https://outlook.office365.com/EWS/Exchange.asmx', caldavUrl: 'https://outlook.office365.com/EWS/Exchange.asmx' }
  const Y: ProviderDef = { name: 'Yahoo', oauth: true, imapHost: 'imap.mail.yahoo.com', imapPort: 993, imapSecurity: 'ssl', smtpHost: 'smtp.mail.yahoo.com', smtpPort: 465, smtpSecurity: 'ssl', carddavUrl: 'https://carddav.address.yahoo.com/', caldavUrl: 'https://caldav.calendar.yahoo.com/' }
  const P: ProviderDef = { name: 'Proton Mail (Bridge)', imapHost: '127.0.0.1', imapPort: 1143, imapSecurity: 'starttls', smtpHost: '127.0.0.1', smtpPort: 1025, smtpSecurity: 'starttls', carddavUrl: '', caldavUrl: '' }
  const WEB: ProviderDef = { name: 'WEB.DE', imapHost: 'imap.web.de', imapPort: 993, imapSecurity: 'ssl', smtpHost: 'smtp.web.de', smtpPort: 465, smtpSecurity: 'ssl', carddavUrl: 'https://carddav.web.de/', caldavUrl: 'https://caldav.web.de/' }
  const GMX: ProviderDef = { name: 'GMX', imapHost: 'imap.gmx.net', imapPort: 993, imapSecurity: 'ssl', smtpHost: 'mail.gmx.net', smtpPort: 465, smtpSecurity: 'ssl', carddavUrl: 'https://carddav.gmx.net/', caldavUrl: 'https://caldav.gmx.net/' }
  const IC: ProviderDef = { name: 'iCloud', imapHost: 'imap.mail.me.com', imapPort: 993, imapSecurity: 'ssl', smtpHost: 'smtp.mail.me.com', smtpPort: 587, smtpSecurity: 'starttls', carddavUrl: 'https://contacts.icloud.com/', caldavUrl: 'https://caldav.icloud.com/' }
  const AOL: ProviderDef = { name: 'AOL', imapHost: 'imap.aol.com', imapPort: 993, imapSecurity: 'ssl', smtpHost: 'smtp.aol.com', smtpPort: 465, smtpSecurity: 'ssl', carddavUrl: '', caldavUrl: '' }
  const TO: ProviderDef = { name: 'T-Online', imapHost: 'secureimap.t-online.de', imapPort: 993, imapSecurity: 'ssl', smtpHost: 'securesmtp.t-online.de', smtpPort: 465, smtpSecurity: 'ssl', carddavUrl: '', caldavUrl: '' }
  const MB: ProviderDef = { name: 'mailbox.org', imapHost: 'imap.mailbox.org', imapPort: 993, imapSecurity: 'ssl', smtpHost: 'smtp.mailbox.org', smtpPort: 587, smtpSecurity: 'starttls', carddavUrl: 'https://dav.mailbox.org/', caldavUrl: 'https://dav.mailbox.org/' }
  const PO: ProviderDef = { name: 'Posteo', imapHost: 'posteo.de', imapPort: 993, imapSecurity: 'ssl', smtpHost: 'posteo.de', smtpPort: 465, smtpSecurity: 'ssl', carddavUrl: 'https://posteo.de:8443/', caldavUrl: 'https://posteo.de:8443/' }
  const ZO: ProviderDef = { name: 'Zoho', imapHost: 'imap.zoho.com', imapPort: 993, imapSecurity: 'ssl', smtpHost: 'smtp.zoho.com', smtpPort: 465, smtpSecurity: 'ssl', carddavUrl: '', caldavUrl: '' }
  const ZOEU: ProviderDef = { name: 'Zoho (EU)', imapHost: 'imap.zoho.eu', imapPort: 993, imapSecurity: 'ssl', smtpHost: 'smtp.zoho.eu', smtpPort: 465, smtpSecurity: 'ssl', carddavUrl: '', caldavUrl: '' }
  const FM: ProviderDef = { name: 'Fastmail', imapHost: 'imap.fastmail.com', imapPort: 993, imapSecurity: 'ssl', smtpHost: 'smtp.fastmail.com', smtpPort: 465, smtpSecurity: 'ssl', carddavUrl: 'https://carddav.fastmail.com/', caldavUrl: 'https://caldav.fastmail.com/' }
  return {
    'gmail.com': G, 'googlemail.com': G,
    'outlook.com': O, 'outlook.de': O, 'hotmail.com': O, 'hotmail.de': O, 'live.com': O, 'live.de': O, 'msn.com': O,
    'yahoo.com': Y, 'yahoo.de': Y, 'ymail.com': Y,
    'proton.me': P, 'protonmail.com': P, 'pm.me': P,
    'web.de': WEB, 'gmx.de': GMX, 'gmx.net': GMX, 'gmx.com': GMX, 'gmx.ch': GMX, 'gmx.at': GMX,
    'icloud.com': IC, 'me.com': IC, 'mac.com': IC,
    'aol.com': AOL, 't-online.de': TO, 'magenta.de': TO,
    'mailbox.org': MB, 'posteo.de': PO, 'posteo.net': PO,
    'zoho.com': ZO, 'zoho.eu': ZOEU, 'zohomail.eu': ZOEU, 'fastmail.com': FM,
  }
})()

const PRESET_PROVIDER_KEYS: Record<Exclude<ProviderPresetId, 'auto' | 'imap'>, string> = {
  gmail: 'gmail.com',
  outlook: 'outlook.com',
  yahoo: 'yahoo.com',
  webde: 'web.de',
  gmxde: 'gmx.de',
  gmxnet: 'gmx.net',
}

export const PROVIDER_PRESET_OPTIONS: ProviderPresetOption[] = [
  { id: 'auto', label: 'Auto' },
  { id: 'gmail', label: 'Gmail / Google Workspace' },
  { id: 'outlook', label: 'Outlook / Microsoft 365' },
  { id: 'yahoo', label: 'Yahoo' },
  { id: 'webde', label: 'WEB.DE' },
  { id: 'gmxde', label: 'GMX.de' },
  { id: 'gmxnet', label: 'GMX.net' },
  { id: 'imap', label: 'Regular IMAP' },
]

export const ACCT_COLORS = ['#2563EB', '#DC2626', '#EA580C', '#D97706', '#16A34A', '#0D9488', '#0EA5E9', '#7C3AED', '#DB2777', '#475569']

export const SETTINGS_EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/

export function deriveInitials(name: string, email: string): string {
  const src = (name || email || '?').trim()
  const parts = src.split(/[\s.@_-]+/).filter(Boolean)
  if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase()
  return src.slice(0, 2).toUpperCase()
}

export function discoverServers(email: string, isExchange = false): DiscoverResult {
  const domain = (email.split('@')[1] || '').toLowerCase()
  const sub = (s: string) => (s || '').replace(/\{email\}/g, email).replace(/\{domain\}/g, domain)
  const fromProv = (p: ProviderDef, source: DiscoverSource): DiscoverResult => ({
    imapHost: p.imapHost, imapPort: p.imapPort, imapSecurity: p.imapSecurity, imapUser: email,
    smtpHost: p.smtpHost, smtpPort: p.smtpPort, smtpSecurity: p.smtpSecurity, smtpUser: email,
    carddavUrl: sub(p.carddavUrl), caldavUrl: sub(p.caldavUrl), source, provider: p.name, oauth: !!p.oauth,
  })
  if (SRV_PROVIDERS[domain]) return fromProv(SRV_PROVIDERS[domain], 'provider')
  if (isExchange) return fromProv(SRV_PROVIDERS['outlook.com'], 'exchange')
  return {
    imapHost: 'imap.' + domain, imapPort: 993, imapSecurity: 'ssl', imapUser: email,
    smtpHost: 'smtp.' + domain, smtpPort: 587, smtpSecurity: 'starttls', smtpUser: email,
    carddavUrl: 'https://dav.' + domain + '/carddav/', caldavUrl: 'https://dav.' + domain + '/caldav/',
    source: 'autoconfig', provider: domain, oauth: false,
  }
}

export function discoverServersForProvider(email: string, preset: ProviderPresetId): DiscoverResult {
  if (preset === 'auto') return discoverServers(email)
  if (preset === 'imap') {
    const domain = (email.split('@')[1] || '').toLowerCase()
    return {
      imapHost: 'imap.' + domain, imapPort: 993, imapSecurity: 'ssl', imapUser: email,
      smtpHost: 'smtp.' + domain, smtpPort: 587, smtpSecurity: 'starttls', smtpUser: email,
      carddavUrl: 'https://dav.' + domain + '/carddav/', caldavUrl: 'https://dav.' + domain + '/caldav/',
      source: 'provider', provider: 'IMAP', oauth: false,
    }
  }
  const provider = SRV_PROVIDERS[PRESET_PROVIDER_KEYS[preset]]
  const domain = (email.split('@')[1] || '').toLowerCase()
  const sub = (s: string) => (s || '').replace(/\{email\}/g, email).replace(/\{domain\}/g, domain)
  return {
    imapHost: provider.imapHost, imapPort: provider.imapPort, imapSecurity: provider.imapSecurity, imapUser: email,
    smtpHost: provider.smtpHost, smtpPort: provider.smtpPort, smtpSecurity: provider.smtpSecurity, smtpUser: email,
    carddavUrl: sub(provider.carddavUrl), caldavUrl: sub(provider.caldavUrl),
    source: 'provider', provider: provider.name, oauth: !!provider.oauth,
  }
}

interface RemoteEndpoint {
  host: string
  port: number
  security: Security
  source: 'srv' | 'ispdb' | 'mx' | 'probe'
}

interface RemoteDiscoverResponse {
  domain: string
  provider: string | null
  imap: RemoteEndpoint | null
  smtp: RemoteEndpoint | null
}

// Full discovery: static provider table first, then the backend DNS lookup
// (RFC 6186 SRV records, MX-guided reachability probe). Falls back to the
// static autoconfig guess when the backend finds nothing or is unreachable.
export async function discoverServersAsync(email: string, isExchange = false): Promise<DiscoverResult> {
  const local = discoverServers(email, isExchange)
  if (local.source !== 'autoconfig') return local
  try {
    const d = await apiGet<RemoteDiscoverResponse>(`/discover?email=${encodeURIComponent(email)}`)
    if (d.imap && d.smtp) {
      const providerName = d.provider || local.provider
      const isGoogleWorkspace = /google|gmail/i.test(providerName)
      const isMicrosoft365 = /microsoft|outlook|office 365/i.test(providerName)
      return {
        ...local,
        imapHost: d.imap.host, imapPort: d.imap.port, imapSecurity: d.imap.security,
        smtpHost: d.smtp.host, smtpPort: d.smtp.port, smtpSecurity: d.smtp.security,
        source: 'dns',
        provider: providerName,
        oauth: isGoogleWorkspace || isMicrosoft365 || local.oauth,
      }
    }
  } catch {
    // Backend unreachable or lookup failed — keep the static guess.
  }
  return local
}

export function serverDefaults(email: string, isExchange = false): ServerConfig {
  const d = discoverServers(email || 'name@example.com', isExchange)
  return {
    imapHost: d.imapHost, imapPort: d.imapPort, imapSecurity: d.imapSecurity, imapUser: d.imapUser,
    smtpHost: d.smtpHost, smtpPort: d.smtpPort, smtpSecurity: d.smtpSecurity, smtpUser: d.smtpUser,
    carddavUrl: d.carddavUrl, caldavUrl: d.caldavUrl,
  }
}

export interface ProviderSummary {
  name: string
  oauth: boolean
  known: boolean
  source: DiscoverSource
}

export function providerInfo(email: string, isExchange = false): ProviderSummary {
  const d = discoverServers(email || 'name@example.com', isExchange)
  return { name: d.source === 'autoconfig' ? 'IMAP' : d.provider, oauth: d.oauth, known: d.source !== 'autoconfig', source: d.source }
}
