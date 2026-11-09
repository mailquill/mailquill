//! Shared building blocks for the Mailquill backend: blob storage, body
//! compression, credential encryption, JWT handling, header decoding,
//! PII-aware logging and TLS client configuration.

pub mod blob;
pub mod compression;
pub mod crypto;
pub mod header;
pub mod jwt;
pub mod pii;
pub mod services;
pub mod tls;
