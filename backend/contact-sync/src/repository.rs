use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::{
    ContactBookIdentity, ContactChangePage, ContactProvider, ContactTombstone, RemoteContact,
};

#[derive(Debug, Clone)]
pub struct MailboxSource {
    pub email_account_id: String,
    pub display_name: String,
    pub provider: ContactProvider,
    pub base_url: Option<String>,
    pub enabled: bool,
    pub capability_state: String,
    pub capability_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacySourceIdentity {
    pub source_id: String,
    pub provider: ContactProvider,
    pub account_identity: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxIdentity {
    pub email_account_id: String,
    pub provider: ContactProvider,
    pub account_identity: String,
    pub carddav_url: Option<String>,
}

pub fn unambiguous_mailbox_match(
    legacy: &LegacySourceIdentity,
    mailboxes: &[MailboxIdentity],
) -> Option<String> {
    let candidates = mailboxes
        .iter()
        .filter(|mailbox| mailbox.provider == legacy.provider)
        .filter(|mailbox| match legacy.provider {
            ContactProvider::CardDav => normalized_url(legacy.base_url.as_deref())
                .zip(normalized_url(mailbox.carddav_url.as_deref()))
                .is_some_and(|(source, account)| source == account),
            ContactProvider::Google | ContactProvider::Graph => legacy
                .account_identity
                .as_deref()
                .is_some_and(|identity| identity.eq_ignore_ascii_case(&mailbox.account_identity)),
        })
        .map(|mailbox| mailbox.email_account_id.clone())
        .collect::<Vec<_>>();

    (candidates.len() == 1).then(|| candidates[0].clone())
}

fn normalized_url(value: Option<&str>) -> Option<String> {
    value.map(|url| url.trim().trim_end_matches('/').to_ascii_lowercase())
}

pub async fn link_legacy_source(
    db: &SqlitePool,
    source_id: &str,
    email_account_id: &str,
) -> Result<bool, sqlx::Error> {
    let mut tx = db.begin().await?;
    let occupied: Option<String> = sqlx::query_scalar(
        "SELECT id FROM contact_accounts WHERE email_account_id = ? AND id <> ?",
    )
    .bind(email_account_id)
    .bind(source_id)
    .fetch_optional(&mut *tx)
    .await?;
    if occupied.is_some() {
        tx.rollback().await?;
        return Ok(false);
    }

    let affected = sqlx::query(
        "UPDATE contact_accounts
         SET email_account_id = ?, management_mode = 'mailbox'
         WHERE id = ? AND (email_account_id IS NULL OR email_account_id = ?)",
    )
    .bind(email_account_id)
    .bind(source_id)
    .bind(email_account_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok(affected == 1)
}

pub async fn reconcile_mailbox_source(
    db: &SqlitePool,
    source: &MailboxSource,
) -> Result<String, sqlx::Error> {
    let mut tx = db.begin().await?;
    let existing: Option<String> =
        sqlx::query_scalar("SELECT id FROM contact_accounts WHERE email_account_id = ?")
            .bind(&source.email_account_id)
            .fetch_optional(&mut *tx)
            .await?;
    let source_id = existing.unwrap_or_else(|| Uuid::new_v4().simple().to_string());

    sqlx::query(
        "INSERT INTO contact_accounts
         (id, display_name, type, base_url, auth_scheme, credentials_encrypted,
          sync_status, email_account_id, management_mode, capability_state,
          capability_reason, enabled)
         VALUES (?, ?, ?, ?, ?, X'', 'idle', ?, 'mailbox', ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
           display_name = excluded.display_name,
           type = excluded.type,
           base_url = excluded.base_url,
           auth_scheme = excluded.auth_scheme,
           management_mode = 'mailbox',
           capability_state = excluded.capability_state,
           capability_reason = excluded.capability_reason,
           enabled = excluded.enabled",
    )
    .bind(&source_id)
    .bind(&source.display_name)
    .bind(source.provider.as_str())
    .bind(&source.base_url)
    .bind(match source.provider {
        ContactProvider::CardDav => "basic",
        ContactProvider::Google | ContactProvider::Graph => "oauth2",
    })
    .bind(&source.email_account_id)
    .bind(&source.capability_state)
    .bind(&source.capability_reason)
    .bind(source.enabled)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(source_id)
}

pub async fn apply_change_page(
    db: &SqlitePool,
    source_id: &str,
    book: &ContactBookIdentity,
    generation: i64,
    page: &ContactChangePage,
) -> Result<(), sqlx::Error> {
    if page.continuation.is_some() && page.final_cursor.is_some() {
        return Err(sqlx::Error::Protocol(
            "a contact page cannot contain both continuation and final cursor".into(),
        ));
    }

    let mut tx = db.begin().await?;
    let book_id = upsert_book(&mut tx, source_id, book, generation).await?;
    upsert_provider_groups(&mut tx, source_id, &book_id, generation, book).await?;
    for contact in &page.upserts {
        if contact.book_remote_id != book.remote_id {
            return Err(sqlx::Error::Protocol(
                "contact page contained a contact from another book".into(),
            ));
        }
        upsert_contact(&mut tx, source_id, &book_id, generation, contact).await?;
    }
    for tombstone in &page.tombstones {
        delete_contact(&mut tx, source_id, &book_id, tombstone).await?;
    }
    if let Some(cursor) = &page.final_cursor {
        sqlx::query(
            "UPDATE contact_books SET sync_cursor = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(cursor)
        .bind(&book_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

async fn upsert_book(
    tx: &mut Transaction<'_, Sqlite>,
    source_id: &str,
    book: &ContactBookIdentity,
    generation: i64,
) -> Result<String, sqlx::Error> {
    let book_id = Uuid::new_v4().simple().to_string();
    sqlx::query(
        "INSERT INTO contact_books
         (id, account_id, remote_id, display_name, parent_remote_id, is_default,
          is_writable, sync_generation, provider_metadata)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(account_id, remote_id) DO UPDATE SET
           display_name = excluded.display_name,
           parent_remote_id = excluded.parent_remote_id,
           is_default = excluded.is_default,
           is_writable = excluded.is_writable,
           sync_generation = excluded.sync_generation,
           provider_metadata = excluded.provider_metadata,
           updated_at = datetime('now')",
    )
    .bind(&book_id)
    .bind(source_id)
    .bind(&book.remote_id)
    .bind(&book.display_name)
    .bind(&book.parent_remote_id)
    .bind(book.is_default)
    .bind(book.is_writable)
    .bind(generation)
    .bind(book.provider_metadata.to_string())
    .execute(&mut **tx)
    .await?;

    sqlx::query_scalar("SELECT id FROM contact_books WHERE account_id = ? AND remote_id = ?")
        .bind(source_id)
        .bind(&book.remote_id)
        .fetch_one(&mut **tx)
        .await
}

async fn upsert_contact(
    tx: &mut Transaction<'_, Sqlite>,
    source_id: &str,
    book_id: &str,
    generation: i64,
    remote: &RemoteContact,
) -> Result<(), sqlx::Error> {
    let contact = &remote.contact;
    let photo_reference = remote.photo.as_ref().map(|photo| photo.reference.as_str());
    let photo_version = remote
        .photo
        .as_ref()
        .and_then(|photo| photo.version.as_deref());
    sqlx::query(
        "INSERT INTO contacts
         (id, account_id, book_id, uid, display_name, given_name, family_name,
          org, title, emails, phones, addresses, notes, raw_vcard, synced_at,
          remote_version, provider_metadata, photo_reference, photo_version, photo_content_type,
          sync_generation)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), ?, ?, ?, ?, ?, ?)
         ON CONFLICT(account_id, uid) DO UPDATE SET
           book_id = excluded.book_id,
           display_name = excluded.display_name,
           given_name = excluded.given_name,
           family_name = excluded.family_name,
           org = excluded.org,
           title = excluded.title,
           emails = excluded.emails,
           phones = excluded.phones,
           addresses = excluded.addresses,
           notes = excluded.notes,
           raw_vcard = excluded.raw_vcard,
           synced_at = excluded.synced_at,
           remote_version = excluded.remote_version,
           provider_metadata = excluded.provider_metadata,
           photo_reference = excluded.photo_reference,
           photo_blob_key = CASE
             WHEN contacts.photo_version IS excluded.photo_version THEN contacts.photo_blob_key
             ELSE NULL
           END,
           photo_version = excluded.photo_version,
           photo_content_type = excluded.photo_content_type,
           sync_generation = excluded.sync_generation",
    )
    .bind(Uuid::new_v4().simple().to_string())
    .bind(source_id)
    .bind(book_id)
    .bind(&remote.remote_id)
    .bind(&contact.display_name)
    .bind(&contact.given_name)
    .bind(&contact.family_name)
    .bind(&contact.org)
    .bind(&contact.title)
    .bind(json(&contact.emails)?)
    .bind(json(&contact.phones)?)
    .bind(json(&contact.addresses)?)
    .bind(&contact.notes)
    .bind(&contact.raw_vcard)
    .bind(&remote.remote_version)
    .bind(remote.provider_metadata.to_string())
    .bind(photo_reference)
    .bind(photo_version)
    .bind(
        remote
            .photo
            .as_ref()
            .and_then(|photo| photo.content_type.as_deref()),
    )
    .bind(generation)
    .execute(&mut **tx)
    .await?;

    let contact_id: String =
        sqlx::query_scalar("SELECT id FROM contacts WHERE account_id = ? AND uid = ?")
            .bind(source_id)
            .bind(&remote.remote_id)
            .fetch_one(&mut **tx)
            .await?;
    sqlx::query("DELETE FROM contact_group_members WHERE contact_id = ?")
        .bind(&contact_id)
        .execute(&mut **tx)
        .await?;
    for membership in &remote.groups {
        let group_id = Uuid::new_v4().simple().to_string();
        sqlx::query(
            "INSERT INTO contact_groups
             (id, account_id, book_id, name, remote_id, sync_generation)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(account_id, remote_id) WHERE remote_id IS NOT NULL DO UPDATE SET
               book_id = excluded.book_id,
               name = COALESCE(?, contact_groups.name),
               sync_generation = excluded.sync_generation",
        )
        .bind(&group_id)
        .bind(source_id)
        .bind(book_id)
        .bind(
            membership
                .display_name
                .as_deref()
                .unwrap_or(&membership.remote_group_id),
        )
        .bind(&membership.remote_group_id)
        .bind(generation)
        .bind(membership.display_name.as_deref())
        .execute(&mut **tx)
        .await?;
        let stored_group_id: String = sqlx::query_scalar(
            "SELECT id FROM contact_groups WHERE account_id = ? AND remote_id = ?",
        )
        .bind(source_id)
        .bind(&membership.remote_group_id)
        .fetch_one(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT OR IGNORE INTO contact_group_members (contact_id, group_id) VALUES (?, ?)",
        )
        .bind(&contact_id)
        .bind(stored_group_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn upsert_provider_groups(
    tx: &mut Transaction<'_, Sqlite>,
    source_id: &str,
    book_id: &str,
    generation: i64,
    book: &ContactBookIdentity,
) -> Result<(), sqlx::Error> {
    for group in book.provider_metadata["contact_groups"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let Some(remote_id) = group["resourceName"].as_str() else {
            continue;
        };
        let name = group["formattedName"]
            .as_str()
            .or_else(|| group["name"].as_str())
            .unwrap_or(remote_id);
        sqlx::query(
            "INSERT INTO contact_groups
             (id, account_id, book_id, name, remote_id, sync_generation)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(account_id, remote_id) WHERE remote_id IS NOT NULL DO UPDATE SET
               book_id = excluded.book_id,
               name = excluded.name,
               sync_generation = excluded.sync_generation",
        )
        .bind(Uuid::new_v4().simple().to_string())
        .bind(source_id)
        .bind(book_id)
        .bind(name)
        .bind(remote_id)
        .bind(generation)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn json<T: serde::Serialize>(value: &T) -> Result<String, sqlx::Error> {
    serde_json::to_string(value).map_err(|error| sqlx::Error::Encode(Box::new(error)))
}

async fn delete_contact(
    tx: &mut Transaction<'_, Sqlite>,
    source_id: &str,
    book_id: &str,
    tombstone: &ContactTombstone,
) -> Result<(), sqlx::Error> {
    if tombstone.book_remote_id.is_empty() {
        return Err(sqlx::Error::Protocol(
            "contact tombstone did not identify a book".into(),
        ));
    }
    sqlx::query("DELETE FROM contacts WHERE account_id = ? AND book_id = ? AND uid = ?")
        .bind(source_id)
        .bind(book_id)
        .bind(&tombstone.remote_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn finish_full_sync(
    db: &SqlitePool,
    source_id: &str,
    generation: i64,
) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM contacts WHERE account_id = ? AND sync_generation <> ?")
        .bind(source_id)
        .bind(generation)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM contact_groups WHERE account_id = ? AND sync_generation <> ?")
        .bind(source_id)
        .bind(generation)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM contact_books WHERE account_id = ? AND sync_generation <> ?")
        .bind(source_id)
        .bind(generation)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE contact_accounts
         SET sync_generation = ?, capability_state = 'idle', last_synced_at = datetime('now')
         WHERE id = ?",
    )
    .bind(generation)
    .bind(source_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

pub async fn source_generation(db: &SqlitePool, source_id: &str) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT sync_generation FROM contact_accounts WHERE id = ?")
        .bind(source_id)
        .fetch_one(db)
        .await
}

pub async fn set_source_state(
    db: &SqlitePool,
    source_id: &str,
    state: &str,
    reason: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE contact_accounts
         SET capability_state = ?, capability_reason = ?,
             sync_status = CASE WHEN ? = 'syncing' THEN 'syncing'
                                WHEN ? = 'error' THEN 'error'
                                ELSE 'idle' END,
             sync_error = CASE WHEN ? = 'error' THEN ? ELSE NULL END
         WHERE id = ?",
    )
    .bind(state)
    .bind(reason)
    .bind(state)
    .bind(state)
    .bind(state)
    .bind(reason)
    .bind(source_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn finish_book_discovery(
    db: &SqlitePool,
    source_id: &str,
    generation: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM contact_books WHERE account_id = ? AND sync_generation <> ?")
        .bind(source_id)
        .bind(generation)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn book_cursor(
    db: &SqlitePool,
    source_id: &str,
    remote_book_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query("SELECT sync_cursor FROM contact_books WHERE account_id = ? AND remote_id = ?")
        .bind(source_id)
        .bind(remote_book_id)
        .fetch_optional(db)
        .await
        .map(|row| {
            row.and_then(|row| row.try_get("sync_cursor").ok())
                .flatten()
        })
}

pub async fn eligible_source_ids(db: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM contact_accounts
         WHERE enabled = 1
           AND capability_state NOT IN ('disabled', 'consent_required', 'reauth_required', 'unavailable')
         ORDER BY id",
    )
    .fetch_all(db)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContactChangePage, ParsedContact, RemoteContact};
    use serde_json::Value;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn database() -> SqlitePool {
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
             VALUES ('mailbox', 'Mailbox', 'owner@example.com', 'imap.example.com', 993,
                     'plain', 'smtp.example.com', 465, 'plain', X'00')",
        )
        .execute(&db)
        .await
        .unwrap();
        db
    }

    fn source() -> MailboxSource {
        MailboxSource {
            email_account_id: "mailbox".into(),
            display_name: "Mailbox contacts".into(),
            provider: ContactProvider::Google,
            base_url: None,
            enabled: true,
            capability_state: "pending".into(),
            capability_reason: None,
        }
    }

    fn book(remote_id: &str) -> ContactBookIdentity {
        ContactBookIdentity {
            remote_id: remote_id.into(),
            display_name: "Contacts".into(),
            parent_remote_id: None,
            is_default: true,
            is_writable: true,
            provider_metadata: Value::Null,
        }
    }

    fn contact(remote_id: &str, book_remote_id: &str) -> RemoteContact {
        RemoteContact {
            book_remote_id: book_remote_id.into(),
            remote_id: remote_id.into(),
            remote_version: Some("v1".into()),
            contact: ParsedContact {
                uid: remote_id.into(),
                display_name: Some(remote_id.into()),
                ..Default::default()
            },
            photo: None,
            groups: Vec::new(),
            provider_metadata: Value::Null,
        }
    }

    #[tokio::test]
    async fn reconciliation_is_idempotent() {
        let db = database().await;
        let first = reconcile_mailbox_source(&db, &source()).await.unwrap();
        let second = reconcile_mailbox_source(&db, &source()).await.unwrap();
        assert_eq!(first, second);
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM contact_accounts WHERE email_account_id = 'mailbox'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn legacy_link_is_idempotent_and_preserves_cached_data() {
        let db = database().await;
        sqlx::query(
            "INSERT INTO contact_accounts
             (id, display_name, type, auth_scheme, credentials_encrypted, sync_token)
             VALUES ('legacy', 'Legacy', 'google', 'oauth2', X'01', 'legacy-cursor')",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO contacts (id, account_id, uid, display_name)
             VALUES ('cached', 'legacy', 'people/1', 'Cached contact')",
        )
        .execute(&db)
        .await
        .unwrap();

        assert!(link_legacy_source(&db, "legacy", "mailbox").await.unwrap());
        assert!(link_legacy_source(&db, "legacy", "mailbox").await.unwrap());
        let retained: (String, String, i64) = sqlx::query_as(
            "SELECT sync_token,
                    (SELECT display_name FROM contacts WHERE account_id = contact_accounts.id),
                    (SELECT count(*) FROM contact_accounts WHERE email_account_id = 'mailbox')
             FROM contact_accounts WHERE id = 'legacy'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(
            retained,
            ("legacy-cursor".into(), "Cached contact".into(), 1)
        );
    }

    #[test]
    fn legacy_matching_requires_exactly_one_mailbox() {
        let legacy = LegacySourceIdentity {
            source_id: "legacy".into(),
            provider: ContactProvider::Google,
            account_identity: Some("owner@example.com".into()),
            base_url: None,
        };
        let mailbox = MailboxIdentity {
            email_account_id: "mailbox".into(),
            provider: ContactProvider::Google,
            account_identity: "OWNER@example.com".into(),
            carddav_url: None,
        };
        assert_eq!(
            unambiguous_mailbox_match(&legacy, std::slice::from_ref(&mailbox)),
            Some("mailbox".into())
        );
        assert_eq!(
            unambiguous_mailbox_match(&legacy, &[mailbox.clone(), mailbox]),
            None
        );
    }

    #[tokio::test]
    async fn pages_apply_upserts_deletes_and_only_final_cursor() {
        let db = database().await;
        let source_id = reconcile_mailbox_source(&db, &source()).await.unwrap();
        let initial = ContactChangePage {
            upserts: vec![contact("one", "default"), contact("two", "default")],
            continuation: Some("page-2".into()),
            ..Default::default()
        };
        apply_change_page(&db, &source_id, &book("default"), 1, &initial)
            .await
            .unwrap();
        assert_eq!(book_cursor(&db, &source_id, "default").await.unwrap(), None);

        let final_page = ContactChangePage {
            upserts: vec![contact("three", "default")],
            tombstones: vec![ContactTombstone {
                book_remote_id: "default".into(),
                remote_id: "one".into(),
            }],
            final_cursor: Some("cursor-1".into()),
            ..Default::default()
        };
        apply_change_page(&db, &source_id, &book("default"), 1, &final_page)
            .await
            .unwrap();
        let ids: Vec<String> =
            sqlx::query_scalar("SELECT uid FROM contacts WHERE account_id = ? ORDER BY uid")
                .bind(&source_id)
                .fetch_all(&db)
                .await
                .unwrap();
        assert_eq!(ids, vec!["three", "two"]);
        assert_eq!(
            book_cursor(&db, &source_id, "default").await.unwrap(),
            Some("cursor-1".into())
        );

        let other_book = ContactChangePage {
            upserts: vec![contact("other", "team")],
            final_cursor: Some("team-cursor".into()),
            ..Default::default()
        };
        apply_change_page(&db, &source_id, &book("team"), 1, &other_book)
            .await
            .unwrap();
        assert_eq!(
            book_cursor(&db, &source_id, "default").await.unwrap(),
            Some("cursor-1".into())
        );
        assert_eq!(
            book_cursor(&db, &source_id, "team").await.unwrap(),
            Some("team-cursor".into())
        );
    }

    #[tokio::test]
    async fn provider_group_metadata_resolves_membership_names() {
        let db = database().await;
        let source_id = reconcile_mailbox_source(&db, &source()).await.unwrap();
        let mut google_book = book("contactGroups/myContacts");
        google_book.provider_metadata = serde_json::json!({
            "contact_groups": [{
                "resourceName": "contactGroups/family",
                "name": "family",
                "formattedName": "Family",
                "groupType": "SYSTEM_CONTACT_GROUP"
            }]
        });
        let mut family = contact("one", "contactGroups/myContacts");
        family.groups = vec![crate::GroupMembership {
            remote_group_id: "contactGroups/family".into(),
            display_name: None,
        }];

        apply_change_page(
            &db,
            &source_id,
            &google_book,
            1,
            &ContactChangePage {
                upserts: vec![family],
                final_cursor: Some("cursor".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let name: String = sqlx::query_scalar(
            "SELECT name FROM contact_groups WHERE account_id = ? AND remote_id = ?",
        )
        .bind(&source_id)
        .bind("contactGroups/family")
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(name, "Family");
    }

    #[tokio::test]
    async fn invalid_page_rolls_back_book_and_cursor_changes() {
        let db = database().await;
        let source_id = reconcile_mailbox_source(&db, &source()).await.unwrap();
        let invalid = ContactChangePage {
            upserts: vec![contact("wrong-book", "other")],
            final_cursor: Some("must-not-commit".into()),
            ..Default::default()
        };
        assert!(
            apply_change_page(&db, &source_id, &book("default"), 1, &invalid)
                .await
                .is_err()
        );
        let books: i64 =
            sqlx::query_scalar("SELECT count(*) FROM contact_books WHERE account_id = ?")
                .bind(&source_id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(books, 0);
    }

    #[tokio::test]
    async fn full_sync_prunes_only_after_finish() {
        let db = database().await;
        let source_id = reconcile_mailbox_source(&db, &source()).await.unwrap();
        let first = ContactChangePage {
            upserts: vec![contact("old", "default")],
            final_cursor: Some("cursor-1".into()),
            ..Default::default()
        };
        apply_change_page(&db, &source_id, &book("default"), 1, &first)
            .await
            .unwrap();
        let second = ContactChangePage {
            upserts: vec![contact("new", "default")],
            final_cursor: Some("cursor-2".into()),
            ..Default::default()
        };
        apply_change_page(&db, &source_id, &book("default"), 2, &second)
            .await
            .unwrap();
        let before: i64 = sqlx::query_scalar("SELECT count(*) FROM contacts WHERE account_id = ?")
            .bind(&source_id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(before, 2);
        finish_full_sync(&db, &source_id, 2).await.unwrap();
        let ids: Vec<String> = sqlx::query_scalar("SELECT uid FROM contacts WHERE account_id = ?")
            .bind(&source_id)
            .fetch_all(&db)
            .await
            .unwrap();
        assert_eq!(ids, vec!["new"]);
    }

    #[tokio::test]
    async fn startup_resume_selects_only_eligible_sources() {
        let db = database().await;
        let active = reconcile_mailbox_source(&db, &source()).await.unwrap();
        sqlx::query(
            "INSERT INTO contact_accounts
             (id, display_name, type, credentials_encrypted, enabled, capability_state)
             VALUES ('disabled', 'Disabled', 'cardav', X'00', 0, 'disabled'),
                    ('consent', 'Consent', 'google', X'00', 1, 'consent_required')",
        )
        .execute(&db)
        .await
        .unwrap();
        assert_eq!(eligible_source_ids(&db).await.unwrap(), vec![active]);
    }
}
