//! In-memory failure counter for credential endpoints.
//!
//! Login had no brute-force protection: an attacker could try passwords for
//! an account as fast as the server hashes them. The limiter counts failed
//! attempts per key (account email, and the client address when a reverse
//! proxy forwards it) inside a fixed window and rejects further attempts once
//! the budget is spent. Successful logins clear the account's counter.

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// Window after which a key's failure count starts over.
pub const WINDOW: Duration = Duration::from_secs(15 * 60);
/// Failed logins allowed per account, and per client address, within a window.
pub const LOGIN_MAX_FAILURES: u32 = 10;
/// Registrations allowed per client address within a window.
pub const REGISTER_MAX_ATTEMPTS: u32 = 20;
/// Above this many tracked keys, expired entries are dropped on the next write.
const PRUNE_THRESHOLD: usize = 10_000;

struct Entry {
    failures: u32,
    window_start: Instant,
}

#[derive(Default)]
pub struct FailureLimiter {
    entries: Mutex<HashMap<String, Entry>>,
}

impl FailureLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `key` has reached `max` failures in the current window.
    pub fn is_blocked(&self, key: &str, max: u32) -> bool {
        let entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        entries
            .get(key)
            .is_some_and(|entry| entry.window_start.elapsed() < WINDOW && entry.failures >= max)
    }

    /// Count one failed attempt against `key`.
    pub fn record_failure(&self, key: &str) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if entries.len() > PRUNE_THRESHOLD {
            entries.retain(|_, entry| entry.window_start.elapsed() < WINDOW);
        }
        let now = Instant::now();
        let entry = entries.entry(key.to_owned()).or_insert(Entry {
            failures: 0,
            window_start: now,
        });
        if entry.window_start.elapsed() >= WINDOW {
            entry.failures = 0;
            entry.window_start = now;
        }
        entry.failures = entry.failures.saturating_add(1);
    }

    /// Forget `key` (after a successful attempt).
    pub fn clear(&self, key: &str) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(key);
    }
}

/// The client address a reverse proxy reports, if any. Direct connections
/// carry no such header; the address is then unknown and only the per-account
/// limit applies. The header is client-controlled when no proxy sets it, so it
/// is never used for anything but this best-effort throttle.
pub fn forwarded_client_ip(headers: &axum::http::HeaderMap) -> Option<String> {
    let forwarded = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|v| !v.is_empty());
    forwarded
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|v| !v.is_empty())
        })
        .map(|ip| ip.chars().take(64).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_after_max_failures_and_clears_on_success() {
        let limiter = FailureLimiter::new();
        for _ in 0..LOGIN_MAX_FAILURES {
            assert!(!limiter.is_blocked("email:a@example.test", LOGIN_MAX_FAILURES));
            limiter.record_failure("email:a@example.test");
        }
        assert!(limiter.is_blocked("email:a@example.test", LOGIN_MAX_FAILURES));
        assert!(!limiter.is_blocked("email:b@example.test", LOGIN_MAX_FAILURES));

        limiter.clear("email:a@example.test");
        assert!(!limiter.is_blocked("email:a@example.test", LOGIN_MAX_FAILURES));
    }

    #[test]
    fn forwarded_ip_takes_the_first_hop() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.9, 10.0.0.1".parse().unwrap());
        assert_eq!(forwarded_client_ip(&headers).as_deref(), Some("203.0.113.9"));
        assert_eq!(forwarded_client_ip(&axum::http::HeaderMap::new()), None);
    }
}
