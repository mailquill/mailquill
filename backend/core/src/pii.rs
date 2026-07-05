use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};

const MODE_REMOVE: u8 = 0;
const MODE_HASH: u8 = 1;
const MODE_ENCRYPT: u8 = 2;
const MODE_PLAINTEXT: u8 = 3;

static PII_MODE: AtomicU8 = AtomicU8::new(MODE_REMOVE);
static PII_ENCRYPT_KEY: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();

/// Initialise PII mode from `LOG_PII_MODE` env var. Call once at startup.
pub fn init_pii_mode() {
    let mode_str = std::env::var("LOG_PII_MODE").unwrap_or_else(|_| "remove".into());
    let mode = match mode_str.as_str() {
        "remove" => MODE_REMOVE,
        "hash" => MODE_HASH,
        "encrypt" => {
            let hex_key = std::env::var("LOG_PII_KEY").unwrap_or_else(|_| {
                panic!("LOG_PII_MODE=encrypt requires LOG_PII_KEY (64 hex chars / 32 bytes)")
            });
            let key_bytes = hex::decode(&hex_key).unwrap_or_else(|_| {
                panic!("LOG_PII_KEY must be 64 valid hex characters (32 bytes)")
            });
            if key_bytes.len() != 32 {
                panic!(
                    "LOG_PII_KEY must be exactly 64 hex chars (32 bytes), got {} bytes",
                    key_bytes.len()
                );
            }
            let arr: [u8; 32] = key_bytes.try_into().unwrap();
            PII_ENCRYPT_KEY.set(arr).ok();
            MODE_ENCRYPT
        }
        "plaintext" => {
            eprintln!("WARNING: LOG_PII_MODE=plaintext — PII will appear in logs unredacted");
            MODE_PLAINTEXT
        }
        other => {
            eprintln!("WARNING: unknown LOG_PII_MODE={other}, defaulting to remove");
            MODE_REMOVE
        }
    };
    PII_MODE.store(mode, Ordering::Relaxed);
}

/// Wraps a PII value for logging. Display/Debug redact according to `LOG_PII_MODE`.
pub struct Pii<'a, T: fmt::Display>(pub &'a T);

impl<T: fmt::Display> fmt::Display for Pii<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match PII_MODE.load(Ordering::Relaxed) {
            MODE_REMOVE => f.write_str("[REDACTED]"),
            MODE_HASH => {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(self.0.to_string().as_bytes());
                let result = hasher.finalize();
                write!(f, "pii:sha256:{}", hex::encode(&result[..8]))
            }
            MODE_ENCRYPT => {
                use aes_gcm::{AeadInPlace, KeyInit, Nonce};
                use rand::RngCore;
                if let Some(key) = PII_ENCRYPT_KEY.get() {
                    let cipher = aes_gcm::Aes256Gcm::new_from_slice(key.as_slice()).unwrap();
                    let mut nonce_bytes = [0u8; 12];
                    rand::thread_rng().fill_bytes(&mut nonce_bytes);
                    let nonce = Nonce::from_slice(&nonce_bytes);
                    let mut buf = self.0.to_string().into_bytes();
                    let tag = cipher
                        .encrypt_in_place_detached(nonce, b"", &mut buf)
                        .unwrap();
                    let mut out = Vec::with_capacity(12 + buf.len() + 16);
                    out.extend_from_slice(&nonce_bytes);
                    out.extend_from_slice(&buf);
                    out.extend_from_slice(&tag);
                    write!(
                        f,
                        "pii:enc:{}",
                        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &out)
                    )
                } else {
                    f.write_str("[REDACTED]")
                }
            }
            _ => fmt::Display::fmt(self.0, f),
        }
    }
}

impl<T: fmt::Display> fmt::Debug for Pii<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_mode_redacts() {
        PII_MODE.store(MODE_REMOVE, Ordering::Relaxed);
        assert_eq!(Pii(&"user@example.com").to_string(), "[REDACTED]");
    }

    #[test]
    fn hash_mode_is_stable() {
        PII_MODE.store(MODE_HASH, Ordering::Relaxed);
        let a = Pii(&"user@example.com").to_string();
        let b = Pii(&"user@example.com").to_string();
        assert_eq!(a, b);
        assert!(a.starts_with("pii:sha256:"));
    }

    #[test]
    fn plaintext_mode_returns_raw() {
        PII_MODE.store(MODE_PLAINTEXT, Ordering::Relaxed);
        assert_eq!(Pii(&"user@example.com").to_string(), "user@example.com");
        PII_MODE.store(MODE_REMOVE, Ordering::Relaxed);
    }
}
