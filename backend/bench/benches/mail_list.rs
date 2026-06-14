//! Benchmark the mail-list query path (`db::queries`) against a large synthetic
//! mailbox set. Generates the db once (to MAILQUILL_BENCH_DB or a temp path) if
//! it isn't already present.

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};
use sqlx::SqlitePool;
use tokio::runtime::Runtime;

fn bench_db_path() -> PathBuf {
    std::env::var("MAILQUILL_BENCH_DB")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/mailquill_bench.db"))
}

async fn setup(path: &std::path::Path) -> SqlitePool {
    let needs_gen = !path.exists()
        || std::fs::metadata(path).map(|m| m.len() < 1_000_000).unwrap_or(true);
    if needs_gen {
        eprintln!("generating benchmark db at {} ...", path.display());
        bench::generate(path, 5, 30_000, 50_000)
            .await
            .expect("generate bench db");
    }
    bench::open_pool(path).await.expect("open bench pool")
}

fn mail_list(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let path = bench_db_path();
    let db = rt.block_on(setup(&path));

    let total: i64 = rt.block_on(async {
        sqlx::query_scalar("SELECT COUNT(*) FROM messages")
            .fetch_one(&db)
            .await
            .unwrap()
    });
    eprintln!("benchmark db: {total} messages");

    let mut group = c.benchmark_group("mail_list");
    group.sample_size(30);

    group.bench_function("unified_inbox_page50", |b| {
        b.iter(|| {
            rt.block_on(async {
                db::queries::unified_page(&db, Some("inbox"), None, None, 50, false)
                    .await
                    .unwrap()
            })
        })
    });

    group.bench_function("unified_inbox_page50_cursor", |b| {
        // Page deep into the list to exercise the cursor path.
        let cursor = rt.block_on(async {
            let p = db::queries::unified_page(&db, Some("inbox"), None, None, 50, false)
                .await
                .unwrap();
            p.next_cursor
        });
        b.iter(|| {
            rt.block_on(async {
                db::queries::unified_page(&db, Some("inbox"), None, cursor.as_deref(), 50, false)
                    .await
                    .unwrap()
            })
        })
    });

    group.finish();
}

criterion_group!(benches, mail_list);
criterion_main!(benches);
