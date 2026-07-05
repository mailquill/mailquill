use sqlx::AnyPool;
use std::sync::Arc;

pub type DbPool = AnyPool;

pub struct AccountService {
    _db: Arc<DbPool>,
}

impl AccountService {
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { _db: db }
    }
}

pub struct MessageService {
    _db: Arc<DbPool>,
}

impl MessageService {
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { _db: db }
    }
}

pub struct CalendarService {
    _db: Arc<DbPool>,
}

impl CalendarService {
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { _db: db }
    }
}

pub struct ContactService {
    _db: Arc<DbPool>,
}

impl ContactService {
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { _db: db }
    }
}

pub struct SyncEngine {
    _db: Arc<DbPool>,
}

impl SyncEngine {
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { _db: db }
    }
}

pub struct SmtpService {
    _db: Arc<DbPool>,
}

impl SmtpService {
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { _db: db }
    }
}

pub struct CryptoService {
    _db: Arc<DbPool>,
}

impl CryptoService {
    pub fn new(db: Arc<DbPool>) -> Self {
        Self { _db: db }
    }
}
