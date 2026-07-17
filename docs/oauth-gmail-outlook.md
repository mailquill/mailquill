# Gmail and Outlook OAuth Setup

Mailquill uses a server-side OAuth authorization-code flow with PKCE. The
frontend starts the flow at `/api/auth/oauth/:provider/start`, the backend
exchanges the authorization code, encrypts the provider tokens, and refreshes
access tokens when sync or send needs them.

Use this guide when enabling Gmail or Outlook sign-in for a deployed Mailquill
instance.

## Required Mailquill Configuration

Set the public backend URL first. It must be the URL users can reach in their
browser, without a trailing slash:

```sh
APP_BASE_URL=https://mail.example.com
```

For local development, the repo-local `.env` uses:

```sh
SERVER_PORT=8765
APP_BASE_URL=http://localhost:8765
```

The exact redirect URIs derived from `APP_BASE_URL` are:

```text
https://mail.example.com/api/auth/oauth/google/callback
https://mail.example.com/api/auth/oauth/microsoft/callback
```

For local development:

```text
http://localhost:8765/api/auth/oauth/google/callback
http://localhost:8765/api/auth/oauth/microsoft/callback
```

Do not set `APP_BASE_URL` to the frontend-only dev server unless that server
proxies `/api` to the backend. For the default local setup, the backend listens
on `8765`, so Google/Microsoft must redirect to `localhost:8765`, not to a UI
port such as `8081` or `5173`.

Configure provider credentials through environment variables or the equivalent
`mailquill.toml` / `mailquill.yaml` keys:

```sh
GOOGLE_OAUTH_CLIENT_ID=...
GOOGLE_OAUTH_CLIENT_SECRET=...
MICROSOFT_OAUTH_CLIENT_ID=...
MICROSOFT_OAUTH_CLIENT_SECRET=...
```

Do not put client secrets in frontend code, committed config, or screenshots.

## Provider Registration URLs

- Google OAuth clients:
  <https://console.cloud.google.com/auth/clients>
- Google API Library, used to enable the Gmail and Calendar APIs:
  <https://console.cloud.google.com/apis/library>
- Microsoft Entra app registrations:
  <https://entra.microsoft.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade>

## Gmail

1. Open Google Cloud Console and select or create a project:
   <https://console.cloud.google.com/>
2. Enable the Gmail API, Google Calendar API, and People API for that project:
   <https://console.cloud.google.com/apis/library/gmail.googleapis.com>
   <https://console.cloud.google.com/apis/library/calendar-json.googleapis.com>
   <https://console.cloud.google.com/apis/library/people.googleapis.com>

   The OAuth client and the Gmail API must be in the same Google Cloud project.
   Mailquill syncs Gmail mailboxes through `gmail.googleapis.com`; OAuth consent
   alone is not enough. If you know the numeric project ID from an error log,
   you can open the activation page directly:

   ```text
   https://console.cloud.google.com/apis/library/gmail.googleapis.com?project=PROJECT_ID
   ```

   After clicking **Enable**, wait a few minutes before retrying the sync.
3. Configure the Google Auth Platform consent screen:
   <https://console.cloud.google.com/auth/overview>
4. Add the Gmail and Calendar scopes used by Mailquill:

   ```text
   https://mail.google.com/
   https://www.googleapis.com/auth/calendar.events
   https://www.googleapis.com/auth/calendar.calendarlist.readonly
   https://www.googleapis.com/auth/contacts
   ```

   Mailquill currently requests this restricted Gmail scope because it needs
   read, write, send, label/move, and delete capability for mailbox operations.
   Google may require app verification and, for production use with restricted
   Gmail data, a security assessment.

   The Calendar scopes let Mailquill list the calendars connected to the Google
   account and read, create, update, and delete their events. Existing Gmail
   accounts must reconnect once to grant newly added scopes. The contacts scope
   grants two-way synchronization of the user's own Google contacts; Mailquill
   does not request Workspace directory access.

5. Create an OAuth client:
   <https://console.cloud.google.com/auth/clients>

   - Application type: `Web application`
   - Authorized redirect URI:

     ```text
     {APP_BASE_URL}/api/auth/oauth/google/callback
     ```

6. Copy the generated client ID and client secret into:

   ```sh
   GOOGLE_OAUTH_CLIENT_ID=...
   GOOGLE_OAUTH_CLIENT_SECRET=...
   ```

7. Restart the backend.
8. In Mailquill, add a Gmail account. The provider discovery should mark Gmail
   as OAuth-only and show the Google sign-in action.

## Outlook / Microsoft 365

1. Open Microsoft Entra admin center:
   <https://entra.microsoft.com/>
2. Go to **Identity > Applications > App registrations** and create a new
   registration:
   <https://entra.microsoft.com/#view/Microsoft_AAD_RegisteredApps/ApplicationsListBlade>
3. Choose the supported account type:

   - Single tenant for one organization only.
   - Multitenant if multiple Microsoft Entra tenants should connect.
   - Include personal Microsoft accounts if Outlook.com, Hotmail, or Live users
     should connect.

4. Configure a platform:

   - Platform: `Web`
   - Redirect URI:

     ```text
     {APP_BASE_URL}/api/auth/oauth/microsoft/callback
     ```

