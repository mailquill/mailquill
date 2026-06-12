/** Injected from package.json at build time (see vite.config.ts). */
export const APP_VERSION: string = __APP_VERSION__

export type ChangeType = 'new' | 'improved' | 'fixed'

export interface ChangelogEntry {
  type: ChangeType
  text: string
}

export interface ChangelogRelease {
  version: string
  /** ISO date — formatted for display in the active locale. */
  date: string
  entries: ChangelogEntry[]
}

/** Newest first. `version === APP_VERSION` is marked as the current release. */
export const CHANGELOG: ChangelogRelease[] = [
  {
    version: '1.0.0',
    date: '2026-06-11',
    entries: [
      { type: 'new', text: 'Vereinheitlichter Posteingang über alle Konten mit zusammengefassten Threads.' },
      { type: 'new', text: 'Gmail- und Outlook-Konten über die Anbieter-API (Sync und Versand).' },
      { type: 'new', text: 'Phishing-Erkennung mit Warnbanner und Schild-Symbol in der Liste.' },
      { type: 'new', text: 'Externe Bilder werden blockiert, bis sie freigegeben oder der Absender erlaubt wird.' },
      { type: 'new', text: 'Kalender und Kontakte über CalDAV/CardDAV in der Seitenleiste.' },
      { type: 'new', text: 'Filterregeln für eingehende E-Mails (Sieve).' },
      { type: 'new', text: 'Benachrichtigungszentrale und Desktop-Benachrichtigungen per Web Push.' },
      { type: 'new', text: 'Seitenleiste und Nachrichtenliste in der Breite anpassbar.' },
    ],
  },
]
