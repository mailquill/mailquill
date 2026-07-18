import { beforeEach, describe, expect, it } from 'vitest'
import { SIDEBAR_WIDTH, useUiPrefs } from './useUiPrefs'

describe('useUiPrefs sidebar expansion persistence', () => {
  beforeEach(() => {
    localStorage.clear()
    useUiPrefs.setState({
      sidebarWidth: SIDEBAR_WIDTH.default,
      collapsedMailboxIds: [],
      expandedFoldersByMailbox: {},
    })
  })

  it('rehydrates collapsed mailboxes and expanded folder paths', async () => {
    useUiPrefs.getState().toggleMailboxCollapsed('account-a')
    useUiPrefs.getState().toggleFolderExpanded('account-a', 'Projects')
    useUiPrefs.getState().toggleFolderExpanded('account-a', 'Projects/Client')

    const persisted = localStorage.getItem('mailquill-ui')
    expect(persisted).not.toBeNull()

    useUiPrefs.setState({
      collapsedMailboxIds: [],
      expandedFoldersByMailbox: {},
    })
    localStorage.setItem('mailquill-ui', persisted!)
    await useUiPrefs.persist.rehydrate()

    expect(useUiPrefs.getState().collapsedMailboxIds).toEqual(['account-a'])
    expect(useUiPrefs.getState().expandedFoldersByMailbox).toEqual({
      'account-a': ['Projects', 'Projects/Client'],
    })
  })
})
