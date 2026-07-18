use mail_sync::provider::{connect, IdMap, ProviderConfig, ProviderError, ProviderKind};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

#[test]
fn provider_kind_roundtrip() {
    for kind in [
        ProviderKind::Imap,
        ProviderKind::GmailImap,
        ProviderKind::GmailApi,
        ProviderKind::OutlookApi,
    ] {
        assert_eq!(ProviderKind::parse(kind.as_str()), kind);
    }
    // Unknown values fall back to IMAP (existing accounts have no kind column).
    assert_eq!(ProviderKind::parse("anything"), ProviderKind::Imap);
}

#[test]
fn gmail_hybrid_uses_imap_transport() {
    assert!(ProviderKind::GmailImap.syncs_over_imap());
    assert!(ProviderKind::GmailImap.sends_over_smtp());
    assert!(!ProviderKind::GmailApi.syncs_over_imap());
}

#[tokio::test]
async fn api_providers_require_oauth_token() {
    for kind in [ProviderKind::GmailApi, ProviderKind::OutlookApi] {
        let err = connect(kind, &ProviderConfig::default())
            .await
            .err()
            .unwrap();
        assert!(matches!(err, ProviderError::Other(_)), "{err}");
    }
}

#[tokio::test]
async fn api_providers_require_db_handle() {
    let config = ProviderConfig {
        oauth_access_token: Some("token".into()),
        ..Default::default()
    };
    for kind in [ProviderKind::GmailApi, ProviderKind::OutlookApi] {
        let err = connect(kind, &config).await.err().unwrap();
        assert!(err.to_string().contains("user db"), "{err}");
    }
}

async fn test_db() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE remote_message_ids (
            account_id TEXT NOT NULL, folder_path TEXT NOT NULL,
            uid INTEGER NOT NULL, remote_id TEXT NOT NULL,
            PRIMARY KEY (account_id, folder_path, uid),
            UNIQUE (account_id, folder_path, remote_id))",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

#[tokio::test]
async fn idmap_assigns_ascending_uids_and_resolves_sets() {
    let ids = IdMap::new(test_db().await, "acc1".into());

    assert_eq!(ids.max_uid("INBOX").await.unwrap(), 0);
    let max = ids
        .assign("INBOX", &["a".into(), "b".into(), "c".into()])
        .await
        .unwrap();
    assert_eq!(max, 3);
    // Second batch continues after the previous max.
    let max = ids.assign("INBOX", &["d".into()]).await.unwrap();
    assert_eq!(max, 4);

    // Range, open range, and single-uid sets.
    let set = ids.resolve_set("INBOX", "2:3").await.unwrap();
    assert_eq!(set, vec![(2, "b".into()), (3, "c".into())]);
    let set = ids.resolve_set("INBOX", "3:*").await.unwrap();
    assert_eq!(set, vec![(3, "c".into()), (4, "d".into())]);
    let set = ids.resolve_set("INBOX", "1").await.unwrap();
    assert_eq!(set, vec![(1, "a".into())]);

    // Folders are independent namespaces.
    assert_eq!(ids.max_uid("SENT").await.unwrap(), 0);

    // Removal frees the mapping; resolve skips it.
    ids.remove("INBOX", 2).await.unwrap();
    let set = ids.resolve_set("INBOX", "1:4").await.unwrap();
    assert_eq!(set.len(), 3);
    assert!(ids.remote_id("INBOX", 2).await.is_err());
}
