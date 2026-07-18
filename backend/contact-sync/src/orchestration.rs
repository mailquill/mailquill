use std::{sync::Arc, time::Duration};

use sqlx::SqlitePool;

use crate::{
    adapters::{ContactProviderAdapter, MutationResult, PROVIDER_API_DISABLED_MESSAGE},
    repository::{
        apply_change_page, book_cursor, finish_book_discovery, finish_full_sync, set_source_state,
        source_generation,
    },
    ContactBookIdentity, ContactChangePage, ContactProvider, ContactTombstone, ParsedContact,
    ProviderError, ProviderErrorCategory, RemoteContact,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    Full,
    Incremental,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncRunResult {
    pub mode: SyncMode,
    pub pages: usize,
    pub upserts: usize,
    pub tombstones: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub base: Duration,
    pub maximum: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            base: Duration::from_secs(5),
            maximum: Duration::from_secs(15 * 60),
        }
    }
}

impl RetryPolicy {
    pub fn delay(&self, consecutive_failures: u32, retry_after_seconds: Option<u64>) -> Duration {
        if let Some(seconds) = retry_after_seconds {
            return Duration::from_secs(seconds).min(self.maximum);
        }
        let exponent = consecutive_failures.saturating_sub(1).min(16);
        self.base
            .saturating_mul(2u32.saturating_pow(exponent))
            .min(self.maximum)
    }
}

pub async fn sync_source_once(
    db: &SqlitePool,
    source_id: &str,
    adapter: Arc<dyn ContactProviderAdapter>,
) -> Result<SyncRunResult, ProviderError> {
    set_source_state(db, source_id, "syncing", None)
        .await
        .map_err(repository_error)?;
    let result = sync_source_attempt(db, source_id, adapter.clone(), true).await;
    let result = match result {
        Err(error) if error.category == ProviderErrorCategory::CursorExpired => {
            tracing::info!(
                source_id,
                operation = "contact_full_sync_fallback",
                "contact cursor expired"
            );
            sync_source_attempt(db, source_id, adapter, false).await
        }
        result => result,
    };
    match &result {
        Ok(run) => {
            set_source_state(db, source_id, "idle", None)
                .await
                .map_err(repository_error)?;
            tracing::info!(
                source_id,
                operation = "contact_sync",
                pages = run.pages,
                upserts = run.upserts,
                tombstones = run.tombstones,
                mode = ?run.mode,
                "contact sync completed"
            );
        }
        Err(error) => {
            let (state, reason) = error_state(error);
            set_source_state(db, source_id, state, Some(reason))
                .await
                .map_err(repository_error)?;
            tracing::warn!(
                source_id,
                operation = "contact_sync",
                error_category = ?error.category,
                error_detail = %error,
                "contact sync failed"
            );
        }
    }
    result
}

pub async fn create_remote_first(
    db: &SqlitePool,
    source_id: &str,
    adapter: Arc<dyn ContactProviderAdapter>,
    book: &ContactBookIdentity,
    contact: &ParsedContact,
) -> Result<MutationResult, ProviderError> {
    let mutation = adapter.create(book, contact).await?;
    let remote = remote_contact(book, contact, &mutation);
    persist_mutation_or_reconcile(
        db,
        source_id,
        adapter,
        book,
        ContactChangePage {
            upserts: vec![remote],
            ..Default::default()
        },
    )
    .await?;
    Ok(mutation)
}

pub async fn update_remote_first(
    db: &SqlitePool,
    source_id: &str,
    adapter: Arc<dyn ContactProviderAdapter>,
    book: &ContactBookIdentity,
    remote_id: &str,
    remote_version: Option<&str>,
    contact: &ParsedContact,
) -> Result<MutationResult, ProviderError> {
    let mutation = adapter
        .update(book, remote_id, remote_version, contact)
        .await?;
    let remote = remote_contact(book, contact, &mutation);
    persist_mutation_or_reconcile(
        db,
        source_id,
        adapter,
        book,
        ContactChangePage {
            upserts: vec![remote],
            ..Default::default()
        },
    )
    .await?;
    Ok(mutation)
}

