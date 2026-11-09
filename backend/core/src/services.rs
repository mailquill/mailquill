//! Placeholder service handles grouped around a shared database pool.
//!
//! None of these carry behaviour yet; they reserve the service boundaries the
//! API layer is expected to grow into.

use sqlx::AnyPool;
use std::sync::Arc;

/// Database pool type the services are constructed over.
pub type DbPool = AnyPool;

/// Account management service (placeholder).
pub struct AccountService {
    _db: Arc<DbPool>,
}

impl AccountService {
    /// Create the service over the shared pool.
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { _db: db }
    }
}

/// Message access service (placeholder).
pub struct MessageService {
    _db: Arc<DbPool>,
}

impl MessageService {
    /// Create the service over the shared pool.
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { _db: db }
    }
}

/// Calendar service (placeholder).
pub struct CalendarService {
    _db: Arc<DbPool>,
}

impl CalendarService {
    /// Create the service over the shared pool.
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { _db: db }
    }
}

/// Contact service (placeholder).
pub struct ContactService {
    _db: Arc<DbPool>,
}

impl ContactService {
    /// Create the service over the shared pool.
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { _db: db }
    }
}

/// Mailbox synchronisation engine handle (placeholder).
pub struct SyncEngine {
    _db: Arc<DbPool>,
}

impl SyncEngine {
    /// Create the service over the shared pool.
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { _db: db }
    }
}

/// Outbound mail service (placeholder).
pub struct SmtpService {
    _db: Arc<DbPool>,
}

impl SmtpService {
    /// Create the service over the shared pool.
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { _db: db }
    }
}

/// Cryptography service (placeholder).
pub struct CryptoService {
    _db: Arc<DbPool>,
}

impl CryptoService {
    /// Create the service over the shared pool.
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { _db: db }
    }
}
