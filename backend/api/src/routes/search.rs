//! Full-text and metadata search over the user's messages.
//!
//! Two endpoints answer one search. `GET /api/search` reads the local indexes
//! only and returns within milliseconds. `GET /api/search/remote` asks the
//! IMAP servers (`UID SEARCH TEXT`) and returns the matches the local indexes
//! missed, which in `lazy` body mode is every message whose body was never
//! downloaded. The client fires both and merges the slower one in when it
//! arrives.

use std::{sync::Arc, time::Duration};

use axum::{
    extract::{Extension, Query, State},
    response::IntoResponse,
    Json,
};
use mail_sync::provider::ProviderKind;
use serde::Deserialize;
use serde_json::json;

use crate::{error::AppError, middleware::UserId, state::AppState};

/// Query of `GET /api/search` and `GET /api/search/remote`: free text `q` plus
/// header, date, folder, account and flag filters, with cursor paging (`limit`
/// capped at 50; the remote endpoint ignores `cursor` and `limit`).
#[derive(Deserialize)]
pub struct SearchQuery {
    q: Option<String>,
    from: Option<String>,
    to: Option<String>,
    subject: Option<String>,
    not: Option<String>,
    after: Option<String>,
    before: Option<String>,
    folder: Option<String>,
    account_id: Option<String>,
    is_read: Option<bool>,
    is_flagged: Option<bool>,
    has_attachment: Option<bool>,
    cursor: Option<String>,
    limit: Option<i64>,
}

/// Columns of one search result row, in `RESULT_COLUMNS` order.
type ResultRow = (
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    bool,
    bool,
    String,
    String,
    Option<String>,
    Option<String>,
);

const RESULT_COLUMNS: &str = "m.id, m.thread_id, m.subject, m.from_addr, m.snippet, \
     m.internal_date, m.is_read, m.is_flagged, m.account_id, m.folder_id, m.list_id, f.full_path";

const RESULT_SOURCE: &str = "messages m LEFT JOIN folders f ON f.id = m.folder_id";

/// `GET /api/search`: messages matching the filters in the local indexes,
/// newest first, as a cursor-paged list. The first page also carries `total`,
/// the number of matches over all pages. Deleted messages are excluded.
pub async fn search(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(q): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let limit = q.limit.unwrap_or(50).clamp(1, 50);

    let mut conditions = vec!["m.is_deleted = 0".to_string()];
    let mut binds: Vec<String> = vec![];
    if let Some(text) = q.q.as_deref() {
        push_text_conditions(&user_db, text, &mut conditions, &mut binds).await;
    }
    push_filter_conditions(&q, &mut conditions, &mut binds);

    // The total is counted before the cursor narrows the set, and only for the
    // first page: later pages would report the same number.
    let total = if q.cursor.is_none() {
        let sql = format!(
            "SELECT COUNT(*) FROM {RESULT_SOURCE} WHERE {}",
            conditions.join(" AND ")
        );
        let mut query = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()));
        for bind in &binds {
            query = query.bind(bind);
        }
        Some(query.fetch_one(&user_db).await?)
    } else {
        None
    };

    // Cursor-based pagination — bound like every other value instead of being
    // spliced into the SQL text.
    if let Some(ref cursor) = q.cursor {
        conditions.push("m.internal_date < ?".to_string());
        binds.push(cursor.clone());
    }

    let rows = fetch_rows(&user_db, &conditions, &binds, limit).await?;
    let next_cursor = rows.last().map(|r| r.5.clone());
    let has_more = rows.len() as i64 == limit;
    let items: Vec<_> = rows.into_iter().map(row_json).collect();

    Ok(Json(json!({
        "items": items,
        "total": total,
        "next_cursor": if has_more { next_cursor } else { None::<String> },
    })))
}

