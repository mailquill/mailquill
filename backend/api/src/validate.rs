//! Zero-trust input validation.
//!
//! Every request field that crosses the API boundary is treated as hostile and
//! validated here on the server, independent of any client-side checks. Failures
//! return `AppError::Unprocessable` (HTTP 422) with a field-scoped message that
//! never echoes back the offending value (avoids reflected-content surprises).

use crate::error::AppError;

/// Auth schemes the backend is willing to attempt. Anything else is rejected.
pub const AUTH_SCHEMES: &[&str] = &["plain", "login", "cram-md5", "oauth2", "xoauth2"];
pub const BODY_SYNC_MODES: &[&str] = &["lazy", "full"];

fn reject(field: &str, why: &str) -> AppError {
    AppError::Unprocessable(format!("{field} {why}"))
}

fn no_control(field: &str, value: &str) -> Result<(), AppError> {
    // Tab/newline are control chars too; none belong in any of these fields.
    if value.chars().any(|c| c.is_control()) {
        return Err(reject(field, "must not contain control characters"));
    }
    Ok(())
}

/// Required free text: non-empty after trimming, bounded length, no control chars.
pub fn text(field: &str, value: &str, max: usize) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(reject(field, "must not be empty"));
    }
    if value.chars().count() > max {
        return Err(reject(field, &format!("must be at most {max} characters")));
    }
    no_control(field, value)
}

/// Optional free text: bounded length and no control chars (may be empty).
#[allow(dead_code)] // shared helper for routes hardened under the zero-trust pass
pub fn opt_text(field: &str, value: &str, max: usize) -> Result<(), AppError> {
    if value.chars().count() > max {
        return Err(reject(field, &format!("must be at most {max} characters")));
    }
    no_control(field, value)
}

/// Email address. Conservative structural check on the raw value: no surrounding
/// or embedded whitespace, exactly one `@`, non-empty local part, a dotted domain
/// using a restricted character set.
pub fn email(field: &str, value: &str) -> Result<(), AppError> {
    if value.is_empty() || value.len() > 320 {
        return Err(reject(field, "must be a valid email address"));
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(reject(field, "must not contain whitespace"));
    }
    let mut parts = value.split('@');
    let (local, domain) = match (parts.next(), parts.next(), parts.next()) {
        (Some(l), Some(d), None) => (l, d),
        _ => return Err(reject(field, "must contain exactly one @")),
    };
    if local.is_empty() || domain.is_empty() {
        return Err(reject(field, "must be a valid email address"));
    }
    if !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') || domain.starts_with('-') {
        return Err(reject(field, "must have a valid domain"));
    }
    if !domain.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return Err(reject(field, "domain contains invalid characters"));
    }
    Ok(())
}

/// Hostname or IPv4 literal used to open a network connection. Restricted charset,
/// bounded length, no whitespace — keeps obviously malformed/injection-y values out
/// before they reach the IMAP/SMTP client.
pub fn host(field: &str, value: &str) -> Result<(), AppError> {
    if value.is_empty() || value.len() > 255 {
        return Err(reject(field, "must be 1-255 characters"));
    }
    if !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        return Err(reject(field, "must be a valid hostname"));
    }
    if value.starts_with('.') || value.ends_with('.') || value.starts_with('-') {
        return Err(reject(field, "must be a valid hostname"));
    }
    Ok(())
}

/// TCP port: reject 0 (the `u16` type already caps the upper bound).
pub fn port(field: &str, value: u16) -> Result<(), AppError> {
    if value == 0 {
        return Err(reject(field, "must be between 1 and 65535"));
    }
    Ok(())
}

/// Value must be a member of an allow-list.
pub fn one_of(field: &str, value: &str, allowed: &[&str]) -> Result<(), AppError> {
    if !allowed.contains(&value) {
        return Err(reject(field, &format!("must be one of: {}", allowed.join(", "))));
    }
    Ok(())
}

/// http(s) URL: scheme-restricted, bounded length, no whitespace/control chars.
pub fn http_url(field: &str, value: &str) -> Result<(), AppError> {
    if value.len() > 2048 {
        return Err(reject(field, "must be at most 2048 characters"));
    }
    if !(value.starts_with("http://") || value.starts_with("https://")) {
        return Err(reject(field, "must be an http(s) URL"));
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(reject(field, "must not contain whitespace"));
    }
    Ok(())
}

/// Base64-encoded DER certificate (TLS trust exception). Decodes to verify
/// well-formedness and caps the size; returns the decoded bytes.
pub fn b64_cert(field: &str, value: &str) -> Result<Vec<u8>, AppError> {
    use base64::Engine;
    if value.is_empty() || value.len() > 32_768 {
        return Err(reject(field, "must be a base64 certificate up to 32768 characters"));
    }
    let der = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .map_err(|_| reject(field, "must be valid base64"))?;
    if der.len() < 64 {
        return Err(reject(field, "is too short to be a DER certificate"));
    }
    Ok(der)
}

/// Integer within an inclusive range.
pub fn range_i64(field: &str, value: i64, min: i64, max: i64) -> Result<(), AppError> {
    if value < min || value > max {
        return Err(reject(field, &format!("must be between {min} and {max}")));
    }
    Ok(())
}