5. Create a client secret under **Certificates & secrets**.
6. Add delegated Microsoft Graph API permissions that match Mailquill's current
   OAuth request:

   ```text
   Mail.ReadWrite
   Mail.Send
   Contacts.ReadWrite
   User.Read
   offline_access
   email
   ```

   `offline_access` is required so Mailquill can receive refresh tokens and
   continue syncing after the short-lived access token expires. `User.Read`
   lets Mailquill read the signed-in user's mailbox address during the OAuth
   callback so the account is created with the real email address.
   `Contacts.ReadWrite` is delegated access to the signed-in user's contacts;
   it does not grant organization-directory synchronization.

7. If your tenant policy requires it, grant admin consent for the application.
8. Copy the application client ID and client secret into:

   ```sh
   MICROSOFT_OAUTH_CLIENT_ID=...
   MICROSOFT_OAUTH_CLIENT_SECRET=...
   ```

9. Restart the backend.
10. In Mailquill, add an Outlook account. The provider discovery should mark
    Outlook/Microsoft as OAuth-only and show the Microsoft sign-in action.

## Runtime Flow

1. User clicks the provider sign-in button.
2. Frontend redirects to:

   ```text
   /api/auth/oauth/google/start?token=...
   /api/auth/oauth/microsoft/start?token=...
   ```

3. Backend validates the Mailquill JWT, generates PKCE state, and redirects to
   the provider.
4. Provider redirects back to:

   ```text
   /api/auth/oauth/{provider}/callback
   ```

5. Backend exchanges the code, encrypts the access and refresh tokens, creates
   the mail account, and redirects the user to:

   ```text
   {APP_BASE_URL}/mail/accounts?connected={account_id}
   ```

## Production Notes

- Use HTTPS for production `APP_BASE_URL`. Provider consoles generally allow
  plain HTTP only for localhost development.
- Create separate OAuth apps for development and production so local redirect
  URIs are not left enabled on the production client.
- Rotate client secrets if they may have been exposed.
- Keep `CREDENTIAL_ENCRYPTION_KEY` stable. Existing encrypted OAuth tokens
  cannot be decrypted if that key changes.
- Google testing-mode apps can require periodic re-consent. Move the app to a
  production publishing state once the consent screen and verification path are
  ready.

## Troubleshooting

### Redirect URI mismatch

The redirect URI in the provider console must exactly match Mailquill's computed
URI, including scheme, host, port, path, and absence of a trailing slash.

Check:

```sh
echo "$APP_BASE_URL/api/auth/oauth/google/callback"
echo "$APP_BASE_URL/api/auth/oauth/microsoft/callback"
```

If the browser lands on `localhost:8081/api/auth/oauth/google/callback` and
shows `404 page not found`, `APP_BASE_URL` points at the wrong local server.
Set it to the backend URL, usually `http://localhost:8765`, register the matching
provider callback URL, and restart the backend.

### OAuth button does not appear

Provider discovery only shows OAuth for known domains such as `gmail.com`,
`googlemail.com`, `outlook.com`, `hotmail.com`, `live.com`, and Microsoft
Exchange discovery results. Confirm the account email domain and the discovered
provider in the add-account form.

### Token refresh fails

Check that the matching client ID and secret are present on the backend and that
the OAuth app was not deleted, disabled, or rotated without updating Mailquill.
For Microsoft, confirm `offline_access` was requested and granted.

### Gmail consent or verification warning

`https://mail.google.com/` is a restricted Gmail scope. For private testing, add
test users to the Google consent screen. For broader production use, follow
Google's verification requirements for restricted Gmail scopes.

### Gmail API returns 403 disabled

If sync fails with `Gmail API has not been used in project ... before or it is
disabled`, the OAuth token is valid but the Gmail API is not enabled for that
Google Cloud project.

Open the Gmail API Library page, select the same project that owns
`GOOGLE_OAUTH_CLIENT_ID`, and click **Enable**:

```text
https://console.cloud.google.com/apis/library/gmail.googleapis.com
```

When the error includes a numeric project ID, use:

```text
https://console.cloud.google.com/apis/library/gmail.googleapis.com?project=PROJECT_ID
```

Restart Mailquill or wait for the next sync attempt after Google finishes
enabling the API.

### Microsoft consent denied

Some organizations block user consent. Ask a tenant administrator to grant
admin consent for the delegated permissions listed above, or use an app
registration in a tenant where users are allowed to consent.

## References

- Google OAuth client setup and redirect URI rules:
  <https://support.google.com/cloud/answer/15549257>
- Gmail API scope reference:
  <https://developers.google.com/workspace/gmail/api/auth/scopes>
- Google Calendar API scope reference:
  <https://developers.google.com/workspace/calendar/api/auth>
- Microsoft app registration and platform settings:
  <https://learn.microsoft.com/en-us/graph/auth-register-app-v2>
- Microsoft identity platform scopes:
  <https://learn.microsoft.com/en-us/entra/identity-platform/scopes-oidc>
- Microsoft Graph permissions reference:
  <https://learn.microsoft.com/en-us/graph/permissions-reference>
