//! Shared helpers for the API-based providers.

use crate::session::FetchedMessage;
use mailparse::MailHeaderMap;

/// Build a `FetchedMessage` from a raw RFC 2822 message (or bare header
/// block). `body` keeps the full raw message when `include_body` is set,
/// otherwise just the header block — mirroring what the IMAP fetches produce,
/// so snippet/blob/phishing handling downstream stays identical.
pub fn fetched_from_raw(
    uid: u32,
    raw: &[u8],
    internal_date: String,
    is_seen: bool,
    is_flagged: bool,
    include_body: bool,
) -> FetchedMessage {
    let mut msg = FetchedMessage {
        uid,
        internal_date,
        is_seen,
        is_flagged,
        ..Default::default()
    };

    if let Ok((headers, _)) = mailparse::parse_headers(raw) {
        let h = |name: &str| headers.get_first_value(name);
        msg.subject = h("Subject").unwrap_or_default();
        msg.from_addr = h("From").unwrap_or_default();
        msg.to_addrs = h("To").unwrap_or_default();
        msg.cc_addrs = h("Cc").unwrap_or_default();
        msg.date = h("Date");
        msg.message_id = h("Message-ID").or_else(|| h("Message-Id"));
        msg.in_reply_to = h("In-Reply-To");
        msg.references = h("References");
        msg.list_id = h("List-Id");
    }

    msg.body = Some(if include_body {
        raw.to_vec()
    } else {
        header_block(raw).to_vec()
    });
    msg
}

/// The header block of a raw message (everything before the blank line).
fn header_block(raw: &[u8]) -> &[u8] {
    if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
        &raw[..pos + 4]
    } else if let Some(pos) = raw.windows(2).position(|w| w == b"\n\n") {
        &raw[..pos + 2]
    } else {
        raw
    }
}

/// Render an API header list (`[{name, value}]`-style pairs) into an RFC 2822
/// header block for phishing analysis and `fetched_from_raw`.
pub fn header_block_from_pairs<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>) -> Vec<u8> {
    let mut out = String::new();
    for (name, value) in pairs {
        out.push_str(name);
        out.push_str(": ");
        out.push_str(value);
        out.push_str("\r\n");
    }
    out.push_str("\r\n");
    out.into_bytes()
}

/// Epoch milliseconds → RFC 3339 string (Gmail `internalDate`).
pub fn rfc3339_from_millis(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339()
}