/// Folders searched on the server per account, largest first. Every folder
/// costs a SELECT plus a SEARCH round trip.
const MAX_REMOTE_FOLDERS: i64 = 25;
/// Newest server matches kept per folder.
const MAX_REMOTE_UIDS_PER_FOLDER: usize = 500;
/// Server-only matches returned in one response.
const MAX_REMOTE_RESULTS: i64 = 200;
/// Time one account gets to answer before it is reported as failed.
const REMOTE_ACCOUNT_TIMEOUT: Duration = Duration::from_secs(25);

/// `GET /api/search/remote`: the free-text matches only the mail servers know
/// about. Every IMAP account searches its synced folders with `UID SEARCH
/// TEXT` (headers and body); the hits are mapped back to local messages, run
/// through the same filters as the local search, and returned without the
/// messages `GET /api/search` already finds. Accounts are queried
/// concurrently; one that fails or times out is counted in `failed_accounts`
/// instead of failing the request.
pub async fn search_remote(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(q): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let terms = Arc::new(q.q.as_deref().map(split_terms).unwrap_or_default());

    let accounts: Vec<(String, String)> =
        sqlx::query_as("SELECT id, provider_kind FROM email_accounts")
            .fetch_all(&user_db)
            .await?;
    let accounts: Vec<String> = accounts
        .into_iter()
        .filter(|(id, kind)| {
            ProviderKind::parse(kind).syncs_over_imap()
                && q.account_id.as_deref().is_none_or(|wanted| wanted == id)
        })
        .map(|(id, _)| id)
        .collect();

    if terms.is_empty() || accounts.is_empty() {
        return Ok(Json(json!({
            "items": [],
            "searched_accounts": 0,
            "failed_accounts": 0,
        })));
    }

    let mut tasks = tokio::task::JoinSet::new();
    for account_id in &accounts {
        tasks.spawn(remote_account_hits(
            state.clone(),
            user_db.clone(),
            account_id.clone(),
            terms.clone(),
        ));
    }
    let mut hits: Vec<(String, Vec<u32>)> = Vec::new();
    let mut failed_accounts = 0;
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(Ok(account_hits)) => hits.extend(account_hits),
            Ok(Err(error)) => {
                tracing::warn!(%error, "server-side search failed for an account");
                failed_accounts += 1;
            }
            Err(error) => {
                tracing::warn!(%error, "server-side search task aborted");
                failed_accounts += 1;
            }
        }
    }

    let mut conditions = vec!["m.is_deleted = 0".to_string()];
    let mut binds: Vec<String> = vec![];
    let items: Vec<serde_json::Value> = match remote_hits_condition(&hits) {
        Some((condition, hit_binds)) => {
            conditions.push(condition);
            binds.extend(hit_binds);

            // Leave out what the local search returns anyway, so the client
            // gets the additions only.
            let mut local = Vec::new();
            let mut local_binds = Vec::new();
            let text = q.q.as_deref().unwrap_or_default();
            push_text_conditions(&user_db, text, &mut local, &mut local_binds).await;
            if !local.is_empty() {
                conditions.push(format!("NOT ({})", local.join(" AND ")));
                binds.extend(local_binds);
            }

            push_filter_conditions(&q, &mut conditions, &mut binds);
            fetch_rows(&user_db, &conditions, &binds, MAX_REMOTE_RESULTS)
                .await?
                .into_iter()
                .map(row_json)
                .collect()
        }
        None => Vec::new(),
    };

    Ok(Json(json!({
        "items": items,
        "searched_accounts": accounts.len(),
        "failed_accounts": failed_accounts,
    })))
}

