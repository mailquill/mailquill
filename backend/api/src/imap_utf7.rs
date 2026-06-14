//! Decode IMAP modified UTF-7 (RFC 3501 §5.1.3) mailbox names for display,
//! e.g. "J&APw-licher" -> "Jülicher".
//!
//! The encoded form stays the wire identifier used in IMAP commands; this is
//! display-only. Invalid sequences are passed through unchanged so a malformed
//! name never panics or silently drops characters.

use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};

/// Decode a modified-UTF-7 IMAP mailbox name into a Unicode string.
pub fn decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            // A run runs from '&' to the next '-'.
            match bytes[i + 1..].iter().position(|&b| b == b'-') {
                Some(rel) => {
                    let end = i + 1 + rel; // index of the '-'
                    let chunk = &input[i + 1..end];
                    if chunk.is_empty() {
                        out.push('&'); // "&-" is a literal '&'
                    } else if let Some(decoded) = decode_run(chunk) {
                        out.push_str(&decoded);
                    } else {
                        out.push_str(&input[i..=end]); // keep raw on error
                    }
                    i = end + 1;
                }
                None => {
                    // No terminator — emit the remainder verbatim.
                    out.push_str(&input[i..]);
                    break;
                }
            }
        } else {
            let ch = input[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Decode one modified-BASE64 run (UTF-16BE; '/' is written as ',').
fn decode_run(chunk: &str) -> Option<String> {
    let std: String = chunk.chars().map(|c| if c == ',' { '/' } else { c }).collect();
    let bytes = STANDARD_NO_PAD.decode(std.as_bytes()).ok()?;
    if bytes.len() % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .collect();
    String::from_utf16(&units).ok()
}
