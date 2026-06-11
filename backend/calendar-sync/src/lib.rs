//! CalDAV read-sync: fetch iCalendar VEVENTs from a CalDAV collection and parse
//! them into a lightweight event model. Discovery follows RFC 4791/6764
//! (current-user-principal → calendar-home-set → calendar collection).

use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::Method;
use url::Url;

#[derive(Clone)]
pub enum DavAuth {
    Basic { username: String, password: String },
    Bearer(String),
}

#[derive(Debug, Clone)]
pub struct ParsedEvent {
    pub title: String,
    pub starts_at: String,
    pub ends_at: String,
    pub all_day: bool,
    pub location: Option<String>,
    pub description: Option<String>,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())
}

fn apply_auth(rb: reqwest::RequestBuilder, auth: &DavAuth) -> reqwest::RequestBuilder {
    match auth {
        DavAuth::Basic { username, password } => rb.basic_auth(username, Some(password)),
        DavAuth::Bearer(token) => rb.bearer_auth(token),
    }
}

async fn dav(
    c: &reqwest::Client,
    method: &str,
    url: &str,
    auth: &DavAuth,
    depth: &str,
    body: &str,
) -> Result<String, String> {
    let m = Method::from_bytes(method.as_bytes()).map_err(|e| e.to_string())?;
    let rb = c
        .request(m, url)
        .header("Depth", depth)
        .header("Content-Type", "application/xml; charset=utf-8")
        .body(body.to_owned());
    let resp = apply_auth(rb, auth).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() && status.as_u16() != 207 {
        return Err(format!("CalDAV {method} {url} -> {status}"));
    }
    Ok(text)
}

/// Fetch and parse all events from a CalDAV account.
pub async fn sync_caldav(base_url: &str, auth: &DavAuth) -> Result<Vec<ParsedEvent>, String> {
    let c = client()?;
    let calendar = discover_calendar(&c, base_url, auth)
        .await
        .unwrap_or_else(|| base_url.to_owned());

    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop><D:getetag/><C:calendar-data/></D:prop>
  <C:filter><C:comp-filter name="VCALENDAR"><C:comp-filter name="VEVENT"/></C:comp-filter></C:filter>
</C:calendar-query>"#;

    let xml = dav(&c, "REPORT", &calendar, auth, "1", body).await?;
    let mut events = Vec::new();
    for ics in extract_texts(&xml, b"calendar-data") {
        events.extend(parse_vevents(&ics));
    }
    Ok(events)
}

async fn discover_calendar(c: &reqwest::Client, base: &str, auth: &DavAuth) -> Option<String> {
    let principal_xml = dav(
        c,
        "PROPFIND",
        base,
        auth,
        "0",
        r#"<d:propfind xmlns:d="DAV:"><d:prop><d:current-user-principal/></d:prop></d:propfind>"#,
    )
    .await
    .ok()?;
    let principal = resolve(base, &first_href_in_elem(&principal_xml, b"current-user-principal")?)?;

    let home_xml = dav(
        c,
        "PROPFIND",
        &principal,
        auth,
        "0",
        r#"<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop><c:calendar-home-set/></d:prop></d:propfind>"#,
    )
    .await
    .ok()?;
    let home = resolve(base, &first_href_in_elem(&home_xml, b"calendar-home-set")?)?;

    let list_xml = dav(
        c,
        "PROPFIND",
        &home,
        auth,
        "1",
        r#"<d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/></d:prop></d:propfind>"#,
    )
    .await
    .ok()?;
    let href = first_href_with_resourcetype(&list_xml, b"calendar")?;
    resolve(base, &href)
}

// ── iCalendar parsing ──────────────────────────────────────────────────────

