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

#[derive(Debug, Clone, Default)]
pub struct ParsedEvent {
    /// iCalendar UID — stable identity across sync runs.
    pub uid: String,
    /// Server resource path (set by the pull, not present in the VEVENT body).
    pub href: Option<String>,
    /// Server ETag for optimistic concurrency on update/delete.
    pub etag: Option<String>,
    pub title: String,
    pub starts_at: String,
    pub ends_at: String,
    pub all_day: bool,
    pub location: Option<String>,
    pub description: Option<String>,
}

/// Fields needed to render a VEVENT we push to the server.
pub struct EventInput<'a> {
    pub uid: &'a str,
    pub title: &'a str,
    pub starts_at: &'a str,
    pub ends_at: &'a str,
    pub all_day: bool,
    pub location: Option<&'a str>,
    pub description: Option<&'a str>,
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

/// Discover the default calendar collection URL for an account, falling back to
/// the base URL if discovery fails.
pub async fn discover_collection(base_url: &str, auth: &DavAuth) -> Result<String, String> {
    let c = client()?;
    Ok(discover_calendar(&c, base_url, auth)
        .await
        .unwrap_or_else(|| base_url.to_owned()))
}

/// Fetch all events from a CalDAV collection. Each event carries its server
/// `href` and `etag` so the caller can match, update, and delete resources.
pub async fn pull(collection_url: &str, auth: &DavAuth) -> Result<Vec<ParsedEvent>, String> {
    let c = client()?;
    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop><D:getetag/><C:calendar-data/></D:prop>
  <C:filter><C:comp-filter name="VCALENDAR"><C:comp-filter name="VEVENT"/></C:comp-filter></C:filter>
</C:calendar-query>"#;

    let xml = dav(&c, "REPORT", collection_url, auth, "1", body).await?;
    let mut events = Vec::new();
    for (href, etag, ics) in parse_calendar_responses(&xml) {
        let abs_href = resolve(collection_url, &href).unwrap_or(href);
        for mut ev in parse_vevents(&ics) {
            ev.href = Some(abs_href.clone());
            ev.etag = etag.clone();
            events.push(ev);
        }
    }
    Ok(events)
}

/// Create or update an event resource (PUT). Returns the new ETag if the server
/// reports one. `if_match` enables optimistic concurrency on update.
pub async fn put_event(
    resource_url: &str,
    auth: &DavAuth,
    ics: &str,
    if_match: Option<&str>,
) -> Result<Option<String>, String> {
    let c = client()?;
    let mut rb = c
        .request(Method::PUT, resource_url)
        .header("Content-Type", "text/calendar; charset=utf-8")
        .body(ics.to_owned());
    if let Some(tag) = if_match {
        rb = rb.header("If-Match", tag);
    }
    let resp = apply_auth(rb, auth).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("CalDAV PUT {resource_url} -> {status}"));
    }
    let etag = resp
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());
    Ok(etag)
}

/// Delete an event resource. A missing resource (404/410) counts as success.
pub async fn delete_event(
    resource_url: &str,
    auth: &DavAuth,
    if_match: Option<&str>,
) -> Result<(), String> {
    let c = client()?;
    let mut rb = c.request(Method::DELETE, resource_url);
    if let Some(tag) = if_match {
        rb = rb.header("If-Match", tag);
    }
    let resp = apply_auth(rb, auth).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    if status.is_success() || status.as_u16() == 404 || status.as_u16() == 410 {
        return Ok(());
    }
    Err(format!("CalDAV DELETE {resource_url} -> {status}"))
}

