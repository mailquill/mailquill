import AxeBuilder from '@axe-core/playwright'
import { expect, test } from '@playwright/test'
import { ContactIntegrationPage } from '../pages/ContactIntegrationPage'

test.describe('mailbox contact integration', () => {
  test('enables contacts from the empty workspace without accessibility violations', async ({ page }) => {
    const contacts = new ContactIntegrationPage(page)
    await contacts.prepare()
    await contacts.openEmptyContacts()

    const accessibility = await new AxeBuilder({ page }).include('[aria-label="Contacts"]').analyze()
    expect(accessibility.violations).toEqual([])
    await contacts.enableContacts()
  })

  test('restores the mailbox contact context after OAuth re-consent', async ({ page }) => {
    const contacts = new ContactIntegrationPage(page)
    await contacts.prepare()
    await contacts.expectOAuthReturnContext()
  })
})
