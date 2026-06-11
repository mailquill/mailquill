# Tauri Integration Path

Mailquill is structured so a future Tauri shell can link the Rust crates without depending on Axum or HTTP types in `core`.

## Linking `core`

Add the backend crates as path dependencies from the Tauri Rust workspace:

```toml
[dependencies]
mailquill-core = { path = "../backend/core" }
db = { path = "../backend/db" }
mail-sync = { path = "../backend/mail-sync" }
smtp = { path = "../backend/smtp" }
```

Keep `api` out of the Tauri command layer unless the desktop app intentionally embeds the HTTP server.

## Command Mapping

Map each Tauri command to the same service boundary used by the API:

- `auth_register`, `auth_login`, `auth_refresh`, `auth_logout`
- `accounts_list`, `accounts_create`, `accounts_update`, `accounts_delete`
- `folders_list`, `mailbox_unified`, `folder_messages`
- `message_get`, `message_mark_read`, `message_flag`, `message_archive`, `message_delete`
- `thread_get`, `thread_archive`, `thread_delete`, `thread_mark_read`
- `send_message`, `search_messages`, `settings_get`, `settings_patch`

Command payloads should reuse the JSON DTO shape exposed by the HTTP API so the frontend transport can switch between HTTP and Tauri IPC without feature code changes.

## Transport Abstraction

The frontend already selects `frontend/src/transport/tauri.ts` when `window.__TAURI__` exists. Implement that file by delegating to Tauri `invoke` calls with the same method names as the HTTP transport:

- `get<T>(path)`
- `post<T>(path, body)`
- `put<T>(path, body)`
- `patch<T>(path, body)`
- `delete<T>(path)`

Keep all server-state access behind TanStack Query hooks. Do not call Tauri IPC directly from pages or widgets.

## State and Storage

Use the same per-user database layout as the API crate. The desktop wrapper should set an app-specific `DATA_DIR`, initialize `CredentialKey` and `JwtKey`, and construct shared sync/blob services once at startup.