/// Search every synced, non-empty folder of one account on its server and
/// return `(folder_id, matching uids)` per folder with hits. Uses the
/// on-demand session pool, so it shares the per-account connection cap with
/// body downloads and reuses a warm session when there is one.
async fn remote_account_hits(
    state: AppState,
    user_db: sqlx::SqlitePool,
    account_id: String,
    terms: Arc<Vec<String>>,
) -> Result<Vec<(String, Vec<u32>)>, String> {
    let folders: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, full_path FROM folders \
         WHERE account_id = ? AND sync_enabled = 1 AND live_message_count > 0 \
         ORDER BY live_message_count DESC LIMIT ?",
    )
    .bind(&account_id)
    .bind(MAX_REMOTE_FOLDERS)
    .fetch_all(&user_db)
    .await
    .map_err(|e| e.to_string())?;
    if folders.is_empty() {
        return Ok(Vec::new());
    }

    let _permit = state.body_fetch_limiter.acquire(&account_id).await;
    let (kind, config) =
        crate::routes::messages::provider_session_config(&state, &user_db, &account_id)
            .await
            .map_err(|e| e.to_string())?;

    // The first attempt may get a pooled session the server has closed in the
    // meantime; that only shows on use, so a failure is retried once on a
    // connection that is known to be new.
    let mut last_error = String::new();
    for fresh in [false, true] {
        let connected = if fresh {
            mail_sync::provider::connect(kind, &config).await
        } else {
            state
                .body_fetch_limiter
                .checkout(&account_id, kind, &config)
                .await
        };
        let mut provider = match connected {
            Ok(provider) => provider,
            Err(error) => {
                last_error = error.to_string();
                continue;
            }
        };
        let searched = tokio::time::timeout(
            REMOTE_ACCOUNT_TIMEOUT,
            search_folders(provider.as_mut(), &folders, &terms),
        )
        .await;
        match searched {
            Ok(Ok(hits)) => {
                state.body_fetch_limiter.checkin(&account_id, provider);
                return Ok(hits);
            }
            // A failed or abandoned command leaves the session in an unknown
            // state; never hand it to the next request.
            Ok(Err(error)) => {
                crate::body_fetch::close_in_background(provider);
                last_error = error.to_string();
            }
            Err(_) => {
                crate::body_fetch::close_in_background(provider);
                return Err("timed out".to_string());
            }
        }
    }
    Err(last_error)
}

/// Run the text search in each folder on one session, returning `(folder_id,
/// matching uids)` for the folders with hits.
async fn search_folders(
    provider: &mut dyn mail_sync::provider::MailProvider,
    folders: &[(String, String)],
    terms: &[String],
) -> Result<Vec<(String, Vec<u32>)>, mail_sync::provider::ProviderError> {
    let mut hits = Vec::new();
    for (folder_id, full_path) in folders {
        match provider.search_text(full_path, terms).await? {
            Some(uids) if !uids.is_empty() => hits.push((folder_id.clone(), uids)),
            Some(_) => {}
            // The backend has no server-side search at all.
            None => break,
        }
    }
    Ok(hits)
}

/// SQL condition selecting the local rows of server hits, with its binds
/// (the folder ids). Uids are numbers and go into the SQL text directly; only
/// the newest [`MAX_REMOTE_UIDS_PER_FOLDER`] per folder are kept. `None` when
/// there are no hits.
fn remote_hits_condition(hits: &[(String, Vec<u32>)]) -> Option<(String, Vec<String>)> {
    let mut groups = Vec::new();
    let mut binds = Vec::new();
    for (folder_id, uids) in hits {
        let mut uids = uids.clone();
        uids.sort_unstable_by(|a, b| b.cmp(a));
        uids.truncate(MAX_REMOTE_UIDS_PER_FOLDER);
        if uids.is_empty() {
            continue;
        }
        let list = uids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        groups.push(format!("(m.folder_id = ? AND m.uid IN ({list}))"));
        binds.push(folder_id.clone());
    }
    (!groups.is_empty()).then(|| (format!("({})", groups.join(" OR ")), binds))
}

/// Run the result query for the given conditions, newest first.
async fn fetch_rows(
    db: &sqlx::SqlitePool,
    conditions: &[String],
    binds: &[String],
    limit: i64,
) -> Result<Vec<ResultRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {RESULT_COLUMNS} FROM {RESULT_SOURCE} WHERE {} \
         ORDER BY m.internal_date DESC LIMIT ?",
        conditions.join(" AND ")
    );
    let mut query = sqlx::query_as::<_, ResultRow>(sqlx::AssertSqlSafe(sql.as_str()));
    for bind in binds {
        query = query.bind(bind);
    }
    query.bind(limit).fetch_all(db).await
}

