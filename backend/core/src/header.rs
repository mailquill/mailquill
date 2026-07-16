/// Decode RFC 2047 encoded-words (for example `=?UTF-8?B?...?=`) in a
/// header value while tolerating malformed boundaries emitted by some mail
/// clients and IMAP servers.
pub fn decode_header_words(value: &str) -> String {
    let mut repaired = strip_decoded_word_boundary_artifacts(value);

    // Some IMAP ENVELOPE implementations drop the first '=' from a single
    // encoded word. It is unambiguous when the complete value has the
    // RFC-2047 shape, so restore it before decoding.
    let lower = repaired.to_ascii_lowercase();
    if repaired.starts_with('?')
        && repaired.ends_with("?=")
        && (lower.contains("?q?") || lower.contains("?b?"))
    {
        repaired.insert(0, '=');
    }

    // RFC 2047 requires linear whitespace between adjacent encoded words,
    // but common senders omit it. mailparse otherwise decodes the payload
    // while leaking the boundary into the visible value.
    repaired = repaired.replace("?==?", "?= =?");
    if !repaired.contains("=?") {
        return repaired;
    }

    match mailparse::parse_header(format!("X: {repaired}").as_bytes()) {
        Ok((header, _)) => header.get_value(),
        Err(_) => repaired,
    }
}

fn strip_decoded_word_boundary_artifacts(value: &str) -> String {
    let mut repaired = value.to_owned();
    loop {
        let lower = repaired.to_ascii_lowercase();
        let Some(start) = lower.find("?==?") else {
            break;
        };
        let charset_start = start + 4;
        let Some(charset_len) = lower[charset_start..].find('?') else {
            break;
        };
        let encoding_start = charset_start + charset_len + 1;
        let bytes = lower.as_bytes();
        let valid_encoding = matches!(bytes.get(encoding_start), Some(b'q' | b'b'))
            && bytes.get(encoding_start + 1) == Some(&b'?');
        let charset = &lower[charset_start..charset_start + charset_len];
        if charset.is_empty()
            || !charset
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            || !valid_encoding
        {
            break;
        }
        repaired.replace_range(start..encoding_start + 2, "");
    }
    repaired
}

#[cfg(test)]
mod tests {
    use super::decode_header_words;

    #[test]
    fn decodes_adjacent_encoded_words_without_whitespace() {
        assert_eq!(
            decode_header_words(
                "=?utf-8?q?Erneuerung_Terasse_-_J=C3=BClicher?==?utf-8?q?_Str._311?="
            ),
            "Erneuerung Terasse - Jülicher Str. 311"
        );
    }

    #[test]
    fn removes_already_decoded_boundary_artifact() {
        assert_eq!(
            decode_header_words("Erneuerung Terasse - Jülicher?==?utf-8?q? Str. 311"),
            "Erneuerung Terasse - Jülicher Str. 311"
        );
    }

    #[test]
    fn repairs_missing_leading_equals_on_encoded_word() {
        assert_eq!(decode_header_words("?utf-8?B?R3LDvMOfZQ==?="), "Grüße");
    }
}
