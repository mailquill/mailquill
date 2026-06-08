use sqlx::{migrate::MigrateError, SqlitePool};

pub async fn run_app_migrations(pool: &SqlitePool) -> Result<(), MigrateError> {
    sqlx::migrate!("./migrations/app").run(pool).await
}

pub async fn run_mail_migrations(pool: &SqlitePool) -> Result<(), MigrateError> {
    sqlx::migrate!("./migrations/mail").run(pool).await
}

pub const WAL_PRAGMAS: &str = "
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA cache_size=-65536;
PRAGMA mmap_size=268435456;
PRAGMA temp_store=MEMORY;
PRAGMA foreign_keys=ON;
";
