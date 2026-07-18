export interface Account {
  id: string
  display_name: string
  primary_email: string
  imap_host: string
  imap_port: number
  imap_auth_scheme: string
  smtp_host: string
  smtp_port: number
  smtp_auth_scheme: string
  body_sync_mode: string
  sync_interval_secs: number
  sync_mode: string
  provider_kind: string
  created_at: string
  carddav_url?: string | null
  caldav_url?: string | null
  caldav_accept_invalid_tls?: boolean
  pgp_key_id?: string | null
  sign_by_default: boolean
  contacts: ContactCapability | null
}

export type ContactCapabilityState =
  | 'disabled'
  | 'pending'
  | 'syncing'
  | 'idle'
  | 'consent_required'
  | 'reauth_required'
  | 'error'
  | 'unavailable'

export interface ContactCapability {
  source_id: string
  provider: 'cardav' | 'graph' | 'google'
  state: ContactCapabilityState
  reason: string | null
  enabled: boolean
  last_synced_at: string | null
  cache_retained: boolean
}

export interface AccountAlias {
  id: string
  account_id: string
  email: string
  display_name: string | null
}

export interface Folder {
  id: string
  account_id: string
  name: string
  /** Raw IMAP path (modified UTF-7), the identifier used for routing/commands. */
  full_path: string
  /** Human-readable hierarchy path when provider identifiers are opaque. */
  folder_display_path?: string
  /** Decoded leaf name for display, e.g. "Jülicher" (server-side decoded). */
  folder_name: string
  /** Raw leaf name as the server stores it. */
  folder_name_server: string
  folder_type: string
  unread_count: number
  sync_enabled?: boolean
}

export interface Message {
  id: string
  account_id: string
  folder_id: string
  uid: number
  message_id_header: string | null
  thread_id: string | null
  in_reply_to: string | null
  references: string | null
  list_id: string | null
  subject: string
  from_addr: string
  to_addrs: string
  cc_addrs: string
  snippet: string
  date: string | null
  internal_date: string
  is_read: boolean
  is_flagged: boolean
  is_deleted: boolean
  body_html?: string | null
  body_text?: string | null
  body_available?: boolean
  thread_size?: number
  thread_unread?: number
  thread_participants?: string[]
  folder_type?: string | null
  folder_path?: string | null
  phishing_verdict?: 'clean' | 'suspicious' | 'phishing' | null
  phishing_score?: number | null
  phishing_checks?: PhishingCheck[]
  attachments?: MessageAttachment[]
}

export interface MessageAttachment {
  id: string
  filename: string | null
  content_type: string
  content_id: string | null
  size_bytes: number | null
}

export interface PhishingCheck {
  id: string
  points: number
  /** English fallback; prefer the localized `phishingCheck.<id>` string. */
  detail: string
  /** Interpolation values for the localized message. */
  params?: Record<string, string | number>
}

export interface Thread {
  thread_id: string
  messages: Message[]
  subject: string
  participants: string[]
  unread_count: number
  message_count: number
  last_date: string
  list_id: string | null
}

export interface SyncStatus {
  account_id: string
  state: string
  last_synced_at: string | null
  error: string | null
  synced: number
  total: number
}

export interface Settings {
  pgp_discovery_wkd_enabled: boolean
  pgp_discovery_keyserver_enabled: boolean
  load_external_images: boolean
  default_calendar_id?: string | null
}

export interface PublicConfig {
  remote_image_proxy_enabled: boolean
}

export interface AllowedImageSender {
  sender: string
  created_at: string
}

export interface BrandEntry {
  id: string
  domain: string
  brand_name: string
}

export interface User {
  user_id: string
  email: string
}

export interface Contact {
  id: string
  account_id: string
  uid: string
  display_name: string | null
  given_name: string | null
  family_name: string | null
  org: string | null
  title: string | null
  emails: LabeledValue[]
  phones: LabeledValue[]
  addresses: PostalAddress[]
  notes: string | null
  photo_blob_key: string | null
  raw_vcard: string | null
  synced_at: string | null
  book_id: string | null
  remote_version: string | null
  photo_reference: string | null
  photo_version: string | null
  photo_content_type: string | null
  source_email_account_id: string | null
  source_provider: 'cardav' | 'graph' | 'google'
  source_state: ContactCapabilityState
  source_enabled: boolean
  source_writable: boolean
  groups: ContactGroup[]
}

export interface ContactGroup {
  id: string
  name: string
  remote_id: string | null
}

