import type { Message } from '@/shared/types'

/** Why the compose dialog was opened; decides how the source message is quoted and which recipients are pre-filled. */
export type ComposeMode = 'new' | 'reply' | 'forward' | 'draft'

/**
 * Seed for a new compose session.
 * @property mode - What kind of message is being written.
 * @property sourceMessage - Message being replied to, forwarded or resumed as a draft; unset for a blank message.
 * @property to - Pre-filled recipient addresses, e.g. from a contact or `mailto:` action.
 */
export interface ComposeInitialState {
  mode: ComposeMode
  sourceMessage?: Message
  to?: string[]
}
