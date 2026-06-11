// Best-effort CardDAV/CalDAV collection URLs for known providers, falling back
// to RFC 6764 well-known URIs derived from the email domain.

interface DavUrls {
  carddav_url: string
  caldav_url: string
}

const PROVIDERS: Record<string, (email: string) => DavUrls> = {
  'gmail.com': (email) => ({
    carddav_url: `https://www.googleapis.com/carddav/v1/principals/${email}/lists/default/`,
    caldav_url: `https://apidata.googleusercontent.com/caldav/v2/${email}/user`,
  }),
  'googlemail.com': (email) => PROVIDERS['gmail.com'](email),
  'icloud.com': () => ({
    carddav_url: 'https://contacts.icloud.com/',
    caldav_url: 'https://caldav.icloud.com/',
  }),
  'me.com': () => PROVIDERS['icloud.com'](''),
  'fastmail.com': () => ({
    carddav_url: 'https://carddav.fastmail.com/dav/addressbooks',
    caldav_url: 'https://caldav.fastmail.com/dav/calendars',
  }),
}

export function davDefaults(email: string, imapHost?: string): DavUrls {
  const domain = (email.split('@')[1] ?? '').toLowerCase()
  if (PROVIDERS[domain]) return PROVIDERS[domain](email)

  // Outlook/Office365 expose no CardDAV/CalDAV — leave blank for manual entry.
  if (/office365|outlook|hotmail|live\.com/.test(`${domain} ${imapHost ?? ''}`)) {
    return { carddav_url: '', caldav_url: '' }
  }

  const host = domain || 'localhost'
  return {
    carddav_url: `https://${host}/.well-known/carddav`,
    caldav_url: `https://${host}/.well-known/caldav`,
  }
}