export interface ContactGroupSummary extends ContactGroup {
  account_id: string
  book_id: string | null
  member_count: number
}

export interface ContactPage {
  items: Contact[]
  total: number
  limit: number
  offset: number
}

export interface RecipientSuggestion {
  id: string
  display_name: string | null
  email: string
  source: 'contact' | 'sender'
}

export interface NewContact {
  account_id: string
  book_id?: string | null
  display_name?: string | null
  given_name?: string | null
  family_name?: string | null
  org?: string | null
  title?: string | null
  emails?: LabeledValue[]
  phones?: LabeledValue[]
  addresses?: PostalAddress[]
  notes?: string | null
}

export interface LabeledValue {
  label: string | null
  value: string
  primary?: boolean
}

export interface PostalAddress {
  label: string | null
  primary?: boolean
  street: string | null
  locality: string | null
  region: string | null
  postal_code: string | null
  country: string | null
}

export interface ContactAccount {
  id: string
  display_name: string
  type: 'cardav' | 'graph' | 'google'
  base_url: string | null
  auth_scheme: string
  sync_token: string | null
  last_synced_at: string | null
  sync_status: string
  sync_error: string | null
  email_account_id: string | null
  management_mode: 'mailbox' | 'independent'
  capability_state: ContactCapabilityState
  capability_reason: string | null
  enabled: boolean
  cache_retained: boolean
}

export interface ContactBook {
  id: string
  account_id: string
  remote_id: string
  display_name: string
  parent_remote_id: string | null
  is_default: boolean
  is_writable: boolean
}

export interface DiscoveredContactBook {
  remote_id: string
  display_name: string
  parent_remote_id: string | null
  is_default: boolean
  is_writable: boolean
}

export interface Calendar {
  id: string
  account_id: string | null
  name: string
  color: string
  is_default?: boolean
  dav_url: string | null
  created_at: string
  provider_type?: CalendarAccount['type'] | null
}

export interface CalendarAccount {
  id: string
  display_name: string
  type: 'caldav' | 'graph' | 'google' | 'openxchange'
  base_url: string | null
  auth_scheme: string
  sync_interval_secs: number
  last_synced_at: string | null
  sync_status: string
  sync_error: string | null
}

export interface CalendarEvent {
  id: string
  calendar_id: string
  title: string
  description: string | null
  location: string | null
  starts_at: string
  ends_at: string
  all_day: boolean
  color: string
  rrule?: string | null
  rrule_uid?: string | null
  recurrence_id?: string | null
  status?: string
  organizer_email?: string | null
  organizer_name?: string | null
  attendees?: string
  ms_busystatus?: string | null
  ms_teams_url?: string | null
  raw_ical?: string | null
}

export interface NewCalendarEvent {
  calendar_id: string
  title: string
  description?: string | null
  location?: string | null
  starts_at: string
  ends_at: string
  all_day?: boolean
  rrule?: string | null
  attendees?: unknown
  organizer_email?: string | null
  organizer_name?: string | null
  recurring_edit_scope?: 'this' | 'following' | 'all'
}

export interface MeetingInvitation {
  id: string
  message_id: string
  method: string
  uid: string
  summary: string | null
  start_dt: string | null
  end_dt: string | null
  organizer_email: string | null
  attendees: string
  user_rsvp_status: string
  raw_ical: string
  ms_teams_url: string | null
}

export type RuleField = 'from' | 'to' | 'subject' | 'body'
export type RuleOp = 'contains' | 'notContains' | 'is'
export type RuleActionType = 'move' | 'markRead' | 'star' | 'delete' | 'forward'
export type RuleEngine = 'sieve' | 'exchange'

export interface RuleCondition {
  field: RuleField
  op: RuleOp
  value: string
}

export interface RuleAction {
  type: RuleActionType
  value?: string
}

export interface Rule {
  id: string
  account_id: string | null
  name: string
  enabled: boolean
  engine: RuleEngine
  match_all: boolean
  conditions: RuleCondition[]
  actions: RuleAction[]
}

export type RuleInput = Omit<Rule, 'id'>

export interface AttachmentInput {
  filename: string
  content_type: string
  data: string
}

export interface SendMessageInput {
  account_id: string
  from: string
  to: string[]
  cc?: string[]
  bcc?: string[]
  subject: string
  body_text?: string
  body_html?: string
  pgp_mime_mode?: 'signed' | 'encrypted'
  pgp_signature?: string
  in_reply_to?: string | null
  references?: string | null
  attachments?: AttachmentInput[]
}
