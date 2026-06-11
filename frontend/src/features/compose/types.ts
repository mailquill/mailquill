import type { Message } from '@/shared/types'

export type ComposeMode = 'new' | 'reply' | 'forward'

export interface ComposeInitialState {
  mode: ComposeMode
  sourceMessage?: Message
  to?: string[]
}
