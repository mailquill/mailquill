//! Per-account cap and connection reuse for on-demand body downloads.
//!
//! Lazy-synced messages have no stored body, so every detail request goes to
//! the provider. Two things made that slow: the message list prefetches
//! details on hover, so a quick sweep over the rows opened one IMAP login per
//! row and ran into provider connection limits (Gmail allows 15 per account,
//! shared with the sync and IDLE connections); and every single request paid
//! TLS handshake plus LOGIN, which dominates the time until a message opens.
//!
//! The limiter caps concurrent downloads per account, and sessions that
//! served a request successfully are kept for a short while so the next body
//! of the same account reuses them.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use mail_sync::provider::{self, MailProvider, ProviderConfig, ProviderError, ProviderKind};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Concurrent provider logins allowed per account for body downloads.
const PERMITS_PER_ACCOUNT: usize = 2;
/// How long an idle session is kept for reuse. Short enough that a server's
/// own idle timeout never bites first.
const IDLE_SESSION_TTL: Duration = Duration::from_secs(60);

struct IdleSession {
    provider: Box<dyn MailProvider>,
    since: Instant,
}

#[derive(Default)]
pub struct BodyFetchLimiter {
    per_account: Mutex<HashMap<String, Arc<Semaphore>>>,
    idle: Mutex<HashMap<String, Vec<IdleSession>>>,
}

impl BodyFetchLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Wait for a download slot of the account. The slot is released when the
    /// returned permit is dropped.
    pub async fn acquire(&self, account_id: &str) -> OwnedSemaphorePermit {
        let semaphore = {
            let mut per_account = self
                .per_account
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            per_account
                .entry(account_id.to_owned())
                .or_insert_with(|| Arc::new(Semaphore::new(PERMITS_PER_ACCOUNT)))
                .clone()
        };
        semaphore
            .acquire_owned()
            .await
            .expect("body fetch semaphore is never closed")
    }

    /// A provider session for the account: a recently used idle one when
    /// available, otherwise a fresh connection. Call with a permit held.
    pub async fn checkout(
        &self,
        account_id: &str,
        kind: ProviderKind,
        config: &ProviderConfig,
    ) -> Result<Box<dyn MailProvider>, ProviderError> {
        let (reusable, stale) = {
            let mut idle = self
                .idle
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let sessions = idle.entry(account_id.to_owned()).or_default();
            let (fresh, stale): (Vec<IdleSession>, Vec<IdleSession>) = sessions
                .drain(..)
                .partition(|session| session.since.elapsed() < IDLE_SESSION_TTL);
            *sessions = fresh;
            (sessions.pop(), stale)
        };
        for session in stale {
            close_in_background(session.provider);
        }
        match reusable {
            Some(session) => Ok(session.provider),
            None => provider::connect(kind, config).await,
        }
    }

    /// Return a session that completed its request successfully, for reuse.
    /// Sessions whose request failed must be dropped by the caller instead;
    /// the connection may be broken.
    pub fn checkin(&self, account_id: &str, provider: Box<dyn MailProvider>) {
        let overflow = {
            let mut idle = self
                .idle
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let sessions = idle.entry(account_id.to_owned()).or_default();
            if sessions.len() < PERMITS_PER_ACCOUNT {
                sessions.push(IdleSession {
                    provider,
                    since: Instant::now(),
                });
                None
            } else {
                Some(provider)
            }
        };
        if let Some(provider) = overflow {
            close_in_background(provider);
        }
    }
}

