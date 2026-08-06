//! Per-component timing of the unified list query, in the real Rust/sqlx/
//! bundled-sqlite stack. Generates a fresh (no-stats) db, reports each phase,
//! then ANALYZEs and reports again.

use sqlx::SqlitePool;
use std::path::Path;
use std::time::Instant;

async fn avg<F, Fut>(n: u32, mut f: F) -> f64
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    f().await; // warmup
    let t = Instant::now();
    for _ in 0..n {
        f().await;
    }
    t.elapsed().as_secs_f64() * 1000.0 / n as f64
}

async fn page_thread_ids(db: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT m.thread_id FROM messages m JOIN folders f ON f.id = m.folder_id WHERE f.folder_type = 'INBOX' AND m.is_deleted = 0 ORDER BY m.internal_date DESC LIMIT 50",
    )
    .fetch_all(db)
    .await
    .unwrap()
}

async fn enrich(db: &SqlitePool, ids: &[String], indexed_by: bool) {
    let ph = vec!["?"; ids.len()].join(",");
    let hint = if indexed_by {
        "INDEXED BY idx_msg_thread"
    } else {
        ""
    };
    let sql_c = format!(
        "SELECT thread_id, COUNT(*), SUM(CASE WHEN is_read=0 THEN 1 ELSE 0 END) FROM messages {hint} WHERE thread_id IN ({ph}) AND is_deleted=0 GROUP BY thread_id",
    );
    let mut q = sqlx::query_as::<_, (String, i64, i64)>(sqlx::AssertSqlSafe(sql_c.as_str()));
    for id in ids {
        q = q.bind(id);
    }
    q.fetch_all(db).await.unwrap();

    let sql_p = format!(
        "SELECT thread_id, from_addr FROM messages {hint} WHERE thread_id IN ({ph}) AND is_deleted=0 ORDER BY internal_date ASC",
    );
    let mut qp = sqlx::query_as::<_, (String, String)>(sqlx::AssertSqlSafe(sql_p.as_str()));
    for id in ids {
        qp = qp.bind(id);
    }
    qp.fetch_all(db).await.unwrap();
}

async fn report(db: &SqlitePool, label: &str) {
    let n = 10;
    let ids = page_thread_ids(db).await;
    println!("== {label} ==");
    println!(
        "  enrich (IN)        : {:.2} ms",
        avg(n, || enrich(db, &ids, false)).await
    );
    println!(
        "  enrich (INDEXED BY): {:.2} ms",
        avg(n, || enrich(db, &ids, true)).await
    );
    println!(
        "  unified_page       : {:.2} ms",
        avg(n, || async {
            db::queries::unified_page(db, Some("inbox"), None, None, 50, false)
                .await
                .unwrap();
        })
        .await
    );
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = Path::new("/tmp/diag_bench.db");
    eprintln!("generating fresh no-stats db ...");
    bench::generate(path, 5, 30_000, 50_000).await?;
    let db = bench::open_pool(path).await?;

    report(&db, "NO ANALYZE").await;
    sqlx::query("ANALYZE").execute(&db).await?;
    report(&db, "AFTER ANALYZE").await;
    Ok(())
}
