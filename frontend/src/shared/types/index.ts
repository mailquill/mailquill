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
  created_at: string
  carddav_url?: string | null
  caldav_url?: string | null
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
}

export interface AllowedImageSender {
  sender: string
  created_at: string
}

export interface User {
  user_id: string
  email: string
}

export interface Contact {
  id: string
  account_id: string | null
  display_name: string
  email: string | null
  phone: string | null
  company: string | null
  job_title: string | null
  notes: string | null
  favorite: boolean
  group_name: string | null
}

export interface NewContact {
  account_id?: string | null
  display_name: string
  email?: string | null
  phone?: string | null
  company?: string | null
  job_title?: string | null
  notes?: string | null
  favorite?: boolean
  group_name?: string | null
}

export interface Calendar {
  id: string
  account_id: string | null
  name: string
  color: string
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
}

export interface NewCalendarEvent {
  calendar_id: string
  title: string
  description?: string | null
  location?: string | null
  starts_at: string
  ends_at: string
  all_day?: boolean
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
  in_reply_to?: string | null
  references?: string | null
  attachments?: AttachmentInput[]
}
