//! Generate a VAPID (Web Push) key pair without external tools.
//!
//! Output matches `web-push`/browser expectations: a P-256 key pair encoded as
//! URL-safe base64 without padding — the private key is the 32-byte scalar, the
//! public key the 65-byte uncompressed EC point. Equivalent to
//! `npx web-push generate-vapid-keys`, but built in: `mailquill vapid-keys`.

use base64::Engine;
use p256::elliptic_curve::sec1::ToSec1Point;
use p256::elliptic_curve::Generate;
use p256::SecretKey;

pub struct VapidKeyPair {
    pub public_key: String,
    pub private_key: String,
}

pub fn generate() -> VapidKeyPair {
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let secret = SecretKey::try_generate().expect("system RNG unavailable");
    VapidKeyPair {
        private_key: b64.encode(secret.to_bytes()),
        public_key: b64.encode(secret.public_key().to_sec1_point(false).as_bytes()),
    }
}

/// Print a generated key pair as ready-to-paste env lines.
pub fn print_generated() {
    let keys = generate();
    println!("# Web Push (VAPID) keys — add to .env or mailquill.toml");
    println!("VAPID_PUBLIC_KEY={}", keys.public_key);
    println!("VAPID_PRIVATE_KEY={}", keys.private_key);
    println!("VAPID_SUBJECT=mailto:admin@example.com");
}
