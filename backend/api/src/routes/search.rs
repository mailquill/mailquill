use axum::{
    extract::{Extension, Query, State},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::{error::AppError, middleware::UserId, state::AppState};

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

pub async fn search(
    State(state): State<AppState>,
    Extension(user): Extension<UserId>,
    Query(q): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let user_db = state.user_db_pool.get(&user.0).await?;
    let limit = q.limit.unwrap_or(50).min(50);

    let mut conditions = vec!["m.is_deleted = 0".to_string()];
    let mut binds: Vec<String> = vec![];

    // FTS full-text search. `rowid` is an INTEGER, so it must be decoded as i64
    // — decoding it as String silently failed (unwrap_or_default), which is why
    // search always returned nothing.
    let fts_ids: Option<Vec<i64>> = if let Some(ref fts_q) = q.q {
        if !fts_q.trim().is_empty() {
            // Typo-tolerant: expand each query word with close vocabulary terms
            // (e.g. "decatlon" → ("decatlon" OR "decathlon")).
            let fts_query = expand_fuzzy_query(&user_db, fts_q).await;
            let ids: Vec<i64> = sqlx::query_scalar(
                "SELECT rowid FROM messages_fts WHERE messages_fts MATCH ? ORDER BY rank LIMIT 1000",
            )
            .bind(&fts_query)
            .fetch_all(&user_db)
            .await
            .unwrap_or_default();
            Some(ids)
        } else {
            None
        }
    } else {
        None
    };

    if let Some(ids) = &fts_ids {
        if ids.is_empty() {
            return Ok(Json(json!({ "items": [], "next_cursor": null })));
        }
        // Inline the rowids as integer literals: they come from our own DB, not
        // user input, so there is nothing to escape.
        let list = ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        conditions.push(format!("m.rowid IN ({list})"));
    }

    if let Some(ref from) = q.from {
        conditions.push("m.from_addr LIKE ?".to_string());
        binds.push(format!("%{from}%"));
    }
    if let Some(ref to) = q.to {
        conditions.push("m.to_addrs LIKE ?".to_string());
        binds.push(format!("%{to}%"));
    }
    if let Some(ref subject) = q.subject {
        conditions.push("m.subject LIKE ?".to_string());
        binds.push(format!("%{subject}%"));
    }
    // Exclude messages whose subject/sender/snippet contain these words.
    if let Some(ref not) = q.not {
        if !not.trim().is_empty() {
            conditions.push(
                "(m.subject NOT LIKE ? AND m.from_addr NOT LIKE ? AND m.snippet NOT LIKE ?)"
                    .to_string(),
            );
            let pat = format!("%{not}%");
            binds.push(pat.clone());
            binds.push(pat.clone());
            binds.push(pat);
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
        binds.push(if is_read {
            "1".to_string()
        } else {
            "0".to_string()
        });
    }
    if let Some(is_flagged) = q.is_flagged {
        conditions.push("m.is_flagged = ?".to_string());
        binds.push(if is_flagged {
            "1".to_string()
        } else {
            "0".to_string()
        });
    }
    if q.has_attachment == Some(true) {
        conditions
            .push("EXISTS (SELECT 1 FROM attachments a WHERE a.message_id = m.id)".to_string());
    }

    // Cursor-based pagination
    if let Some(ref cursor) = q.cursor {
        conditions.push(format!(
            "m.internal_date < '{}'",
            cursor.replace('\'', "''")
        ));
    }

    let where_clause = conditions.join(" AND ");
    let sql = format!(
        "SELECT m.id, m.thread_id, m.subject, m.from_addr, m.snippet, m.internal_date, m.is_read, m.is_flagged, m.account_id, m.folder_id, m.list_id, f.full_path FROM messages m LEFT JOIN folders f ON f.id = m.folder_id WHERE {where_clause} ORDER BY m.internal_date DESC LIMIT ?",
    );

    let mut query = sqlx::query_as::<
        _,
        (
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
        ),
    >(&sql);
    for b in &binds {
        query = query.bind(b);
    }
    query = query.bind(limit);

    let rows = query.fetch_all(&user_db).await?;

    let next_cursor = rows.last().map(|r| r.5.clone());
    let has_more = rows.len() as i64 == limit;

    let items: Vec<_> = rows
        .into_iter()
        .map(
            |(
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
            )| {
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
            },
        )
        .collect();

    Ok(Json(json!({
        "items": items,
        "next_cursor": if has_more { next_cursor } else { None::<String> },
    })))
}

/// Max fuzzy alternatives added per query word (keeps the MATCH query bounded).
const MAX_FUZZY_TERMS: usize = 8;

/// Build a typo-tolerant FTS5 MATCH query from free-text input. Each word that
/// is long enough is OR-expanded with the closest indexed terms (by Levenshtein
/// distance) so small typos still match — "Decatlon" finds "Decathlon". Short
/// or non-alphanumeric words are passed through verbatim (quoted).
async fn expand_fuzzy_query(db: &sqlx::SqlitePool, raw: &str) -> String {
    let mut groups: Vec<String> = Vec::new();
    for word in raw.split_whitespace() {
        let lower = word.to_lowercase();
        // Only fuzz alphanumeric words of 4+ chars; shorter/odd tokens stay exact.
        let fuzzable = lower.chars().all(|c| c.is_alphanumeric()) && lower.chars().count() >= 4;
        if !fuzzable {
            groups.push(quote_term(&lower));
            continue;
        }
        let max_dist = if lower.chars().count() <= 6 { 1 } else { 2 };
        let mut alts = nearest_terms(db, &lower, max_dist).await;
        // Always keep the original word as an alternative.
        if !alts.iter().any(|t| t == &lower) {
            alts.insert(0, lower.clone());
        }
        let ored = alts
            .iter()
            .map(|t| quote_term(t))
            .collect::<Vec<_>>()
            .join(" OR ");
        groups.push(format!("({ored})"));
    }
    // Space between groups = implicit AND in FTS5.
    groups.join(" ")
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
