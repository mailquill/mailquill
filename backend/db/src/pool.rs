use crate::migrations::{run_mail_migrations, WAL_PRAGMAS};
use lru::LruCache;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::{
    num::NonZeroUsize,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

const MAX_POOLS: usize = 32;
const IDLE_EVICT: Duration = Duration::from_secs(600);

struct PoolEntry {
    pool: SqlitePool,
    last_used: Instant,
}

/// LRU cache of per-user SQLite connection pools.
pub struct UserDbPool {
    data_dir: PathBuf,
    cache: Mutex<LruCache<String, PoolEntry>>,
}

impl UserDbPool {
    pub fn new(data_dir: impl Into<PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            data_dir: data_dir.into(),
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(MAX_POOLS).unwrap())),
        })
    }

    /// Get or create a SqlitePool for the given user.
    pub async fn get(&self, user_id: &str) -> Result<SqlitePool, PoolError> {
        let mut cache = self.cache.lock().await;

        if let Some(entry) = cache.get_mut(user_id) {
            if entry.last_used.elapsed() < IDLE_EVICT {
                entry.last_used = Instant::now();
                return Ok(entry.pool.clone());
            }
            // Entry is idle; evict and reopen
            cache.pop(user_id);
        }

        let pool = self.open_user_pool(user_id).await?;
        cache.push(
            user_id.to_string(),
            PoolEntry {
                pool: pool.clone(),
                last_used: Instant::now(),
            },
        );
        Ok(pool)
    }

    async fn open_user_pool(&self, user_id: &str) -> Result<SqlitePool, PoolError> {
        let dir = self.data_dir.join("users").join(user_id);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(PoolError::Io)?;

        let db_path = dir.join("mail.db");
        let opts = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true);

        let pool = SqlitePool::connect_with(opts)
            .await
            .map_err(PoolError::Sqlx)?;

        sqlx::query(WAL_PRAGMAS)
            .execute(&pool)
            .await
            .map_err(PoolError::Sqlx)?;

        run_mail_migrations(&pool)
            .await
            .map_err(|e| PoolError::Sqlx(sqlx::Error::Protocol(e.to_string())))?;

        Ok(pool)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PoolError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
}