pub async fn delete_remote_first(
    db: &SqlitePool,
    source_id: &str,
    adapter: Arc<dyn ContactProviderAdapter>,
    book: &ContactBookIdentity,
    remote_id: &str,
    remote_version: Option<&str>,
) -> Result<(), ProviderError> {
    adapter.delete(book, remote_id, remote_version).await?;
    persist_mutation_or_reconcile(
        db,
        source_id,
        adapter,
        book,
        ContactChangePage {
            tombstones: vec![ContactTombstone {
                book_remote_id: book.remote_id.clone(),
                remote_id: remote_id.to_owned(),
            }],
            ..Default::default()
        },
    )
    .await
}

fn remote_contact(
    book: &ContactBookIdentity,
    contact: &ParsedContact,
    mutation: &MutationResult,
) -> RemoteContact {
    RemoteContact {
        book_remote_id: book.remote_id.clone(),
        remote_id: mutation.remote_id.clone(),
        remote_version: mutation.remote_version.clone(),
        contact: contact.clone(),
        photo: None,
        groups: Vec::new(),
        provider_metadata: serde_json::Value::Null,
    }
}

async fn persist_mutation_or_reconcile(
    db: &SqlitePool,
    source_id: &str,
    adapter: Arc<dyn ContactProviderAdapter>,
    book: &ContactBookIdentity,
    page: ContactChangePage,
) -> Result<(), ProviderError> {
    let generation = source_generation(db, source_id)
        .await
        .map_err(repository_error)?;
    if apply_change_page(db, source_id, book, generation, &page)
        .await
        .is_ok()
    {
        return Ok(());
    }
    tracing::warn!(
        source_id,
        operation = "contact_mutation_reconcile",
        "remote contact mutation succeeded but local persistence failed"
    );
    sync_source_once(db, source_id, adapter)
        .await
        .map(|_| ())
        .map_err(|_| ProviderError {
            category: ProviderErrorCategory::Unavailable,
            message: "remote contact changed but local reconciliation failed".to_owned(),
            retry_after_seconds: None,
        })
}

async fn sync_source_attempt(
    db: &SqlitePool,
    source_id: &str,
    adapter: Arc<dyn ContactProviderAdapter>,
    allow_incremental: bool,
) -> Result<SyncRunResult, ProviderError> {
    let mut books = adapter.books().await?;
    let (provider_metadata, provider): (String, String) =
        sqlx::query_as("SELECT provider_metadata, type FROM contact_accounts WHERE id = ?")
            .bind(source_id)
            .fetch_one(db)
            .await
            .map_err(repository_error)?;
    if provider != ContactProvider::Google.as_str() {
        if let Some(selected) = serde_json::from_str::<serde_json::Value>(&provider_metadata)
            .ok()
            .and_then(|value| value.get("selected_book_remote_ids").cloned())
            .and_then(|value| serde_json::from_value::<Vec<String>>(value).ok())
        {
            books.retain(|book| selected.contains(&book.remote_id));
        }
    }
    let generation = source_generation(db, source_id)
        .await
        .map_err(repository_error)?
        + 1;
    let mut cursors = Vec::with_capacity(books.len());
    for book in &books {
        cursors.push(
            book_cursor(db, source_id, &book.remote_id)
                .await
                .map_err(repository_error)?,
        );
    }
    let full_sync = !allow_incremental
        || cursors.iter().any(Option::is_none)
        || books.iter().any(|book| {
            book.provider_metadata["supports_sync_collection"].as_bool() == Some(false)
        });
    let mode = if full_sync {
        SyncMode::Full
    } else {
        SyncMode::Incremental
    };
    let mut result = SyncRunResult {
        mode,
        pages: 0,
        upserts: 0,
        tombstones: 0,
    };

    for (book, stored_cursor) in books.iter().zip(cursors.iter()) {
        sync_book(
            db,
            source_id,
            adapter.as_ref(),
            book,
            if full_sync {
                None
            } else {
                stored_cursor.as_deref()
            },
            generation,
            &mut result,
        )
        .await?;
    }
    if full_sync {
        finish_full_sync(db, source_id, generation)
            .await
            .map_err(repository_error)?;
    } else {
        finish_book_discovery(db, source_id, generation)
            .await
            .map_err(repository_error)?;
    }
    Ok(result)
}

