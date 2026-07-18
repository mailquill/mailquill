import { expect, type Page, type Route } from '@playwright/test'

const MAILBOX = {
  id: 'mailbox-1',
  display_name: 'Work mailbox',
  primary_email: 'work@example.test',
  imap_host: 'imap.example.test',
  imap_port: 993,
  imap_auth_scheme: 'plain',
  smtp_host: 'smtp.example.test',
  smtp_port: 587,
  smtp_auth_scheme: 'plain',
  body_sync_mode: 'lazy',
  sync_interval_secs: 300,
  sync_mode: 'idle',
  provider_kind: 'imap',
  created_at: '2026-07-18T00:00:00Z',
  carddav_url: 'https://dav.example.test/addressbooks/',
  caldav_url: null,
  caldav_accept_invalid_tls: false,
  pgp_key_id: null,
  sign_by_default: false,
  contacts: {
    source_id: 'source-1',
    provider: 'cardav',
    state: 'disabled',
    reason: null,
    enabled: false,
    last_synced_at: null,
    cache_retained: true,
  },
}

/** Browser page object for mailbox-owned contact setup and recovery journeys. */
export class ContactIntegrationPage {
  readonly page: Page
  enableRequests = 0

  /** @param page - Playwright page controlled by the current test. */
  constructor(page: Page) {
    this.page = page
  }

  /**
   * Install an authenticated session and deterministic API responses.
   * @returns A promise resolved once browser routes are ready.
   */
  async prepare(): Promise<void> {
    await this.page.addInitScript(() => {
      localStorage.setItem('mailquill-auth', JSON.stringify({
        state: { accessToken: 'e2e-access-token', userId: 'user-1', email: 'user@example.test' },
        version: 0,
      }))
    })
    await this.page.route(
      (url) => url.pathname === '/api' || url.pathname.startsWith('/api/'),
      (route) => this.respond(route),
    )
  }

  /** @returns A promise resolved when the empty Contacts workspace is visible. */
  async openEmptyContacts(): Promise<void> {
    await this.page.goto('/mail/contacts')
    await expect(this.page.getByText('Work mailbox', { exact: true })).toBeVisible()
    await expect(this.page.getByRole('button', { name: 'Enable contacts' })).toBeVisible()
  }

  /** @returns A promise resolved after mailbox-scoped contact enablement is requested. */
  async enableContacts(): Promise<void> {
    await this.page.getByRole('button', { name: 'Enable contacts' }).click()
    await expect.poll(() => this.enableRequests).toBe(1)
  }

  /** @returns A promise resolved when OAuth return context and restored focus are visible. */
  async expectOAuthReturnContext(): Promise<void> {
    await this.page.goto('/mail/settings?section=accounts&connected=mailbox-1&contacts=connected')
    await expect(this.page.getByText('Contact permission was updated. The current status is shown below.')).toBeVisible()
    await expect(this.page.locator('#contact-capability-mailbox-1')).toBeFocused()
  }

  private async respond(route: Route): Promise<void> {
    const request = route.request()
    const url = new URL(request.url())
    const path = url.pathname.replace(/^\/api/, '')
    if (path === '/accounts/mailbox-1/contacts/enable' && request.method() === 'POST') {
      this.enableRequests += 1
      await route.fulfill({ json: { ...MAILBOX.contacts, state: 'pending', enabled: true } })
      return
    }
    if (path === '/accounts') return this.json(route, [MAILBOX])
    if (path === '/contact-accounts') return this.json(route, [])
    if (path === '/contacts') return this.json(route, { items: [], total: 0, next_cursor: null })
    if (path === '/contact-groups') return this.json(route, [])
    if (path === '/mailbox/unified/counts') {
      return this.json(route, { inbox: 0, starred: 0, sent: 0, drafts: 0, archive: 0, spam: 0, trash: 0 })
    }
    if (path.endsWith('/folders')) return this.json(route, [])
    if (path.endsWith('/sync-status')) {
      return this.json(route, { account_id: 'mailbox-1', state: 'idle', synced: 0, total: 0, last_synced_at: null })
    }
    if (path === '/events') {
      await route.fulfill({ status: 200, contentType: 'text/event-stream', body: '' })
      return
    }
    await this.json(route, [])
  }

  private async json(route: Route, body: unknown): Promise<void> {
    await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) })
  }
}
