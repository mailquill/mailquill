export const APP_VERSION = '1.6.0'

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
    version: '1.6.0',
    date: '2026-06-11',
    entries: [
      { type: 'new', text: 'Phishing-Erkennung mit Warnbanner und Schild-Symbol in der Liste.' },
      { type: 'new', text: 'Gmail- und Outlook-Konten über die Anbieter-API (Sync und Versand).' },
      { type: 'new', text: 'Externe Bilder werden blockiert, bis sie freigegeben oder der Absender erlaubt wird.' },
      { type: 'new', text: 'Aktualisierungsstatus-Popup mit Gesamt- und Pro-Konto-Fortschritt neben dem Benachrichtigungs-Icon.' },
      { type: 'improved', text: 'Konto-Button an die übrigen Topbar-Buttons angeglichen.' },
      { type: 'improved', text: 'Aktualisieren in einen einzigen Knopf in der Topbar zusammengeführt.' },
      { type: 'fixed', text: 'Eingebettete Bilder (cid:) werden im Verlauf korrekt angezeigt.' },
    ],
  },
  {
    version: '1.5.2',
    date: '2026-05-27',
    entries: [
      { type: 'fixed', text: 'Dunkelmodus: besserer Kontrast bei Badges und Fortschrittsbalken.' },
      { type: 'fixed', text: 'Seitenleisten-Breite wird nach Neustart korrekt gemerkt.' },
      { type: 'fixed', text: 'Ungelesen-Zähler aktualisiert sich sofort nach dem Lesen.' },
    ],
  },
  {
    version: '1.5.0',
    date: '2026-05-12',
    entries: [
      { type: 'new', text: 'Benachrichtigungszentrale mit Tabs „Alle/Ungelesen".' },
      { type: 'new', text: 'Speicherübersicht pro Konto im Profilmenü.' },
      { type: 'new', text: 'Seitenleiste und Nachrichtenliste in der Breite anpassbar.' },
      { type: 'improved', text: 'Schnellere Suche über alle Postfächer.' },
      { type: 'improved', text: 'Sehr lange Nachrichtenlisten werden virtualisiert und laden flüssig nach.' },
    ],
  },
  {
    version: '1.4.0',
    date: '2026-04-20',
    entries: [
      { type: 'new', text: 'Kalender und Kontakte in die Seitenleiste integriert.' },
      { type: 'new', text: 'Filterregeln für eingehende E-Mails (Sieve).' },
      { type: 'improved', text: 'Links in E-Mails öffnen immer in einem neuen Tab.' },
    ],
  },
  {
    version: '1.3.0',
    date: '2026-03-18',
    entries: [
      { type: 'new', text: 'Vereinheitlichter Posteingang über alle Konten.' },
      { type: 'new', text: 'Desktop-Benachrichtigungen per Web Push.' },
      { type: 'improved', text: 'Nachrichten-Threads zusammengefasst dargestellt.' },
    ],
  },
]
