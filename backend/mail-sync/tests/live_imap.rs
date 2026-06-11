//! Live IMAP smoke test against a real server, exercising the exact sync code
//! path (connect → list → highest_uid → fetch_headers). Skipped unless
//! credentials are supplied via env:
//!
//! ```sh
//! IMAP_HOST=mail.example.com IMAP_USER=you@example.com IMAP_PASS=secret \
//!   cargo test -p mail-sync --test live_imap -- --nocapture
//! ```
//!
//! Optional: IMAP_PORT (default 993), IMAP_FOLDER (default INBOX),
//! IMAP_AUTH (default "plain").

use mail_sync::session;

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

#[tokio::test]
async fn live_fetch_headers() {
    let (Some(host), Some(user), Some(pass)) =
        (env("IMAP_HOST"), env("IMAP_USER"), env("IMAP_PASS"))
    else {
        eprintln!("SKIP live_fetch_headers: set IMAP_HOST/IMAP_USER/IMAP_PASS to run");
        return;
    };
    let port: u16 = env("IMAP_PORT").and_then(|s| s.parse().ok()).unwrap_or(993);
    let folder = env("IMAP_FOLDER").unwrap_or_else(|| "INBOX".to_owned());
    let auth = env("IMAP_AUTH").unwrap_or_else(|| "plain".to_owned());

    eprintln!("connecting to {host}:{port} as {user} (auth={auth})…");
    let mut sess = session::connect_imap(&host, port, &user, &pass, None, &auth, None)
        .await
        .expect("connect_imap failed");
    eprintln!("connected + authenticated OK");

    let folders = session::list_folders(&mut sess)
        .await
        .expect("list_folders failed");
    eprintln!("list_folders: {} folders", folders.len());

    let (uidvalidity, exists) = session::select_folder(&mut sess, &folder)
        .await
        .expect("select_folder failed");
    eprintln!("select {folder}: uidvalidity={uidvalidity} exists={exists}");

    let highest = session::highest_uid(&mut sess)
        .await
        .expect("highest_uid failed");
    eprintln!("highest_uid={highest:?}");

    // Target a range that actually contains messages: UIDs need not start at 1,
    // so probe the top 10 UIDs rather than a fixed low range.
    let top = highest.unwrap_or(0);
    let range = format!("{}:{}", top.saturating_sub(9), top);
    let msgs = session::fetch_headers(&mut sess, &range)
        .await
        .expect("fetch_headers failed");
    eprintln!("fetch_headers({range}): {} messages parsed", msgs.len());
    for m in msgs.iter().take(5) {
        eprintln!(
            "  uid={} from={:?} subject={:?}",
            m.uid, m.from_addr, m.subject
        );
    }

    let _ = sess.logout().await;

    // Regression guard for the parenthesised-FETCH bug: a non-empty mailbox must
    // yield parsed messages. A bare (unparenthesised) item list returns nothing.
    assert!(
        exists == 0 || !msgs.is_empty(),
        "mailbox has {exists} messages but fetch_headers parsed 0"
    );
}