fn row_json(row: ResultRow) -> serde_json::Value {
    let (
        id,
        thread_id,
        subject,
        from_addr,
        snippet,
        internal_date,
        is_read,
        is_flagged,
        account_id,
        folder_id,
        list_id,
        folder_path,
    ) = row;
    json!({
        "id": id,
        "thread_id": thread_id,
        "subject": subject,
        "from_addr": from_addr,
        "snippet": snippet,
        "internal_date": internal_date,
        "is_read": is_read,
        "is_flagged": is_flagged,
        "account_id": account_id,
        "folder_id": folder_id,
        "folder_path": folder_path,
        "list_id": list_id,
    })
}

/// Append the structured (non free-text) filters of a search query.
fn push_filter_conditions(q: &SearchQuery, conditions: &mut Vec<String>, binds: &mut Vec<String>) {
    if let Some(from) = q.from.as_deref().filter(|v| !v.trim().is_empty()) {
        conditions.push("m.from_addr LIKE ? ESCAPE '\\'".to_string());
        binds.push(like_pattern(from.trim()));
    }
    if let Some(to) = q.to.as_deref().filter(|v| !v.trim().is_empty()) {
        conditions.push("m.to_addrs LIKE ? ESCAPE '\\'".to_string());
        binds.push(like_pattern(to.trim()));
    }
    if let Some(subject) = q.subject.as_deref().filter(|v| !v.trim().is_empty()) {
        conditions.push("m.subject LIKE ? ESCAPE '\\'".to_string());
        binds.push(like_pattern(subject.trim()));
    }
    // Exclude messages whose subject/sender/snippet contain any of these terms.
    if let Some(not) = q.not.as_deref() {
        for term in split_terms(not) {
            conditions.push(
                "(m.subject NOT LIKE ? ESCAPE '\\' AND m.from_addr NOT LIKE ? ESCAPE '\\' \
                 AND m.snippet NOT LIKE ? ESCAPE '\\')"
                    .to_string(),
            );
            let pattern = like_pattern(&term);
            for _ in 0..3 {
                binds.push(pattern.clone());
            }
        }
    }
    if let Some(ref after) = q.after {
        conditions.push("m.internal_date >= ?".to_string());
        binds.push(after.clone());
    }
    if let Some(ref before) = q.before {
        conditions.push("m.internal_date <= ?".to_string());
        binds.push(before.clone());
    }
    if let Some(ref folder) = q.folder {
        conditions.push("f.full_path = ?".to_string());
        binds.push(folder.clone());
    }
    if let Some(ref account) = q.account_id {
        conditions.push("m.account_id = ?".to_string());
        binds.push(account.clone());
    }
    if let Some(is_read) = q.is_read {
        conditions.push("m.is_read = ?".to_string());
        binds.push(if is_read { "1" } else { "0" }.to_string());
    }
    if let Some(is_flagged) = q.is_flagged {
        conditions.push("m.is_flagged = ?".to_string());
        binds.push(if is_flagged { "1" } else { "0" }.to_string());
    }
    if q.has_attachment == Some(true) {
        conditions
            .push("EXISTS (SELECT 1 FROM attachments a WHERE a.message_id = m.id)".to_string());
    }
}

/// Max typo alternatives tried for a query word that matches nothing as typed.
const MAX_FUZZY_TERMS: usize = 5;

/// Max free-text terms honoured per query (keeps the SQL bounded).
const MAX_QUERY_TERMS: usize = 12;

/// Word index over subject, sender and body.
const MAIN_INDEX: &str = "messages_fts";
/// Word index over the To/Cc headers.
const ADDRESS_INDEX: &str = "messages_addr_fts";
/// Trigram (substring) index over the subject.
const SUBJECT_SUBSTRING_INDEX: &str = "messages_subject_tri";

