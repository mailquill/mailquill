//! Library surface of the `api` crate.
//!
//! Modules live here (rather than only in `main.rs`) so that integration tests
//! under `tests/` can exercise the public API. The binary (`src/main.rs`)
//! consumes these modules via the `api::` crate path.

pub mod config;
pub mod error;
pub mod middleware;
pub mod oauth_tokens;
pub mod routes;
pub mod secrets;
pub mod sieve;
pub mod state;
pub mod sync_impl;
pub mod validate;
pub mod vapid_keys;
