//! Generate the required cryptographic secrets without external tools
//! (equivalent to `openssl rand -hex 32`).
//!
//! - `CREDENTIAL_ENCRYPTION_KEY`: AES-256-GCM key — 32 bytes, 64 hex chars.
//! - `JWT_SECRET`: JWT signing secret — 32 random bytes, hex-encoded.

fn random_hex_32() -> String {
    let bytes: [u8; 32] = rand::random();
    hex::encode(bytes)
}

/// Print freshly generated secrets as ready-to-paste env lines.
pub fn print_generated() {
    println!("# Required secrets — add to .env or mailquill.toml");
    println!("CREDENTIAL_ENCRYPTION_KEY={}", random_hex_32());
    println!("JWT_SECRET={}", random_hex_32());
}