/// Render a minimal VCALENDAR/VEVENT for a single event.
pub fn build_ics(ev: &EventInput) -> String {
    let mut out = String::new();
    out.push_str("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Mailquill//Calendar//EN\r\nBEGIN:VEVENT\r\n");
    out.push_str(&format!("UID:{}\r\n", ev.uid));
    out.push_str(&format!(
        "DTSTAMP:{}\r\n",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    ));
    if ev.all_day {
        out.push_str(&format!("DTSTART;VALUE=DATE:{}\r\n", ical_format(ev.starts_at, true)));
        out.push_str(&format!("DTEND;VALUE=DATE:{}\r\n", ical_format(ev.ends_at, true)));
    } else {
        out.push_str(&format!("DTSTART:{}\r\n", ical_format(ev.starts_at, false)));
        out.push_str(&format!("DTEND:{}\r\n", ical_format(ev.ends_at, false)));
    }
    out.push_str(&format!("SUMMARY:{}\r\n", escape_ical(ev.title)));
    if let Some(l) = ev.location.filter(|s| !s.is_empty()) {
        out.push_str(&format!("LOCATION:{}\r\n", escape_ical(l)));
    }
    if let Some(d) = ev.description.filter(|s| !s.is_empty()) {
        out.push_str(&format!("DESCRIPTION:{}\r\n", escape_ical(d)));
    }
    out.push_str("END:VEVENT\r\nEND:VCALENDAR\r\n");
    out
}

/// ISO-8601 -> compact iCal date/datetime (UTC). All-day yields `YYYYMMDD`.
fn ical_format(iso: &str, all_day: bool) -> String {
    let digits: String = iso.chars().filter(|c| c.is_ascii_digit()).collect();
    if all_day {
        return digits.chars().take(8).collect();
    }
    let mut d: String = digits.chars().take(14).collect();
    while d.len() < 14 {
        d.push('0');
    }
    format!("{}T{}Z", &d[0..8], &d[8..14])
}

fn escape_ical(v: &str) -> String {
    v.replace('\\', "\\\\").replace('\n', "\\n").replace(',', "\\,").replace(';', "\\;")
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
    let mut uid: Option<String> = None;
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
            uid = None;
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
                    uid: uid.take().unwrap_or_default(),
                    href: None,
                    etag: None,
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
            "UID" => uid = Some(value.to_owned()),
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

/// Parse a CalDAV multistatus into (href, etag, calendar-data) tuples, one per
/// `<response>` that carries a VEVENT.
fn parse_calendar_responses(xml: &str) -> Vec<(String, Option<String>, String)> {
    let mut reader = Reader::from_str(xml);
    let mut out = Vec::new();
    let mut in_response = false;
    let mut cur: Option<&'static str> = None;
    let mut href = String::new();
    let mut etag = String::new();
    let mut data = String::new();
    let mut buf = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match local_name(e.name().as_ref()) {
                b"response" => {
                    in_response = true;
                    href.clear();
                    etag.clear();
                    data.clear();
                }
                b"href" if in_response && href.is_empty() => {
                    cur = Some("href");
                    buf.clear();
                }
                b"getetag" if in_response => {
                    cur = Some("etag");
                    buf.clear();
                }
                b"calendar-data" if in_response => {
                    cur = Some("data");
                    buf.clear();
                }
                _ => {}
            },
            Ok(Event::Text(e)) if cur.is_some() => {
                if let Ok(t) = e.unescape() {
                    buf.push_str(&t);
                }
            }
            Ok(Event::CData(e)) if cur.is_some() => {
                buf.push_str(&String::from_utf8_lossy(&e.into_inner()));
            }
            Ok(Event::End(e)) => match local_name(e.name().as_ref()) {
                b"href" if cur == Some("href") => {
                    href = buf.trim().to_owned();
                    cur = None;
                }
                b"getetag" if cur == Some("etag") => {
                    etag = buf.trim().to_owned();
                    cur = None;
                }
                b"calendar-data" if cur == Some("data") => {
                    data = std::mem::take(&mut buf);
                    cur = None;
                }
                b"response" => {
                    in_response = false;
                    if data.contains("VEVENT") {
                        out.push((
                            href.clone(),
                            if etag.is_empty() { None } else { Some(etag.clone()) },
                            std::mem::take(&mut data),
                        ));
                    }
                }
                _ => {}
            },
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