/// Log a session out without making the request wait for it.
pub fn close_in_background(mut provider: Box<dyn MailProvider>) {
    tokio::spawn(async move {
        let _ = provider.close().await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use mail_sync::provider::{FolderStatus, ProviderError};
    use mail_sync::session::{FetchedMessage, FolderInfo};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A session that only knows how to be closed; every real operation is
    /// an error. Enough to observe caching and closing behaviour.
    struct FakeSession {
        closed: Arc<AtomicUsize>,
    }

    fn unused<T>() -> Result<T, ProviderError> {
        Err(ProviderError::Other("unused in test".into()))
    }

    #[async_trait]
    impl MailProvider for FakeSession {
        async fn list_folders(&mut self) -> Result<Vec<FolderInfo>, ProviderError> {
            unused()
        }
        async fn folder_status(&mut self, _folder: &str) -> Result<FolderStatus, ProviderError> {
            unused()
        }
        async fn highest_uid(&mut self, _folder: &str) -> Result<Option<u32>, ProviderError> {
            unused()
        }
        async fn fetch_flags(
            &mut self,
            _folder: &str,
            _uid_set: &str,
        ) -> Result<Vec<(u32, bool, bool, bool)>, ProviderError> {
            unused()
        }
        async fn fetch_headers(
            &mut self,
            _folder: &str,
            _uid_set: &str,
        ) -> Result<Vec<FetchedMessage>, ProviderError> {
            unused()
        }
        async fn fetch_full(
            &mut self,
            _folder: &str,
            _uid_set: &str,
        ) -> Result<Vec<FetchedMessage>, ProviderError> {
            unused()
        }
        async fn fetch_raw(&mut self, _folder: &str, _uid: u32) -> Result<Vec<u8>, ProviderError> {
            unused()
        }
        async fn set_flag(
            &mut self,
            _folder: &str,
            _uid: u32,
            _flag: &str,
            _value: bool,
        ) -> Result<(), ProviderError> {
            unused()
        }
        async fn move_message(
            &mut self,
            _src_folder: &str,
            _uid: u32,
            _dest_folder: &str,
        ) -> Result<(), ProviderError> {
            unused()
        }
        async fn delete_permanently(&mut self, _folder: &str, _uid: u32) -> Result<(), ProviderError> {
            unused()
        }
        async fn append_sent(&mut self, _raw_message: &[u8]) -> Result<(), ProviderError> {
            unused()
        }
        async fn close(&mut self) -> Result<(), ProviderError> {
            self.closed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn session(closed: &Arc<AtomicUsize>) -> Box<dyn MailProvider> {
        Box::new(FakeSession {
            closed: closed.clone(),
        })
    }

    #[tokio::test]
    async fn checked_in_sessions_are_handed_out_again_instead_of_connecting() {
        let limiter = BodyFetchLimiter::new();
        let closed = Arc::new(AtomicUsize::new(0));
        limiter.checkin("acc", session(&closed));

        // Connecting with an empty IMAP config would fail, so a successful
        // checkout proves the idle session was reused.
        let reused = limiter
            .checkout("acc", ProviderKind::Imap, &ProviderConfig::default())
            .await;
        assert!(reused.is_ok());
        assert_eq!(closed.load(Ordering::SeqCst), 0, "a reused session is not closed");

        // The pool is per account: another account has nothing to reuse.
        let other = limiter
            .checkout("other", ProviderKind::Imap, &ProviderConfig::default())
            .await;
        assert!(other.is_err(), "no idle session means a real connection attempt");
    }

    #[tokio::test]
    async fn surplus_sessions_are_closed_rather_than_cached() {
        let limiter = BodyFetchLimiter::new();
        let closed = Arc::new(AtomicUsize::new(0));
        for _ in 0..=PERMITS_PER_ACCOUNT {
            limiter.checkin("acc", session(&closed));
        }
        // close() runs on a spawned task.
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(closed.load(Ordering::SeqCst), 1);

        for _ in 0..PERMITS_PER_ACCOUNT {
            assert!(limiter
                .checkout("acc", ProviderKind::Imap, &ProviderConfig::default())
                .await
                .is_ok());
        }
    }

    #[tokio::test]
    async fn limits_concurrent_fetches_per_account() {
        let limiter = BodyFetchLimiter::new();
        let _first = limiter.acquire("a").await;
        let _second = limiter.acquire("a").await;

        let third = tokio::time::timeout(Duration::from_millis(50), limiter.acquire("a")).await;
        assert!(third.is_err(), "a third fetch of the same account must wait");

        let other = tokio::time::timeout(Duration::from_millis(50), limiter.acquire("b")).await;
        assert!(other.is_ok(), "another account has its own slots");
    }

    #[tokio::test]
    async fn released_slots_are_reused() {
        let limiter = BodyFetchLimiter::new();
        let first = limiter.acquire("a").await;
        let _second = limiter.acquire("a").await;
        drop(first);

        let third = tokio::time::timeout(Duration::from_millis(50), limiter.acquire("a")).await;
        assert!(third.is_ok(), "dropping a permit frees its slot");
    }
}
