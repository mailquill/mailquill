//! Per-account cap on on-demand body downloads.
//!
//! Lazy-synced messages have no stored body, so every detail request opens a
//! provider connection. The message list prefetches details on hover, so a
//! quick sweep over the rows would otherwise open one IMAP login per row and
//! run into provider connection limits (Gmail allows 15 per account, shared
//! with the sync and IDLE connections) — bodies then fail to load at all.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Concurrent provider logins allowed per account for body downloads.
const PERMITS_PER_ACCOUNT: usize = 2;

#[derive(Default)]
pub struct BodyFetchLimiter {
    per_account: Mutex<HashMap<String, Arc<Semaphore>>>,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