async fn sync_book(
    db: &SqlitePool,
    source_id: &str,
    adapter: &dyn ContactProviderAdapter,
    book: &ContactBookIdentity,
    cursor: Option<&str>,
    generation: i64,
    result: &mut SyncRunResult,
) -> Result<(), ProviderError> {
    let mut continuation: Option<String> = None;
    loop {
        let page = adapter
            .changes(book, cursor, continuation.as_deref())
            .await?;
        if page.continuation.is_none() && page.final_cursor.is_none() {
            return Err(ProviderError {
                category: ProviderErrorCategory::InvalidResponse,
                message: "final contact page did not contain a durable cursor".to_owned(),
                retry_after_seconds: None,
            });
        }
        result.pages += 1;
        result.upserts += page.upserts.len();
        result.tombstones += page.tombstones.len();
        continuation.clone_from(&page.continuation);
        apply_change_page(db, source_id, book, generation, &page)
            .await
            .map_err(repository_error)?;
        if continuation.is_none() {
            break;
        }
    }
    Ok(())
}

fn error_state(error: &ProviderError) -> (&'static str, &'static str) {
    match error.category {
        ProviderErrorCategory::ConsentRequired => ("consent_required", "contact_scope_missing"),
        ProviderErrorCategory::ReauthenticationRequired | ProviderErrorCategory::Authentication => {
            ("reauth_required", "oauth_reauthentication_required")
        }
        ProviderErrorCategory::Unavailable if error.message == PROVIDER_API_DISABLED_MESSAGE => {
            ("unavailable", "provider_configuration_required")
        }
        ProviderErrorCategory::Unavailable => ("unavailable", "provider_unavailable"),
        _ => ("error", provider_reason(error.category)),
    }
}

fn provider_reason(category: ProviderErrorCategory) -> &'static str {
    match category {
        ProviderErrorCategory::CursorExpired => "cursor_expired",
        ProviderErrorCategory::Conflict => "remote_conflict",
        ProviderErrorCategory::RateLimited => "rate_limited",
        ProviderErrorCategory::Transport => "transport_error",
        ProviderErrorCategory::InvalidResponse => "invalid_provider_response",
        ProviderErrorCategory::ConsentRequired => "contact_scope_missing",
        ProviderErrorCategory::ReauthenticationRequired | ProviderErrorCategory::Authentication => {
            "oauth_reauthentication_required"
        }
        ProviderErrorCategory::Unavailable => "provider_unavailable",
    }
}

