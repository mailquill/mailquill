//! Generate a synthetic mail.db for benchmarking.
//!
//! Usage: cargo run -p bench --release --bin gen -- [path] [accounts] [min] [max]
//! Defaults: /tmp/mailquill_bench.db 5 30000 50000

use std::path::PathBuf;
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = PathBuf::from(
        args.get(1)
            .cloned()
            .unwrap_or_else(|| "/tmp/mailquill_bench.db".into()),
    );
    let accounts: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);
    let min: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(30_000);
    let max: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(50_000);

    println!("generating {accounts} mailboxes, {min}-{max} messages each -> {}", path.display());
    let t = Instant::now();
    let total = bench::generate(&path, accounts, min, max).await?;
    println!("done: {total} messages in {:.1}s", t.elapsed().as_secs_f64());
    Ok(())
}
