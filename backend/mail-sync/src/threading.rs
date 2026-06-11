use sha2::{Digest, Sha256};

/// Normalize subject by stripping reply/forward prefixes (task 5.4).
pub fn normalize_subject(subject: &str) -> String {
    let prefixes = [
        "re:", "fwd:", "aw:", "fwd:", "sv:", "sv:", "vs:", "RE:", "FWD:", "AW:", "FWD:", "SV:",
        "Sv:", "Vs:",
    ];
    let mut s = subject.trim().to_owned();
    loop {
        let lower = s.to_lowercase();
        let mut stripped = false;
        for prefix in &prefixes {
            let p = prefix.to_lowercase();
            if lower.starts_with(&p) {
                s = s[p.len()..].trim().to_owned();
                stripped = true;
                break;
            }
        }
        if !stripped {
            break;
        }
    }
    s
}

/// Compute thread_id from root message_id (tasks 5.1, 5.2).
/// thread_id = hex(sha256(root_message_id)[..8])
pub fn thread_id_from_message_id(root_message_id: &str) -> String {
    let hash = Sha256::digest(root_message_id.as_bytes());
    hex::encode(&hash[..8])
}

/// Compute thread_id for mailing list messages (task 5.3).
/// thread_id = hex(sha256(list_id + ':' + subject_normalized)[..8])
pub fn thread_id_from_list(list_id: &str, subject_normalized: &str) -> String {
    let key = format!("{list_id}:{subject_normalized}");
    let hash = Sha256::digest(key.as_bytes());
    hex::encode(&hash[..8])
}

/// JWZ fallback: find the root message_id by walking References chain.
/// Given `in_reply_to` and `references` headers, returns the root message-id.
pub fn jwz_root(
    _message_id: &str,
    in_reply_to: Option<&str>,
    references: Option<&str>,
    existing_thread_ids: &std::collections::HashMap<String, String>,
) -> Option<String> {
    // If this message references an existing thread, use that thread's root
    if let Some(refs) = references {
        let ref_ids: Vec<&str> = refs.split_whitespace().collect();
        // Walk from oldest (leftmost) to newest
        for ref_id in &ref_ids {
            let clean = ref_id.trim_matches(|c| c == '<' || c == '>');
            if let Some(tid) = existing_thread_ids.get(clean) {
                return Some(tid.clone());
            }
        }
        // Use leftmost reference as root
        if let Some(first) = ref_ids.first() {
            let clean = first.trim_matches(|c| c == '<' || c == '>');
            return Some(thread_id_from_message_id(clean));
        }
    }

    if let Some(irt) = in_reply_to {
        let clean = irt.trim_matches(|c| c == '<' || c == '>');
        if let Some(tid) = existing_thread_ids.get(clean) {
            return Some(tid.clone());
        }
        return Some(thread_id_from_message_id(clean));
    }

    None
}

/// Assign thread_id to a message given its headers and existing thread state.
pub fn assign_thread_id(
    message_id: Option<&str>,
    in_reply_to: Option<&str>,
    references: Option<&str>,
    list_id: Option<&str>,
    subject: &str,
    existing_thread_ids: &std::collections::HashMap<String, String>,
) -> String {
    // Try JWZ first
    if let Some(tid) = jwz_root(
        message_id.unwrap_or(""),
        in_reply_to,
        references,
        existing_thread_ids,
    ) {
        return tid;
    }

    // Mailing list fallback
    if let Some(lid) = list_id {
        let subject_norm = normalize_subject(subject);
        return thread_id_from_list(lid, &subject_norm);
    }

    // No thread — use own message_id as root
    if let Some(mid) = message_id {
        return thread_id_from_message_id(mid);
    }

    // Last resort: random-ish (shouldn't happen)
    thread_id_from_message_id(&format!("{}:{}", subject, chrono::Utc::now().timestamp()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_re_prefix() {
        assert_eq!(normalize_subject("Re: Hello"), "Hello");
        assert_eq!(normalize_subject("RE: Re: Hello"), "Hello");
        assert_eq!(normalize_subject("FWD: Hello"), "Hello");
        assert_eq!(normalize_subject("Fwd: Re: Hello"), "Hello");
        assert_eq!(normalize_subject("AW: Something"), "Something");
    }

    #[test]
    fn thread_id_deterministic() {
        let a = thread_id_from_message_id("<abc@example.com>");
        let b = thread_id_from_message_id("<abc@example.com>");
        assert_eq!(a, b);
        assert_eq!(a.len(), 16); // 8 bytes = 16 hex chars
    }
}