fn repository_error(_error: sqlx::Error) -> ProviderError {
    ProviderError {
        category: ProviderErrorCategory::Unavailable,
        message: "contact cache operation failed".to_owned(),
        retry_after_seconds: None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, VecDeque};

    use async_trait::async_trait;
    use serde_json::Value;
    use sqlx::sqlite::SqlitePoolOptions;
    use tokio::sync::Mutex;

    use super::*;
    use crate::{
        adapters::{MutationResult, PhotoPayload, PROVIDER_API_DISABLED_MESSAGE},
        repository::{reconcile_mailbox_source, MailboxSource},
        ContactChangePage, ContactProvider, ContactTombstone, ParsedContact, RemoteContact,
    };

    #[test]
    fn disabled_provider_api_requires_configuration_instead_of_new_consent() {
        let error = ProviderError {
            category: ProviderErrorCategory::Unavailable,
            message: PROVIDER_API_DISABLED_MESSAGE.to_owned(),
            retry_after_seconds: None,
        };

        assert_eq!(
            error_state(&error),
            ("unavailable", "provider_configuration_required")
        );
    }

    type ChangeCall = (String, Option<String>, Option<String>);

    struct MockAdapter {
        books: Vec<ContactBookIdentity>,
        pages: Mutex<HashMap<String, VecDeque<Result<ContactChangePage, ProviderError>>>>,
        calls: Mutex<Vec<ChangeCall>>,
    }

    impl MockAdapter {
        fn new(
            books: Vec<ContactBookIdentity>,
            pages: HashMap<String, VecDeque<Result<ContactChangePage, ProviderError>>>,
        ) -> Self {
            Self {
                books,
                pages: Mutex::new(pages),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl ContactProviderAdapter for MockAdapter {
        async fn books(&self) -> Result<Vec<ContactBookIdentity>, ProviderError> {
            Ok(self.books.clone())
        }

        async fn changes(
            &self,
            book: &ContactBookIdentity,
            cursor: Option<&str>,
            continuation: Option<&str>,
        ) -> Result<ContactChangePage, ProviderError> {
            self.calls.lock().await.push((
                book.remote_id.clone(),
                cursor.map(str::to_owned),
                continuation.map(str::to_owned),
            ));
            self.pages
                .lock()
                .await
                .get_mut(&book.remote_id)
                .and_then(VecDeque::pop_front)
                .unwrap_or_else(|| {
                    Ok(ContactChangePage {
                        final_cursor: Some("unchanged".into()),
                        ..Default::default()
                    })
                })
        }

        async fn create(
            &self,
            _book: &ContactBookIdentity,
            _contact: &ParsedContact,
        ) -> Result<MutationResult, ProviderError> {
            unreachable!()
        }

        async fn update(
            &self,
            _book: &ContactBookIdentity,
            _remote_id: &str,
            _remote_version: Option<&str>,
            _contact: &ParsedContact,
        ) -> Result<MutationResult, ProviderError> {
            unreachable!()
        }

        async fn delete(
            &self,
            _book: &ContactBookIdentity,
            _remote_id: &str,
            _remote_version: Option<&str>,
        ) -> Result<(), ProviderError> {
            unreachable!()
        }

        async fn photo(
            &self,
            _book: &ContactBookIdentity,
            _remote_id: &str,
            _photo_reference: Option<&str>,
        ) -> Result<Option<PhotoPayload>, ProviderError> {
            unreachable!()
        }

        async fn update_photo(
            &self,
            _book: &ContactBookIdentity,
            _remote_id: &str,
            _remote_version: Option<&str>,
            _photo: &PhotoPayload,
        ) -> Result<MutationResult, ProviderError> {
            unreachable!()
        }
    }

    async fn database() -> (SqlitePool, String) {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db::migrations::run_mail_migrations(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO email_accounts
             (id, display_name, primary_email, imap_host, imap_port, imap_auth_scheme,
              smtp_host, smtp_port, smtp_auth_scheme, credentials_encrypted)
             VALUES ('mailbox', 'Mailbox', 'owner@example.com', 'imap', 993, 'plain',
                     'smtp', 465, 'plain', X'00')",
        )
        .execute(&db)
        .await
        .unwrap();
        let source_id = reconcile_mailbox_source(
            &db,
            &MailboxSource {
                email_account_id: "mailbox".into(),
                display_name: "Contacts".into(),
                provider: ContactProvider::Google,
                base_url: None,
                enabled: true,
                capability_state: "pending".into(),
                capability_reason: None,
            },
        )
        .await
        .unwrap();
        (db, source_id)
    }

    fn book(id: &str) -> ContactBookIdentity {
        ContactBookIdentity {
            remote_id: id.into(),
            display_name: id.into(),
            parent_remote_id: None,
            is_default: id == "default",
            is_writable: true,
            provider_metadata: Value::Null,
        }
    }

    fn contact(id: &str, book: &str) -> RemoteContact {
        RemoteContact {
            book_remote_id: book.into(),
            remote_id: id.into(),
            remote_version: Some("v1".into()),
            contact: ParsedContact {
                uid: id.into(),
                display_name: Some(id.into()),
                ..Default::default()
            },
            photo: None,
            groups: Vec::new(),
            provider_metadata: Value::Null,
        }
    }

    #[tokio::test]
    async fn initial_sync_exhausts_pages_and_books_before_pruning() {
        let (db, source_id) = database().await;
        let adapter = Arc::new(MockAdapter::new(
            vec![book("default"), book("team")],
            HashMap::from([
                (
                    "default".into(),
                    VecDeque::from([
                        Ok(ContactChangePage {
                            upserts: vec![contact("one", "default")],
                            continuation: Some("page-2".into()),
                            ..Default::default()
                        }),
                        Ok(ContactChangePage {
                            upserts: vec![contact("two", "default")],
                            final_cursor: Some("default-cursor".into()),
                            ..Default::default()
                        }),
                    ]),
                ),
                (
                    "team".into(),
                    VecDeque::from([Ok(ContactChangePage {
                        upserts: vec![contact("three", "team")],
                        final_cursor: Some("team-cursor".into()),
                        ..Default::default()
                    })]),
                ),
            ]),
        ));
        let run = sync_source_once(&db, &source_id, adapter).await.unwrap();
        assert_eq!(run.mode, SyncMode::Full);
        assert_eq!(run.pages, 3);
        assert_eq!(run.upserts, 3);
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM contacts WHERE account_id = ?")
            .bind(&source_id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn sync_honors_selected_address_books() {
        let (db, source_id) = database().await;
        sqlx::query(
            "UPDATE contact_accounts SET type = 'graph', provider_metadata = ? WHERE id = ?",
        )
        .bind(serde_json::json!({ "selected_book_remote_ids": ["team"] }).to_string())
        .bind(&source_id)
        .execute(&db)
        .await
        .unwrap();
        let adapter = Arc::new(MockAdapter::new(
            vec![book("default"), book("team")],
            HashMap::from([(
                "team".into(),
                VecDeque::from([Ok(ContactChangePage {
                    upserts: vec![contact("selected", "team")],
                    final_cursor: Some("team-cursor".into()),
                    ..Default::default()
                })]),
            )]),
        ));

        sync_source_once(&db, &source_id, adapter.clone())
            .await
            .unwrap();

        let calls = adapter.calls.lock().await.clone();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "team");
        let synced: Vec<String> =
            sqlx::query_scalar("SELECT uid FROM contacts WHERE account_id = ? ORDER BY uid")
                .bind(&source_id)
                .fetch_all(&db)
                .await
                .unwrap();
        assert_eq!(synced, vec!["selected"]);
    }

    #[tokio::test]
    async fn google_ignores_legacy_group_book_selection() {
        let (db, source_id) = database().await;
        sqlx::query("UPDATE contact_accounts SET provider_metadata = ? WHERE id = ?")
            .bind(
                serde_json::json!({
                    "selected_book_remote_ids": ["contactGroups/family"]
                })
                .to_string(),
            )
            .bind(&source_id)
            .execute(&db)
            .await
            .unwrap();
        let adapter = Arc::new(MockAdapter::new(
            vec![book("contactGroups/myContacts")],
            HashMap::from([(
                "contactGroups/myContacts".into(),
                VecDeque::from([Ok(ContactChangePage {
                    upserts: vec![contact("selected", "contactGroups/myContacts")],
                    final_cursor: Some("cursor".into()),
                    ..Default::default()
                })]),
            )]),
        ));

        sync_source_once(&db, &source_id, adapter.clone())
            .await
            .unwrap();

        assert_eq!(adapter.calls.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn failed_page_keeps_cursor_and_replay_is_safe() {
        let (db, source_id) = database().await;
        let initial = Arc::new(MockAdapter::new(
            vec![book("default")],
            HashMap::from([(
                "default".into(),
                VecDeque::from([Ok(ContactChangePage {
                    upserts: vec![contact("one", "default")],
                    final_cursor: Some("cursor-1".into()),
                    ..Default::default()
                })]),
            )]),
        ));
        sync_source_once(&db, &source_id, initial).await.unwrap();

        let failure = ProviderError {
            category: ProviderErrorCategory::Transport,
            message: "temporary".into(),
            retry_after_seconds: None,
        };
        let failing = Arc::new(MockAdapter::new(
            vec![book("default")],
            HashMap::from([(
                "default".into(),
                VecDeque::from([
                    Ok(ContactChangePage {
                        upserts: vec![contact("two", "default")],
                        continuation: Some("page-2".into()),
                        ..Default::default()
                    }),
                    Err(failure),
                ]),
            )]),
        ));
        assert!(sync_source_once(&db, &source_id, failing).await.is_err());
        let state: (String, Option<String>) = sqlx::query_as(
            "SELECT capability_state, capability_reason FROM contact_accounts WHERE id = ?",
        )
        .bind(&source_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(state.0, "error");
        assert_eq!(state.1.as_deref(), Some("transport_error"));
        assert_eq!(
            book_cursor(&db, &source_id, "default").await.unwrap(),
            Some("cursor-1".into())
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM contacts WHERE account_id = ? AND uid = 'two'",
        )
        .bind(&source_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn no_change_delta_advances_only_its_final_cursor() {
        let (db, source_id) = database().await;
        let initial = Arc::new(MockAdapter::new(
            vec![book("default")],
            HashMap::from([(
                "default".into(),
                VecDeque::from([Ok(ContactChangePage {
                    upserts: vec![contact("one", "default")],
                    final_cursor: Some("cursor-1".into()),
                    ..Default::default()
                })]),
            )]),
        ));
        sync_source_once(&db, &source_id, initial).await.unwrap();
        let delta = Arc::new(MockAdapter::new(
            vec![book("default")],
            HashMap::from([(
                "default".into(),
                VecDeque::from([Ok(ContactChangePage {
                    final_cursor: Some("cursor-2".into()),
                    ..Default::default()
                })]),
            )]),
        ));
        let run = sync_source_once(&db, &source_id, delta).await.unwrap();
        assert_eq!(run.mode, SyncMode::Incremental);
        assert_eq!(run.upserts, 0);
        assert_eq!(run.tombstones, 0);
        assert_eq!(
            book_cursor(&db, &source_id, "default").await.unwrap(),
            Some("cursor-2".into())
        );
    }

    #[tokio::test]
    async fn expired_cursor_falls_back_to_full_sync_and_applies_tombstones() {
        let (db, source_id) = database().await;
        let initial = Arc::new(MockAdapter::new(
            vec![book("default")],
            HashMap::from([(
                "default".into(),
                VecDeque::from([Ok(ContactChangePage {
                    upserts: vec![contact("old", "default")],
                    final_cursor: Some("cursor-1".into()),
                    ..Default::default()
                })]),
            )]),
        ));
        sync_source_once(&db, &source_id, initial).await.unwrap();
        let expired = ProviderError {
            category: ProviderErrorCategory::CursorExpired,
            message: "expired".into(),
            retry_after_seconds: None,
        };
        let adapter = Arc::new(MockAdapter::new(
            vec![book("default")],
            HashMap::from([(
                "default".into(),
                VecDeque::from([
                    Err(expired),
                    Ok(ContactChangePage {
                        upserts: vec![contact("new", "default")],
                        tombstones: vec![ContactTombstone {
                            book_remote_id: "default".into(),
                            remote_id: "old".into(),
                        }],
                        final_cursor: Some("cursor-2".into()),
                        ..Default::default()
                    }),
                ]),
            )]),
        ));
        let run = sync_source_once(&db, &source_id, adapter).await.unwrap();
        assert_eq!(run.mode, SyncMode::Full);
        let ids: Vec<String> = sqlx::query_scalar("SELECT uid FROM contacts WHERE account_id = ?")
            .bind(&source_id)
            .fetch_all(&db)
            .await
            .unwrap();
        assert_eq!(ids, vec!["new"]);
    }

    #[test]
    fn retry_policy_honors_retry_after_and_bounds_exponential_delay() {
        let policy = RetryPolicy {
            base: Duration::from_secs(2),
            maximum: Duration::from_secs(30),
        };
        assert_eq!(policy.delay(1, None), Duration::from_secs(2));
        assert_eq!(policy.delay(5, None), Duration::from_secs(30));
        assert_eq!(policy.delay(2, Some(17)), Duration::from_secs(17));
        assert_eq!(policy.delay(2, Some(60)), Duration::from_secs(30));
    }
}