/// Split free text into search terms: whitespace separates terms, a
/// double-quoted run stays together as one phrase. Empty terms are dropped
/// and the list is capped at [`MAX_QUERY_TERMS`].
fn split_terms(raw: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for c in raw.chars() {
        if c == '"' {
            in_quotes = !in_quotes;
        } else if c.is_whitespace() && !in_quotes {
            terms.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    terms.push(current);
    terms
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .take(MAX_QUERY_TERMS)
        .collect()
}

/// Append one condition per free-text term, so a message must contain *every*
/// term. A term may match in any of the three indexes, all without scanning
/// `messages`:
///
/// * as a word or word prefix in subject, sender or body (`rechn` finds
///   `Rechnung`), and likewise in the recipients;
/// * as a substring of the subject, which compound words need (`rechnung`
///   finds `Abrechnung`);
/// * as a whole Message-ID, for id-shaped terms only: ordinary words hit
///   those opaque strings by accident.
///
/// Typo tolerance is a fallback: indexed words close to the term are used only
/// when the term as typed matches nothing anywhere, so a correctly spelled
/// query is never widened to its neighbours (`test` must not return `best`
/// and `rest`).
async fn push_text_conditions(
    db: &sqlx::SqlitePool,
    text: &str,
    conditions: &mut Vec<String>,
    binds: &mut Vec<String>,
) {
    for term in split_terms(text) {
        let lower = term.to_lowercase();
        let mut sources: Vec<String> = Vec::new();
        let mut term_binds: Vec<String> = Vec::new();

        // Keep MATCH inside the final query. Materialising and truncating
        // rowids first made older messages disappear for common terms before
        // filters and date ordering were applied.
        let word_query = word_match_query(&lower);
        if let Some(query) = &word_query {
            for index in [MAIN_INDEX, ADDRESS_INDEX] {
                sources.push(match_source(index));
                term_binds.push(query.clone());
            }
        }
        // The trigram tokenizer cannot match anything shorter than 3 chars.
        let substring_query = (lower.chars().count() >= 3).then(|| quote_term(&lower));
        if let Some(query) = &substring_query {
            sources.push(match_source(SUBJECT_SUBSTRING_INDEX));
            term_binds.push(query.clone());
        }

        if let Some(query) = &word_query {
            let found = fts_matches(db, MAIN_INDEX, query).await
                || fts_matches(db, ADDRESS_INDEX, query).await
                || match &substring_query {
                    Some(sub) => fts_matches(db, SUBJECT_SUBSTRING_INDEX, sub).await,
                    None => false,
                };
            if !found {
                if let Some(typo_query) = typo_match_query(db, &lower).await {
                    sources.push(match_source(MAIN_INDEX));
                    term_binds.push(typo_query);
                }
            }
        }

        let mut alternatives: Vec<String> = Vec::new();
        if !sources.is_empty() {
            alternatives.push(format!("m.rowid IN ({})", sources.join(" UNION ")));
        }
        if term.contains('@') {
            let bare = term.trim_matches(|c| c == '<' || c == '>');
            alternatives.push("m.message_id_header IN (?, ?)".to_string());
            term_binds.push(bare.to_string());
            term_binds.push(format!("<{bare}>"));
        }
        // A term of punctuation only has nothing to match on; it does not
        // constrain the result.
        if alternatives.is_empty() {
            continue;
        }
        conditions.push(format!("({})", alternatives.join(" OR ")));
        binds.extend(term_binds);
    }
}

/// Subquery yielding the rowids an FTS index matches for one bound query.
fn match_source(index: &str) -> String {
    format!("SELECT rowid FROM {index} WHERE {index} MATCH ?")
}

/// FTS5 expression for a term in the word indexes, or `None` when the term
/// holds nothing the tokenizer would index. A plain word matches itself and
/// longer continuations; phrases and terms with punctuation (addresses,
/// `e-mail`) match as an exact token sequence.
fn word_match_query(lower: &str) -> Option<String> {
    if !lower.chars().any(char::is_alphanumeric) {
        return None;
    }
    Some(if lower.chars().all(char::is_alphanumeric) {
        quote_prefix_term(lower)
    } else {
        quote_term(lower)
    })
}

/// FTS5 expression matching the indexed words closest to a misspelled plain
/// word, or `None` when the term is not a plain word, too short to correct
/// sensibly, or has no near neighbour.
async fn typo_match_query(db: &sqlx::SqlitePool, lower: &str) -> Option<String> {
    let length = lower.chars().count();
    if length < 4 || !lower.chars().all(char::is_alphanumeric) {
        return None;
    }
    let max_dist = if length <= 6 { 1 } else { 2 };
    let nearest = nearest_terms(db, lower, max_dist).await;
    (!nearest.is_empty()).then(|| {
        nearest
            .iter()
            .map(|t| quote_term(t))
            .collect::<Vec<_>>()
            .join(" OR ")
    })
}

/// Whether the FTS expression matches at least one row of the index. Errors
/// count as a match so a broken lookup never triggers typo expansion.
async fn fts_matches(db: &sqlx::SqlitePool, index: &str, fts_query: &str) -> bool {
    let sql = format!("SELECT EXISTS(SELECT 1 FROM {index} WHERE {index} MATCH ?)");
    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.as_str()))
        .bind(fts_query)
        .fetch_one(db)
        .await
        .map_or(true, |found| found != 0)
}