fn parse_vevents(ics: &str) -> Vec<ParsedEvent> {
    let unfolded = unfold(ics);
    let mut events = Vec::new();
    let mut in_event = false;
    let mut title: Option<String> = None;
    let mut start: Option<String> = None;
    let mut end: Option<String> = None;
    let mut all_day = false;
    let mut loc: Option<String> = None;
    let mut desc: Option<String> = None;

    for line in unfolded.lines() {
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("BEGIN:VEVENT") {
            in_event = true;
            title = None;
            start = None;
            end = None;
            all_day = false;
            loc = None;
            desc = None;
            continue;
        }
        if upper.starts_with("END:VEVENT") {
            if let (Some(t), Some(s)) = (title.take(), start.take()) {
                let e: String = end.take().unwrap_or_else(|| s.clone());
                events.push(ParsedEvent {
                    title: t,
                    starts_at: s,
                    ends_at: e,
                    all_day,
                    location: loc.take(),
                    description: desc.take(),
                });
            }
            in_event = false;
            continue;
        }
        if !in_event {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else { continue };
        let prop = key.split(';').next().unwrap_or("").to_ascii_uppercase();
        let value = value.trim();
        match prop.as_str() {
            "SUMMARY" => title = Some(unescape_ical(value)),
            "DTSTART" => {
                let (iso, date_only) = ical_to_iso(value);
                if date_only || key.to_ascii_uppercase().contains("VALUE=DATE") {
                    all_day = true;
                }
                start = Some(iso);
            }
            "DTEND" => {
                let (iso, _) = ical_to_iso(value);
                end = Some(iso);
            }
            "LOCATION" => loc = Some(unescape_ical(value)),
            "DESCRIPTION" => desc = Some(unescape_ical(value)),
            _ => {}
        }
    }
    events
}

/// Convert a compact iCal date/datetime to an ISO-8601-ish string. Returns
/// (value, is_date_only).
fn ical_to_iso(v: &str) -> (String, bool) {
    let s = v.trim();
    let digit_count = s.chars().take_while(|c| c.is_ascii_digit()).count();
    if s.len() == 8 && digit_count == 8 {
        return (format!("{}-{}-{}T00:00:00", &s[0..4], &s[4..6], &s[6..8]), true);
    }
    if s.len() >= 15 && s.as_bytes().get(8) == Some(&b'T') {
        let z = if s.ends_with('Z') { "Z" } else { "" };
        return (
            format!(
                "{}-{}-{}T{}:{}:{}{}",
                &s[0..4],
                &s[4..6],
                &s[6..8],
                &s[9..11],
                &s[11..13],
                &s[13..15],
                z
            ),
            false,
        );
    }
    (s.to_owned(), false)
}

fn unescape_ical(v: &str) -> String {
    v.replace("\\n", "\n").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\")
}

fn unfold(raw: &str) -> String {
    let mut out = String::new();
    for line in raw.replace("\r\n", "\n").split('\n') {
        if line.starts_with(' ') || line.starts_with('\t') {
            out.push_str(line.trim_start());
        } else {
            out.push('\n');
            out.push_str(line);
        }
    }
    out
}

// ── XML helpers (namespace-agnostic via local names) ───────────────────────

fn local_name(qname: &[u8]) -> &[u8] {
    match qname.iter().rposition(|&b| b == b':') {
        Some(i) => &qname[i + 1..],
        None => qname,
    }
}

fn extract_texts(xml: &str, target: &[u8]) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut buf = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                if local_name(e.name().as_ref()) == target {
                    depth += 1;
                    if depth == 1 {
                        buf.clear();
                    }
                }
            }
            Ok(Event::Text(e)) if depth > 0 => {
                if let Ok(t) = e.unescape() {
                    buf.push_str(&t);
                }
            }
            Ok(Event::CData(e)) if depth > 0 => {
                buf.push_str(&String::from_utf8_lossy(&e.into_inner()));
            }
            Ok(Event::End(e)) => {
                if local_name(e.name().as_ref()) == target && depth > 0 {
                    depth -= 1;
                    if depth == 0 {
                        out.push(std::mem::take(&mut buf));
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

fn first_href_in_elem(xml: &str, target: &[u8]) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut in_target = 0i32;
    let mut in_href = false;
    let mut buf = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let ln = local_name(name.as_ref());
                if ln == target {
                    in_target += 1;
                } else if in_target > 0 && ln == b"href" {
                    in_href = true;
                    buf.clear();
                }
            }
            Ok(Event::Text(e)) if in_href => {
                if let Ok(t) = e.unescape() {
                    buf.push_str(&t);
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let ln = local_name(name.as_ref());
                if ln == b"href" && in_href {
                    return Some(buf.trim().to_owned());
                }
                if ln == target && in_target > 0 {
                    in_target -= 1;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    None
}

fn first_href_with_resourcetype(xml: &str, rtype: &[u8]) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let (mut in_response, mut in_href, mut in_rtype) = (false, false, 0i32);
    let mut matched = false;
    let mut href: Option<String> = None;
    let mut buf = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let ln = local_name(name.as_ref());
                if ln == b"response" {
                    in_response = true;
                    matched = false;
                    href = None;
                } else if in_response && ln == b"href" && href.is_none() {
                    in_href = true;
                    buf.clear();
                } else if ln == b"resourcetype" {
                    in_rtype += 1;
                } else if in_rtype > 0 && ln == rtype {
                    matched = true;
                }
            }
            Ok(Event::Empty(e)) => {
                if in_rtype > 0 && local_name(e.name().as_ref()) == rtype {
                    matched = true;
                }
            }
            Ok(Event::Text(e)) if in_href => {
                if let Ok(t) = e.unescape() {
                    buf.push_str(&t);
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let ln = local_name(name.as_ref());
                if ln == b"href" && in_href {
                    href = Some(buf.trim().to_owned());
                    in_href = false;
                } else if ln == b"resourcetype" && in_rtype > 0 {
                    in_rtype -= 1;
                } else if ln == b"response" {
                    in_response = false;
                    if matched {
                        if let Some(h) = href.take() {
                            return Some(h);
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    None
}

fn resolve(base: &str, href: &str) -> Option<String> {
    Url::parse(base).ok()?.join(href).ok().map(|u| u.to_string())
}
