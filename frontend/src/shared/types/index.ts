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
  created_at: string
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
  full_path: string
  folder_type: string
  unread_count: number
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
}

export interface Settings {
  pgp_discovery_wkd_enabled: boolean
  pgp_discovery_keyserver_enabled: boolean
}

export interface User {
  user_id: string
  email: string
}

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
