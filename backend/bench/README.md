# bench — mail-list performance benchmark

Benchmarks the hot mail-list query path (`db::queries::unified_page` /
`folder_page`) against large synthetic mailboxes.

## Generate test data

```bash
# 5 mailboxes, 30k–50k messages each -> /tmp/mailquill_bench.db
cargo run -p bench --release --bin gen

# custom: <path> <accounts> <min> <max>
cargo run -p bench --release --bin gen -- /tmp/big.db 10 50000 80000
```

The generated db uses the production schema (the `db` crate's migrations) and is
left **without** ANALYZE statistics — the worst case for the query planner, so
the benchmark reflects a freshly-synced mailbox that hasn't been analysed yet.

## Run the benchmark

```bash
cargo bench -p bench          # regenerates the db if missing
MAILQUILL_BENCH_DB=/tmp/big.db cargo bench -p bench
```

## Per-component diagnosis

```bash
cargo run -p bench --release --bin diag   # times each query phase, no-stats vs ANALYZE
```

## Result (211k messages, 5 mailboxes, no statistics)

| query | time |
|-------|------|
| `unified_page` first page (50) | ~27 ms |
| `unified_page` cursor page | ~28 ms |

Determinism without ANALYZE comes from three things:
- partial index `idx_msg_folder_undeleted(folder_id) WHERE is_deleted = 0` for counts,
- per-folder equality counts instead of a `folder_type` join,
- `INDEXED BY idx_msg_thread` on the thread-enrichment `IN (...)` queries.
