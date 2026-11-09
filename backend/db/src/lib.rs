//! SQLite persistence for Mailquill: embedded migrations, the per-user
//! mail database pool cache and the shared mail list queries.

pub mod migrations;
pub mod pool;
pub mod queries;
