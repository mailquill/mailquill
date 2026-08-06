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

    if let Some(fts_q) = q.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        // Keep MATCH inside the final query. Materialising and truncating rowids
        // first made older messages disappear for common terms before filters
        // and date ordering were applied. The LIKE fallbacks cover searchable
        // metadata that is not part of the body-oriented FTS index.
        conditions.push(
            "(m.rowid IN (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?) \
             OR m.subject LIKE ? ESCAPE '\\' \
             OR m.from_addr LIKE ? ESCAPE '\\' \
             OR m.to_addrs LIKE ? ESCAPE '\\' \
             OR m.cc_addrs LIKE ? ESCAPE '\\' \
             OR m.snippet LIKE ? ESCAPE '\\' \
             OR COALESCE(m.message_id_header, '') LIKE ? ESCAPE '\\')"
                .to_string(),
        );
        binds.push(expand_fuzzy_query(&user_db, fts_q).await);
        let pattern = like_pattern(fts_q);
        for _ in 0..6 {
            binds.push(pattern.clone());
        }
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
    >(sqlx::AssertSqlSafe(sql.as_str()));
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

/// Build a prefix- and typo-tolerant FTS5 MATCH query from free-text input.
/// Alphanumeric terms match complete tokens and longer continuations, while
/// sufficiently long terms are OR-expanded with nearby indexed vocabulary.
async fn expand_fuzzy_query(db: &sqlx::SqlitePool, raw: &str) -> String {
    let mut groups: Vec<String> = Vec::new();
    for word in raw.split_whitespace() {
        let lower = word.to_lowercase();
        let alphanumeric = lower.chars().all(|c| c.is_alphanumeric());
        if !alphanumeric {
            groups.push(quote_term(&lower));
            continue;
        }
        // Only fuzz words of 4+ chars; shorter terms still use prefix matching.
        let fuzzable = lower.chars().count() >= 4;
        if !fuzzable {
            groups.push(quote_prefix_term(&lower));
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
            .map(|t| quote_prefix_term(t))
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
    use super::{expand_fuzzy_query, like_pattern};

    async fn search_db() -> Result<sqlx::SqlitePool, sqlx::Error> {
        let db = sqlx::SqlitePool::connect("sqlite::memory:").await?;
        sqlx::query(
            "CREATE VIRTUAL TABLE messages_fts USING fts5(subject, from_addr, body_text, content='')",
        )
        .execute(&db)
        .await?;
        sqlx::query("CREATE VIRTUAL TABLE messages_vocab USING fts5vocab('messages_fts', 'row')")
            .execute(&db)
            .await?;
        sqlx::query(
            "INSERT INTO messages_fts(rowid, subject, from_addr, body_text) VALUES (1, 'Decathlon receipt', 'shop@example.test', '')",
        )
        .execute(&db)
        .await?;
        Ok(db)
    }

    #[tokio::test]
    async fn free_text_query_matches_prefixes_and_typos() -> Result<(), sqlx::Error> {
        let db = search_db().await?;

        for raw in ["decat", "decatlon"] {
            let query = expand_fuzzy_query(&db, raw).await;
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM messages_fts WHERE messages_fts MATCH ?")
                    .bind(query)
                    .fetch_one(&db)
                    .await?;
            assert_eq!(count, 1, "query {raw:?} should find the indexed subject");
        }
        Ok(())
    }

    #[test]
    fn like_fallback_treats_wildcards_as_literals() {
        assert_eq!(like_pattern(r"100%_done\today"), r"%100\%\_done\\today%");
    }
}