/// Indexed terms within `max_dist` edits of `word`, closest first, capped.
async fn nearest_terms(db: &sqlx::SqlitePool, word: &str, max_dist: usize) -> Vec<String> {
    let len = word.chars().count() as i64;
    let dist = max_dist as i64;
    // Length window prunes the candidate set before the (cheap) edit-distance pass.
    let candidates: Vec<String> =
        sqlx::query_scalar("SELECT term FROM messages_vocab WHERE length(term) BETWEEN ? AND ?")
            .bind(len - dist)
            .bind(len + dist)
            .fetch_all(db)
            .await
            .unwrap_or_default();

    let mut scored: Vec<(usize, String)> = candidates
        .into_iter()
        .filter_map(|term| {
            let d = strsim::levenshtein(word, &term);
            (d <= max_dist).then_some((d, term))
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(MAX_FUZZY_TERMS)
        .map(|(_, t)| t)
        .collect()
}

/// Quote a bareword for an FTS5 query, escaping embedded double quotes.
fn quote_term(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}

fn quote_prefix_term(term: &str) -> String {
    format!("{}*", quote_term(term))
}

fn like_pattern(term: &str) -> String {
    let escaped = term
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

#[cfg(test)]
mod tests {
    use super::{like_pattern, push_text_conditions, remote_hits_condition, split_terms};

    /// A migrated in-memory mail database holding one message per row of
    /// `(subject, from_addr, to_addrs)`. Message ids are `m1`, `m2`, … and
    /// uids 1, 2, … in slice order, all in folder `inbox`.
    async fn search_db(rows: &[(&str, &str, &str)]) -> Result<sqlx::SqlitePool, sqlx::Error> {
        // One connection: every pooled connection would otherwise open its
        // own empty in-memory database.
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        db::migrations::run_mail_migrations(&db)
            .await
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        sqlx::query(
            "INSERT INTO email_accounts
             (id, display_name, primary_email, imap_host, imap_port, imap_auth_scheme,
              smtp_host, smtp_port, smtp_auth_scheme, credentials_encrypted)
             VALUES ('acc', 'A', 'a@example.org', 'imap', 993, 'plain', 'smtp', 465, 'plain', X'00');
             INSERT INTO folders (id, account_id, name, full_path) VALUES ('inbox', 'acc', 'INBOX', 'INBOX');",
        )
        .execute(&db)
        .await?;
        for (index, (subject, from_addr, to_addrs)) in rows.iter().enumerate() {
            let number = index as i64 + 1;
            let id = format!("m{number}");
            sqlx::query(
                "INSERT INTO messages (id, account_id, folder_id, uid, message_id_header, subject, snippet, from_addr, to_addrs, internal_date)
                 VALUES (?, 'acc', 'inbox', ?, ?, ?, ?, ?, ?, '2026-01-01T00:00:00Z')",
            )
            .bind(&id)
            .bind(number)
            .bind(format!("<{id}-test-haus@mail.example.org>"))
            .bind(subject)
            .bind(subject)
            .bind(from_addr)
            .bind(to_addrs)
            .execute(&db)
            .await?;
            // The sync writes the main index itself; the two metadata indexes
            // follow through their triggers.
            sqlx::query(
                "INSERT INTO messages_fts(rowid, subject, from_addr, body_text)
                 VALUES ((SELECT rowid FROM messages WHERE id = ?), ?, ?, '')",
            )
            .bind(&id)
            .bind(subject)
            .bind(from_addr)
            .execute(&db)
            .await?;
        }
        Ok(db)
    }

    /// Ids of the messages the free-text query returns, in insertion order.
    async fn search_ids(db: &sqlx::SqlitePool, text: &str) -> Result<Vec<String>, sqlx::Error> {
        let mut conditions = vec!["1 = 1".to_string()];
        let mut binds = Vec::new();
        push_text_conditions(db, text, &mut conditions, &mut binds).await;
        let sql = format!(
            "SELECT m.id FROM messages m WHERE {} ORDER BY m.uid",
            conditions.join(" AND ")
        );
        let mut query = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql.as_str()));
        for bind in &binds {
            query = query.bind(bind);
        }
        query.fetch_all(db).await
    }

    const NONE: [&str; 0] = [];

    const ROWS: &[(&str, &str, &str)] = &[
        ("Decathlon receipt", "shop@example.org", "me@example.org"),
        ("Test results", "lab@example.org", "me@example.org"),
        (
            "Best offers of the rest of the year",
            "ads@example.org",
            "me@example.org",
        ),
        (
            "Abrechnung für Mai",
            "billing@example.org",
            "Anna Gmailer <anna@gmail.example.org>",
        ),
    ];

    #[tokio::test]
    async fn free_text_query_matches_prefixes_and_typos() -> Result<(), sqlx::Error> {
        let db = search_db(ROWS).await?;
        for raw in ["decat", "decatlon", "DECATHLON"] {
            assert_eq!(search_ids(&db, raw).await?, ["m1"], "query {raw:?}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn correctly_spelled_word_is_not_widened_to_neighbours() -> Result<(), sqlx::Error> {
        let db = search_db(ROWS).await?;
        // "best" and "rest" are one edit away from "test" but must not match.
        assert_eq!(search_ids(&db, "test").await?, ["m2"]);
        Ok(())
    }

    #[tokio::test]
    async fn every_term_must_match_across_fields() -> Result<(), sqlx::Error> {
        let db = search_db(ROWS).await?;
        // One term in the subject, the other only in the recipient header.
        assert_eq!(search_ids(&db, "abrechnung anna").await?, ["m4"]);
        assert_eq!(search_ids(&db, "abrechnung decathlon").await?, NONE);
        // Quoted phrases keep their word order.
        assert_eq!(search_ids(&db, "\"rest of the year\"").await?, ["m3"]);
        assert_eq!(search_ids(&db, "\"year of the rest\"").await?, NONE);
        Ok(())
    }

    #[tokio::test]
    async fn subject_matches_substrings_but_addresses_only_word_starts() -> Result<(), sqlx::Error>
    {
        let db = search_db(ROWS).await?;
        // Tail of a compound word in the subject, with and without diacritics.
        assert_eq!(search_ids(&db, "rechnung").await?, ["m4"]);
        assert_eq!(search_ids(&db, "fur").await?, ["m4"]);
        // Recipients match from the start of a name or address part …
        assert_eq!(search_ids(&db, "gmail").await?, ["m4"]);
        assert_eq!(search_ids(&db, "anna@gmail.example.org").await?, ["m4"]);
        // … not in the middle of one ("mailer" inside "Gmailer").
        assert_eq!(search_ids(&db, "mailer").await?, NONE);
        Ok(())
    }

    #[tokio::test]
    async fn message_ids_match_only_as_a_whole() -> Result<(), sqlx::Error> {
        let db = search_db(ROWS).await?;
        assert_eq!(search_ids(&db, "haus").await?, NONE);
        assert_eq!(
            search_ids(&db, "m2-test-haus@mail.example.org").await?,
            ["m2"]
        );
        assert_eq!(
            search_ids(&db, "<m2-test-haus@mail.example.org>").await?,
            ["m2"]
        );
        Ok(())
    }

    #[tokio::test]
    async fn metadata_indexes_follow_message_changes() -> Result<(), sqlx::Error> {
        let db = search_db(ROWS).await?;
        sqlx::query(
            "UPDATE messages SET subject = 'Mahnung', to_addrs = 'bob@example.org' WHERE id = 'm4'",
        )
        .execute(&db)
        .await?;
        // The sync refreshes the main index itself when headers change.
        sqlx::query(
            "INSERT OR REPLACE INTO messages_fts(rowid, subject, from_addr, body_text)
             VALUES ((SELECT rowid FROM messages WHERE id = 'm4'), 'Mahnung', 'billing@example.org', '')",
        )
        .execute(&db)
        .await?;
        assert_eq!(search_ids(&db, "rechnung").await?, NONE);
        assert_eq!(search_ids(&db, "anna").await?, NONE);
        assert_eq!(search_ids(&db, "ahnung bob").await?, ["m4"]);

        sqlx::query("DELETE FROM messages WHERE id = 'm4'")
            .execute(&db)
            .await?;
        assert_eq!(search_ids(&db, "ahnung").await?, NONE);
        // Both indexes still agree with the table they are built over.
        for index in ["messages_addr_fts", "messages_subject_tri"] {
            let sql = format!("INSERT INTO {index}({index}) VALUES ('integrity-check')");
            sqlx::query(sqlx::AssertSqlSafe(sql.as_str()))
                .execute(&db)
                .await?;
        }
        Ok(())
    }

    #[tokio::test]
    async fn server_hits_exclude_what_the_local_search_finds() -> Result<(), sqlx::Error> {
        let db = search_db(ROWS).await?;
        // The server reports uids 1, 2 and 99 (unknown locally) for "test";
        // locally only m2 matches, so m1 is the one addition.
        let hits = vec![("inbox".to_string(), vec![1, 2, 99])];
        let (condition, mut binds) = remote_hits_condition(&hits).expect("hits");
        let mut local = Vec::new();
        let mut local_binds = Vec::new();
        push_text_conditions(&db, "test", &mut local, &mut local_binds).await;
        binds.extend(local_binds);
        let sql = format!(
            "SELECT m.id FROM messages m WHERE {condition} AND NOT ({})",
            local.join(" AND ")
        );
        let mut query = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql.as_str()));
        for bind in &binds {
            query = query.bind(bind);
        }
        assert_eq!(query.fetch_all(&db).await?, ["m1"]);
        assert!(remote_hits_condition(&[("inbox".to_string(), vec![])]).is_none());
        Ok(())
    }

    #[test]
    fn terms_split_on_whitespace_and_keep_quoted_phrases() {
        assert_eq!(
            split_terms("  foo \"bar baz\"  qux \"\" "),
            ["foo", "bar baz", "qux"]
        );
    }

    #[test]
    fn like_fallback_treats_wildcards_as_literals() {
        assert_eq!(like_pattern(r"100%_done\today"), r"%100\%\_done\\today%");
    }
}
