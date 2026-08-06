use aes_gcm::{AeadInPlace, Aes256Gcm, Key, KeyInit, Nonce, Tag};
use rand::RngCore;

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
    #[error("credential encryption error: {0}")]
    Encrypt(String),
    #[error("credential decryption error: {0}")]
    Decrypt(String),
    #[error("invalid ciphertext")]
    InvalidCiphertext,
}

/// AES-256-GCM credential key derived from CREDENTIAL_ENCRYPTION_KEY env var.
/// Format: [nonce(12B)][ciphertext][tag(16B)]
pub struct CredentialKey(pub [u8; 32]);

impl CredentialKey {
    /// Load from CREDENTIAL_ENCRYPTION_KEY env var (64 hex chars). Panics at startup if absent or wrong length.
    pub fn from_env() -> Self {
        let hex = std::env::var("CREDENTIAL_ENCRYPTION_KEY").unwrap_or_else(|_| {
            panic!("CREDENTIAL_ENCRYPTION_KEY env var required (64 hex chars / 32 bytes)")
        });
        Self::from_hex(&hex)
    }

    /// Build from a 64-hex-char string. Panics if invalid.
    pub fn from_hex(hex_str: &str) -> Self {
        let bytes = hex::decode(hex_str).unwrap_or_else(|_| {
            panic!("credential_encryption_key must be 64 valid hex characters")
        });
        if bytes.len() != 32 {
            panic!(
                "credential_encryption_key must be exactly 64 hex chars (32 bytes), got {} bytes",
                bytes.len()
            );
        }
        let arr: [u8; 32] = bytes.try_into().unwrap();
        Self(arr)
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let key = Key::<Aes256Gcm>::from(self.0);
        let cipher = Aes256Gcm::new(&key);

        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from(nonce_bytes);

        let mut buf = plaintext.to_vec();
        let tag = cipher
            .encrypt_in_place_detached(&nonce, b"", &mut buf)
            .map_err(|e| CryptoError::Encrypt(e.to_string()))?;

        let mut out = Vec::with_capacity(12 + buf.len() + 16);
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&buf);
        out.extend_from_slice(&tag);
        Ok(out)
    }

    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if ciphertext.len() < 28 {
            return Err(CryptoError::InvalidCiphertext);
        }
        let (nonce_bytes, rest) = ciphertext.split_at(12);
        let (ct, tag_bytes) = rest.split_at(rest.len() - 16);

        let key = Key::<Aes256Gcm>::from(self.0);
        let cipher = Aes256Gcm::new(&key);
        let nonce_arr: [u8; 12] = nonce_bytes.try_into().expect("split_at yields 12 bytes");
        let tag_arr: [u8; 16] = tag_bytes.try_into().expect("split_at yields 16 bytes");
        let nonce = Nonce::from(nonce_arr);
        let tag = Tag::from(tag_arr);

        let mut buf = ct.to_vec();
        cipher
            .decrypt_in_place_detached(&nonce, b"", &mut buf, &tag)
            .map_err(|e| CryptoError::Decrypt(e.to_string()))?;
        Ok(buf)
    }
}

/// Hash a high-entropy token with SHA-256 for storage in the DB.
pub fn hash_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Generate a random URL-safe base64 token (256 bits).
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}
