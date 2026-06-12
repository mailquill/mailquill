/// MIME parsing utilities (task 4.8).
/// Extracts text/plain, text/html, and attachments from RFC 2822 messages.
use mailparse::{MailHeaderMap, ParsedMail};

#[derive(Debug, Default)]
pub struct ParsedBody {
    pub text: Option<String>,
    pub html: Option<String>,
    pub attachments: Vec<Attachment>,
    pub has_calendar: bool,
}

#[derive(Debug)]
pub struct Attachment {
    pub filename: Option<String>,
    pub content_type: String,
    /// MIME Content-ID (without angle brackets) for inline parts that the
    /// HTML body references as `cid:` URLs.
    pub content_id: Option<String>,
    pub data: Vec<u8>,
}

/// Parse a raw RFC 2822 email and extract body parts.
pub fn parse_mime(raw: &[u8]) -> Result<ParsedBody, String> {
    let parsed = mailparse::parse_mail(raw).map_err(|e| e.to_string())?;
    let mut body = ParsedBody::default();
    walk_parts(&parsed, &mut body);
    Ok(body)
}

fn walk_parts(part: &ParsedMail, body: &mut ParsedBody) {
    let ct = part.ctype.mimetype.to_lowercase();

    if ct.starts_with("multipart/") {
        for sub in &part.subparts {
            walk_parts(sub, body);
        }
        return;
    }

    let disposition = part
        .get_headers()
        .get_first_value("Content-Disposition")
        .unwrap_or_default()
        .to_lowercase();

    let content_id = part
        .get_headers()
        .get_first_value("Content-ID")
        .map(|v| v.trim().trim_start_matches('<').trim_end_matches('>').to_owned())
        .filter(|v| !v.is_empty());

    // Inline images often carry only a Content-ID, no Content-Disposition —
    // they must still be stored so cid: references in the HTML resolve.
    let is_attachment = disposition.starts_with("attachment")
        || (disposition.starts_with("inline") && !ct.starts_with("text/"))
        || (content_id.is_some() && !ct.starts_with("text/") && !ct.starts_with("multipart/"));

    if is_attachment {
        let filename = part
            .get_headers()
            .get_first_value("Content-Disposition")
            .and_then(|d| {
                d.split(';').find_map(|p| {
                    let p = p.trim();
                    if p.starts_with("filename=") || p.starts_with("filename*=") {
                        Some(
                            p.splitn(2, '=')
                                .nth(1)
                                .unwrap_or("")
                                .trim_matches('"')
                                .to_owned(),
                        )
                    } else {
                        None
                    }
                })
            })
            .or_else(|| part.ctype.params.get("name").cloned());

        let data = part.get_body_raw().unwrap_or_default();
        body.attachments.push(Attachment {
            filename,
            content_type: ct.clone(),
            content_id,
            data,
        });
        return;
    }

    if ct == "text/calendar" {
        body.has_calendar = true;
    }

    match ct.as_str() {
        "text/plain" => {
            if body.text.is_none() {
                let text = part.get_body().unwrap_or_default();
                body.text = Some(text);
            }
        }
        "text/html" => {
            if body.html.is_none() {
                let html = part.get_body().unwrap_or_default();
                body.html = Some(html);
            }
        }
        _ => {}
    }
}

/// Pre-compute a 160-char snippet from subject and body text (task 4.7).
pub fn compute_snippet(subject: &str, body_text: Option<&str>) -> String {
    let text = body_text.unwrap_or_default().trim();
    let combined = if !text.is_empty() {
        format!("{subject}: {text}")
    } else {
        subject.to_owned()
    };
    let snippet: String = combined
        .chars()
        .filter(|c| !c.is_control())
        .take(160)
        .collect();
    snippet
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_truncates() {
        let long = "x".repeat(200);
        let s = compute_snippet("subj", Some(&long));
        assert!(s.len() <= 160);
    }
}
