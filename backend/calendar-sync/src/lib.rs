//! CalDAV read-sync: fetch iCalendar VEVENTs from a CalDAV collection and parse
//! them into a lightweight event model. Discovery follows RFC 4791/6764
//! (current-user-principal → calendar-home-set → calendar collection).

use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::Method;
use serde_json::Value;
use std::error::Error as StdError;
use url::Url;

/// Stable marker used by API callers to distinguish certificate validation failures.
pub const TLS_CERTIFICATE_ERROR_PREFIX: &str = "tls certificate validation failed";

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
    pub rrule: Option<String>,
    pub rrule_uid: Option<String>,
    pub recurrence_id: Option<String>,
    pub status: Option<String>,
    pub organizer_email: Option<String>,
    pub organizer_name: Option<String>,
    pub attendees_json: Option<String>,
    pub ms_busystatus: Option<String>,
    pub ms_teams_url: Option<String>,
    pub raw_ical: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DiscoveredCalendar {
    pub name: String,
    pub url: String,
    pub color: Option<String>,
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
    pub rrule: Option<&'a str>,
    pub attendees_json: Option<&'a str>,
    pub organizer_email: Option<&'a str>,
    pub organizer_name: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct SyncResult {
    pub calendars: Vec<DiscoveredCalendar>,
    pub events: Vec<ParsedEvent>,
    pub sync_token: Option<String>,
}

fn client() -> Result<reqwest::Client, String> {
    client_with_trusted_cert(None)
}

fn client_with_trusted_cert(trusted_cert_der: Option<&[u8]>) -> Result<reqwest::Client, String> {
    client_with_tls_options(trusted_cert_der, false)
}

fn client_with_tls_options(
    trusted_cert_der: Option<&[u8]>,
    accept_invalid_tls: bool,
) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30));
    if accept_invalid_tls {
        builder = builder
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true);
    }
    if let Some(der) = trusted_cert_der {
        let cert = reqwest::Certificate::from_der(der)
            .map_err(|e| format!("trusted certificate invalid: {e}"))?;
        builder = builder
            .add_root_certificate(cert)
            .danger_accept_invalid_hostnames(true);
    }
    builder.build().map_err(|e| e.to_string())
}

fn apply_auth(rb: reqwest::RequestBuilder, auth: &DavAuth) -> reqwest::RequestBuilder {
    match auth {
        DavAuth::Basic { username, password } => rb.basic_auth(username, Some(password)),
        DavAuth::Bearer(token) => rb.bearer_auth(token),
    }
}

fn explain_transport_error(err: reqwest::Error) -> String {
    if error_chain_contains_certificate_failure(&err) {
        return format!("{TLS_CERTIFICATE_ERROR_PREFIX}: {err}");
    }
    if err.is_timeout() {
        return format!("request timed out: {err}");
    }
    if err.is_connect() {
        return format!("connection failed: {err}");
    }
    format!("request failed: {err}")
}

fn error_chain_contains_certificate_failure(mut err: &(dyn StdError + 'static)) -> bool {
    loop {
        let message = err.to_string().to_ascii_lowercase();
        let mentions_certificate = message.contains("certificate") || message.contains("cert ");
        let indicates_validation_failure = [
            "invalid",
            "not valid",
            "expired",
            "unknown issuer",
            "hostname",
            "name mismatch",
            "verify",
            "verification",
        ]
        .iter()
        .any(|needle| message.contains(needle));
        if mentions_certificate && indicates_validation_failure {
            return true;
        }
        let Some(source) = err.source() else {
            return false;
        };
        err = source;
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
    let resp = apply_auth(rb, auth)
        .send()
        .await
        .map_err(explain_transport_error)?;
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
    Ok(discover_calendars(&c, base_url, auth)
        .await
        .ok()
        .and_then(|calendars| calendars.into_iter().next().map(|calendar| calendar.url))
        .unwrap_or_else(|| base_url.to_owned()))
}

/// Discover every CalDAV calendar collection exposed by an account.
pub async fn discover_collections(
    base_url: &str,
    auth: &DavAuth,
) -> Result<Vec<DiscoveredCalendar>, String> {
    let c = client()?;
    discover_calendars(&c, base_url, auth).await
}

pub async fn discover_collections_with_trusted_cert(
    base_url: &str,
    auth: &DavAuth,
    trusted_cert_der: Option<&[u8]>,
) -> Result<Vec<DiscoveredCalendar>, String> {
    discover_collections_with_tls_options(base_url, auth, trusted_cert_der, false).await
}

pub async fn discover_collections_with_tls_options(
    base_url: &str,
    auth: &DavAuth,
    trusted_cert_der: Option<&[u8]>,
    accept_invalid_tls: bool,
) -> Result<Vec<DiscoveredCalendar>, String> {
    let c = client_with_tls_options(trusted_cert_der, accept_invalid_tls)?;
    discover_calendars(&c, base_url, auth).await
}

/// Fetch all events from a CalDAV collection. Each event carries its server
/// `href` and `etag` so the caller can match, update, and delete resources.
pub async fn pull(collection_url: &str, auth: &DavAuth) -> Result<Vec<ParsedEvent>, String> {
    pull_range(collection_url, auth, None, None).await
}

/// Fetch events from a CalDAV collection within an optional UTC range.
pub async fn pull_range(
    collection_url: &str,
    auth: &DavAuth,
    start_utc: Option<chrono::DateTime<chrono::Utc>>,
    end_utc: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Vec<ParsedEvent>, String> {
    pull_range_with_trusted_cert(collection_url, auth, start_utc, end_utc, None).await
}

pub async fn pull_range_with_trusted_cert(
    collection_url: &str,
    auth: &DavAuth,
    start_utc: Option<chrono::DateTime<chrono::Utc>>,
    end_utc: Option<chrono::DateTime<chrono::Utc>>,
    trusted_cert_der: Option<&[u8]>,
) -> Result<Vec<ParsedEvent>, String> {
    pull_range_with_tls_options(
        collection_url,
        auth,
        start_utc,
        end_utc,
        trusted_cert_der,
        false,
    )
    .await
}

pub async fn pull_range_with_tls_options(
    collection_url: &str,
    auth: &DavAuth,
    start_utc: Option<chrono::DateTime<chrono::Utc>>,
    end_utc: Option<chrono::DateTime<chrono::Utc>>,
    trusted_cert_der: Option<&[u8]>,
    accept_invalid_tls: bool,
) -> Result<Vec<ParsedEvent>, String> {
    let c = client_with_tls_options(trusted_cert_der, accept_invalid_tls)?;
    let time_range = match (start_utc, end_utc) {
        (Some(start), Some(end)) => format!(
            r#"<C:time-range start="{}" end="{}"/>"#,
            start.format("%Y%m%dT%H%M%SZ"),
            end.format("%Y%m%dT%H%M%SZ")
        ),
        _ => String::new(),
    };
    let body = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop><D:getetag/><C:calendar-data/></D:prop>
  <C:filter><C:comp-filter name="VCALENDAR"><C:comp-filter name="VEVENT">{time_range}</C:comp-filter></C:comp-filter></C:filter>
</C:calendar-query>"#
    );

    let xml = dav(&c, "REPORT", collection_url, auth, "1", &body).await?;
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

/// Read the collection change tag used for cheap "anything changed?" checks.
pub async fn collection_ctag(
    collection_url: &str,
    auth: &DavAuth,
) -> Result<Option<String>, String> {
    collection_ctag_with_trusted_cert(collection_url, auth, None).await
}

pub async fn collection_ctag_with_trusted_cert(
    collection_url: &str,
    auth: &DavAuth,
    trusted_cert_der: Option<&[u8]>,
) -> Result<Option<String>, String> {
    collection_ctag_with_tls_options(collection_url, auth, trusted_cert_der, false).await
}

pub async fn collection_ctag_with_tls_options(
    collection_url: &str,
    auth: &DavAuth,
    trusted_cert_der: Option<&[u8]>,
    accept_invalid_tls: bool,
) -> Result<Option<String>, String> {
    let c = client_with_tls_options(trusted_cert_der, accept_invalid_tls)?;
    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:CS="http://calendarserver.org/ns/">
  <D:prop><CS:getctag/></D:prop>
</D:propfind>"#;
    let xml = dav(&c, "PROPFIND", collection_url, auth, "0", body).await?;
    Ok(first_text_in_elem(&xml, b"getctag"))
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
    put_event_with_client(&c, resource_url, auth, ics, if_match).await
}

pub async fn put_event_with_tls_options(
    resource_url: &str,
    auth: &DavAuth,
    ics: &str,
    if_match: Option<&str>,
    trusted_cert_der: Option<&[u8]>,
    accept_invalid_tls: bool,
) -> Result<Option<String>, String> {
    let c = client_with_tls_options(trusted_cert_der, accept_invalid_tls)?;
    put_event_with_client(&c, resource_url, auth, ics, if_match).await
}

async fn put_event_with_client(
    c: &reqwest::Client,
    resource_url: &str,
    auth: &DavAuth,
    ics: &str,
    if_match: Option<&str>,
) -> Result<Option<String>, String> {
    let mut rb = c
        .request(Method::PUT, resource_url)
        .header("Content-Type", "text/calendar; charset=utf-8")
        .body(ics.to_owned());
    if let Some(tag) = if_match {
        rb = rb.header("If-Match", tag);
    }
    let resp = apply_auth(rb, auth)
        .send()
        .await
        .map_err(explain_transport_error)?;
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
    delete_event_with_client(&c, resource_url, auth, if_match).await
}

pub async fn delete_event_with_tls_options(
    resource_url: &str,
    auth: &DavAuth,
    if_match: Option<&str>,
    trusted_cert_der: Option<&[u8]>,
    accept_invalid_tls: bool,
) -> Result<(), String> {
    let c = client_with_tls_options(trusted_cert_der, accept_invalid_tls)?;
    delete_event_with_client(&c, resource_url, auth, if_match).await
}

async fn delete_event_with_client(
    c: &reqwest::Client,
    resource_url: &str,
    auth: &DavAuth,
    if_match: Option<&str>,
) -> Result<(), String> {
    let mut rb = c.request(Method::DELETE, resource_url);
    if let Some(tag) = if_match {
        rb = rb.header("If-Match", tag);
    }
    let resp = apply_auth(rb, auth)
        .send()
        .await
        .map_err(explain_transport_error)?;
    let status = resp.status();
    if status.is_success() || status.as_u16() == 404 || status.as_u16() == 410 {
        return Ok(());
    }
    Err(format!("CalDAV DELETE {resource_url} -> {status}"))
}

pub async fn graph_sync(
    access_token: &str,
    start: chrono::DateTime<chrono::Utc>,
    end: chrono::DateTime<chrono::Utc>,
    delta_link: Option<&str>,
) -> Result<SyncResult, String> {
    let c = client()?;
    let calendars_json = bearer_json(
        &c,
        "https://graph.microsoft.com/v1.0/me/calendars",
        access_token,
    )
    .await?;
    let calendars = calendars_json["value"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| DiscoveredCalendar {
            name: item["name"].as_str().unwrap_or("Calendar").to_owned(),
            url: item["id"].as_str().unwrap_or_default().to_owned(),
            color: item["hexColor"].as_str().map(str::to_owned),
        })
        .collect::<Vec<_>>();
    let url = delta_link.map(str::to_owned).unwrap_or_else(|| {
        format!(
            "https://graph.microsoft.com/v1.0/me/calendarView/delta?startDateTime={}&endDateTime={}",
            start.to_rfc3339(),
            end.to_rfc3339()
        )
    });
    let events_json = bearer_json(&c, &url, access_token).await?;
    let events = events_json["value"]
        .as_array()
        .into_iter()
        .flatten()
        .map(graph_event)
        .collect();
    let sync_token = events_json["@odata.deltaLink"].as_str().map(str::to_owned);
    Ok(SyncResult {
        calendars,
        events,
        sync_token,
    })
}

pub async fn graph_write(
    access_token: &str,
    remote_id: Option<&str>,
    ev: &EventInput<'_>,
    delete: bool,
) -> Result<Option<String>, String> {
    let c = client()?;
    let url = remote_id
        .map(|id| format!("https://graph.microsoft.com/v1.0/me/events/{id}"))
        .unwrap_or_else(|| "https://graph.microsoft.com/v1.0/me/events".to_owned());
    let req = if delete {
        c.delete(&url)
    } else if remote_id.is_some() {
        c.patch(&url).json(&graph_event_body(ev))
    } else {
        c.post(&url).json(&graph_event_body(ev))
    };
    let resp = req
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Graph write -> {}", resp.status()));
    }
    if delete {
        return Ok(None);
    }
    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(json["id"].as_str().map(str::to_owned))
}

pub async fn google_sync(
    access_token: &str,
    start: chrono::DateTime<chrono::Utc>,
    end: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<SyncResult>, String> {
    let c = client()?;
    let calendars_json = bearer_json(
        &c,
        "https://www.googleapis.com/calendar/v3/users/me/calendarList?maxResults=250",
        access_token,
    )
    .await?;
    let calendars = calendars_json["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["accessRole"].as_str() != Some("freeBusyReader"))
        .map(|item| DiscoveredCalendar {
            name: item["summary"].as_str().unwrap_or("Calendar").to_owned(),
            url: item["id"].as_str().unwrap_or("primary").to_owned(),
            color: item["backgroundColor"].as_str().map(str::to_owned),
        })
        .collect::<Vec<_>>();

    let mut results = Vec::with_capacity(calendars.len());
    for calendar in calendars {
        let url = format!(
            "https://www.googleapis.com/calendar/v3/calendars/{}/events?singleEvents=false&maxResults=2500&timeMin={}&timeMax={}",
            urlencoding::encode(&calendar.url),
            urlencoding::encode(start.to_rfc3339().as_str()),
            urlencoding::encode(end.to_rfc3339().as_str())
        );
        let events_json = bearer_json(&c, &url, access_token).await?;
        let events = events_json["items"]
            .as_array()
            .into_iter()
            .flatten()
            .map(google_event)
            .collect();
        let sync_token = events_json["nextSyncToken"].as_str().map(str::to_owned);
        results.push(SyncResult {
            calendars: vec![calendar],
            events,
            sync_token,
        });
    }
    Ok(results)
}

pub async fn google_write(
    access_token: &str,
    calendar_id: &str,
    remote_id: Option<&str>,
    ev: &EventInput<'_>,
    delete: bool,
) -> Result<Option<String>, String> {
    let c = client()?;
    let base = format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events",
        urlencoding::encode(calendar_id)
    );
    let url = remote_id
        .map(|id| format!("{base}/{}", urlencoding::encode(id)))
        .unwrap_or(base);
    let req = if delete {
        c.delete(&url)
    } else if remote_id.is_some() {
        c.put(&url).json(&google_event_body(ev))
    } else {
        c.post(&url).json(&google_event_body(ev))
    };
    let resp = req
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Google write -> {}", resp.status()));
    }
    if delete {
        return Ok(None);
    }
    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(json["id"].as_str().map(str::to_owned))
}

pub async fn ox_sync(base_url: &str, auth: &DavAuth) -> Result<SyncResult, String> {
    let c = client()?;
    let url = format!("{}/api/chronos?action=all", base_url.trim_end_matches('/'));
    let resp = apply_auth(c.get(&url), auth)
        .send()
        .await
        .map_err(explain_transport_error)?;
    if !resp.status().is_success() {
        return Err(format!("OX chronos -> {}", resp.status()));
    }
    let json: Value = resp.json().await.map_err(|e| e.to_string())?;
    let data = json["data"].as_array().cloned().unwrap_or_default();
    let events = data.iter().map(ox_event).collect();
    Ok(SyncResult {
        calendars: vec![DiscoveredCalendar {
            name: "Open-Xchange".into(),
            url: base_url.to_owned(),
            color: None,
        }],
        events,
        sync_token: None,
    })
}

pub fn expand_rrule(master: &ParsedEvent, window_days: i64) -> Vec<ParsedEvent> {
    let Some(rrule) = &master.rrule else {
        return vec![master.clone()];
    };
    let freq = rrule_param(rrule, "FREQ").unwrap_or_else(|| "DAILY".to_owned());
    let count = rrule_param(rrule, "COUNT")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(256);
    let Ok(start) = chrono::DateTime::parse_from_rfc3339(&ensure_tz(&master.starts_at)) else {
        return vec![master.clone()];
    };
    let Ok(end) = chrono::DateTime::parse_from_rfc3339(&ensure_tz(&master.ends_at)) else {
        return vec![master.clone()];
    };
    let duration = end - start;
    let until = chrono::Utc::now() + chrono::Duration::days(window_days);
    let mut out = Vec::new();
    for i in 0..count {
        let next = match freq.as_str() {
            "WEEKLY" => start + chrono::Duration::weeks(i as i64),
            "MONTHLY" => start + chrono::Duration::days(30 * i as i64),
            "YEARLY" => start + chrono::Duration::days(365 * i as i64),
            _ => start + chrono::Duration::days(i as i64),
        };
        if next.with_timezone(&chrono::Utc) > until {
            break;
        }
        let mut ev = master.clone();
        ev.rrule_uid = None;
        ev.starts_at = next.to_rfc3339();
        ev.ends_at = (next + duration).to_rfc3339();
        ev.recurrence_id = Some(ev.starts_at.clone());
        out.push(ev);
    }
    out
}

async fn bearer_json(c: &reqwest::Client, url: &str, token: &str) -> Result<Value, String> {
    let resp = c
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(explain_transport_error)?;
    if !resp.status().is_success() {
        return Err(format!("{url} -> {}", resp.status()));
    }
    resp.json().await.map_err(|e| e.to_string())
}

fn graph_event(item: &Value) -> ParsedEvent {
    let start = item["start"]["dateTime"].as_str().unwrap_or_default();
    let end = item["end"]["dateTime"].as_str().unwrap_or(start);
    let attendees = item["attendees"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "email": a["emailAddress"]["address"].as_str().unwrap_or_default(),
                "name": a["emailAddress"]["name"].as_str().unwrap_or_default(),
                "partstat": a["status"]["response"].as_str().unwrap_or("needsAction"),
            })
        })
        .collect::<Vec<_>>();
    ParsedEvent {
        uid: item["iCalUId"]
            .as_str()
            .or_else(|| item["id"].as_str())
            .unwrap_or_default()
            .to_owned(),
        href: item["id"].as_str().map(str::to_owned),
        etag: item["@odata.etag"].as_str().map(str::to_owned),
        title: item["subject"].as_str().unwrap_or("(No title)").to_owned(),
        starts_at: ensure_tz(start),
        ends_at: ensure_tz(end),
        all_day: item["isAllDay"].as_bool().unwrap_or(false),
        location: item["location"]["displayName"].as_str().map(str::to_owned),
        description: item["bodyPreview"].as_str().map(str::to_owned),
        rrule: None,
        rrule_uid: None,
        recurrence_id: None,
        status: item["showAs"].as_str().map(str::to_owned),
        organizer_email: item["organizer"]["emailAddress"]["address"]
            .as_str()
            .map(str::to_owned),
        organizer_name: item["organizer"]["emailAddress"]["name"]
            .as_str()
            .map(str::to_owned),
        attendees_json: Some(Value::Array(attendees).to_string()),
        ms_busystatus: item["showAs"].as_str().map(str::to_owned),
        ms_teams_url: item["onlineMeeting"]["joinUrl"].as_str().map(str::to_owned),
        raw_ical: None,
    }
}

fn google_event(item: &Value) -> ParsedEvent {
    let start = item["start"]["dateTime"]
        .as_str()
        .or_else(|| item["start"]["date"].as_str())
        .unwrap_or_default();
    let end = item["end"]["dateTime"]
        .as_str()
        .or_else(|| item["end"]["date"].as_str())
        .unwrap_or(start);
    let attendees = item["attendees"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|a| {
            serde_json::json!({
                "email": a["email"].as_str().unwrap_or_default(),
                "name": a["displayName"].as_str().unwrap_or_default(),
                "partstat": a["responseStatus"].as_str().unwrap_or("needsAction"),
            })
        })
        .collect::<Vec<_>>();
    ParsedEvent {
        uid: item["iCalUID"]
            .as_str()
            .or_else(|| item["id"].as_str())
            .unwrap_or_default()
            .to_owned(),
        href: item["id"].as_str().map(str::to_owned),
        etag: item["etag"].as_str().map(str::to_owned),
        title: item["summary"].as_str().unwrap_or("(No title)").to_owned(),
        starts_at: ensure_tz(start),
        ends_at: ensure_tz(end),
        all_day: item["start"]["date"].is_string(),
        location: item["location"].as_str().map(str::to_owned),
        description: item["description"].as_str().map(str::to_owned),
        rrule: item["recurrence"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .map(|s| s.trim_start_matches("RRULE:").to_owned()),
        rrule_uid: None,
        recurrence_id: item["recurringEventId"].as_str().map(str::to_owned),
        status: item["status"].as_str().map(str::to_owned),
        organizer_email: item["organizer"]["email"].as_str().map(str::to_owned),
        organizer_name: item["organizer"]["displayName"].as_str().map(str::to_owned),
        attendees_json: Some(Value::Array(attendees).to_string()),
        ms_busystatus: None,
        ms_teams_url: None,
        raw_ical: None,
    }
}

fn ox_event(item: &Value) -> ParsedEvent {
    ParsedEvent {
        uid: item["uid"]
            .as_str()
            .or_else(|| item["id"].as_str())
            .unwrap_or_default()
            .to_owned(),
        href: item["id"].as_str().map(str::to_owned),
        etag: item["timestamp"].as_i64().map(|v| v.to_string()),
        title: item["summary"]
            .as_str()
            .or_else(|| item["title"].as_str())
            .unwrap_or("(No title)")
            .to_owned(),
        starts_at: ensure_tz(
            item["startDate"]["value"]
                .as_str()
                .or_else(|| item["start"].as_str())
                .unwrap_or_default(),
        ),
        ends_at: ensure_tz(
            item["endDate"]["value"]
                .as_str()
                .or_else(|| item["end"].as_str())
                .unwrap_or_default(),
        ),
        all_day: item["startDate"]["tzid"].as_str() == Some("UTC")
            && item["startDate"]["value"]
                .as_str()
                .is_some_and(|v| v.len() <= 10),
        location: item["location"].as_str().map(str::to_owned),
        description: item["description"].as_str().map(str::to_owned),
        rrule: item["rrule"].as_str().map(str::to_owned),
        rrule_uid: None,
        recurrence_id: item["recurrenceId"].as_str().map(str::to_owned),
        status: item["status"].as_str().map(str::to_owned),
        organizer_email: item["organizer"].as_str().map(str::to_owned),
        organizer_name: None,
        attendees_json: Some(
            Value::Array(item["attendees"].as_array().cloned().unwrap_or_default()).to_string(),
        ),
        ms_busystatus: None,
        ms_teams_url: None,
        raw_ical: None,
    }
}

fn graph_event_body(ev: &EventInput<'_>) -> Value {
    serde_json::json!({
        "subject": ev.title,
        "body": { "contentType": "text", "content": ev.description.unwrap_or("") },
        "start": { "dateTime": ev.starts_at, "timeZone": "UTC" },
        "end": { "dateTime": ev.ends_at, "timeZone": "UTC" },
        "location": { "displayName": ev.location.unwrap_or("") },
        "isAllDay": ev.all_day,
    })
}

fn google_event_body(ev: &EventInput<'_>) -> Value {
    serde_json::json!({
        "summary": ev.title,
        "description": ev.description.unwrap_or(""),
        "location": ev.location.unwrap_or(""),
        "start": if ev.all_day { serde_json::json!({ "date": ev.starts_at.get(0..10).unwrap_or(ev.starts_at) }) } else { serde_json::json!({ "dateTime": ev.starts_at }) },
        "end": if ev.all_day { serde_json::json!({ "date": ev.ends_at.get(0..10).unwrap_or(ev.ends_at) }) } else { serde_json::json!({ "dateTime": ev.ends_at }) },
        "recurrence": ev.rrule.map(|r| vec![format!("RRULE:{}", r.trim_start_matches("RRULE:"))]),
    })
}

fn ensure_tz(value: &str) -> String {
    if value.is_empty() {
        return chrono::Utc::now().to_rfc3339();
    }
    if value.ends_with('Z')
        || value.contains('+')
        || value
            .rsplit_once('-')
            .is_some_and(|(_, tail)| tail.contains(':'))
    {
        value.to_owned()
    } else if value.len() == 10 {
        format!("{value}T00:00:00Z")
    } else {
        format!("{value}Z")
    }
}

fn rrule_param(rrule: &str, name: &str) -> Option<String> {
    rrule.split(';').find_map(|part| {
        let (k, v) = part.split_once('=')?;
        k.eq_ignore_ascii_case(name).then(|| v.to_ascii_uppercase())
    })
}

/// Render a minimal VCALENDAR/VEVENT for a single event.
pub fn build_ics(ev: &EventInput) -> String {
    let mut out = String::new();
    out.push_str(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Mailquill//Calendar//EN\r\nBEGIN:VEVENT\r\n",
    );
    out.push_str(&format!("UID:{}\r\n", ev.uid));
    out.push_str(&format!(
        "DTSTAMP:{}\r\n",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    ));
    if ev.all_day {
        out.push_str(&format!(
            "DTSTART;VALUE=DATE:{}\r\n",
            ical_format(ev.starts_at, true)
        ));
        out.push_str(&format!(
            "DTEND;VALUE=DATE:{}\r\n",
            ical_format(ev.ends_at, true)
        ));
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
    if let Some(rrule) = ev.rrule.filter(|s| !s.is_empty()) {
        out.push_str(&format!("RRULE:{}\r\n", rrule.trim_start_matches("RRULE:")));
    }
    if let Some(org) = ev.organizer_email.filter(|s| !s.is_empty()) {
        let name = ev.organizer_name.unwrap_or(org);
        out.push_str(&format!(
            "ORGANIZER;CN={}:mailto:{}\r\n",
            escape_ical(name),
            org
        ));
    }
    if let Some(attendees) = ev.attendees_json {
        if let Ok(values) = serde_json::from_str::<Vec<Value>>(attendees) {
            for attendee in values {
                let email = attendee["email"].as_str().unwrap_or_default();
                if email.is_empty() {
                    continue;
                }
                let name = attendee["name"].as_str().unwrap_or(email);
                let partstat = attendee["partstat"].as_str().unwrap_or("NEEDS-ACTION");
                out.push_str(&format!(
                    "ATTENDEE;CN={};PARTSTAT={}:mailto:{}\r\n",
                    escape_ical(name),
                    partstat.to_ascii_uppercase(),
                    email
                ));
            }
        }
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
    v.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace(',', "\\,")
        .replace(';', "\\;")
}

async fn discover_calendars(
    c: &reqwest::Client,
    base: &str,
    auth: &DavAuth,
) -> Result<Vec<DiscoveredCalendar>, String> {
    let principal_xml = match dav(
        c,
        "PROPFIND",
        base,
        auth,
        "0",
        r#"<d:propfind xmlns:d="DAV:"><d:prop><d:current-user-principal/></d:prop></d:propfind>"#,
    )
    .await
    {
        Ok(xml) => xml,
        Err(err) => {
            return discover_calendar_collections_at(c, base, auth)
                .await
                .map_err(|fallback| {
                    format!("principal discovery failed: {err}; direct scan failed: {fallback}")
                });
        }
    };
    let Some(principal_href) = first_href_in_elem(&principal_xml, b"current-user-principal") else {
        return discover_calendar_collections_at(c, base, auth)
            .await
            .map_err(|fallback| {
                format!("current-user-principal not found; direct scan failed: {fallback}")
            });
    };
    let principal = resolve(base, &principal_href)
        .ok_or_else(|| "current-user-principal URL is invalid".to_owned())?;

    let home_xml = match dav(
        c,
        "PROPFIND",
        &principal,
        auth,
        "0",
        r#"<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop><c:calendar-home-set/></d:prop></d:propfind>"#,
    )
    .await
    {
        Ok(xml) => xml,
        Err(err) => {
            return discover_calendar_collections_at(c, base, auth)
                .await
                .map_err(|fallback| {
                    format!("calendar-home-set discovery failed: {err}; direct scan failed: {fallback}")
                });
        }
    };
    let Some(home_href) = first_href_in_elem(&home_xml, b"calendar-home-set") else {
        return discover_calendar_collections_at(c, base, auth)
            .await
            .map_err(|fallback| {
                format!("calendar-home-set not found; direct scan failed: {fallback}")
            });
    };
    let home =
        resolve(base, &home_href).ok_or_else(|| "calendar-home-set URL is invalid".to_owned())?;

    discover_calendar_collections_at(c, &home, auth).await
}

async fn discover_calendar_collections_at(
    c: &reqwest::Client,
    base: &str,
    auth: &DavAuth,
) -> Result<Vec<DiscoveredCalendar>, String> {
    let body = r#"<d:propfind xmlns:d="DAV:" xmlns:a="http://apple.com/ns/ical/"><d:prop><d:resourcetype/><d:displayname/><a:calendar-color/></d:prop></d:propfind>"#;
    let list_xml = match dav(c, "PROPFIND", base, auth, "1", body).await {
        Ok(xml) => xml,
        Err(_) => dav(c, "PROPFIND", base, auth, "0", body)
            .await
            .map_err(|e| format!("calendar collection discovery failed: {e}"))?,
    };
    let calendars: Vec<DiscoveredCalendar> = calendars_with_resourcetype(&list_xml, b"calendar")
        .into_iter()
        .filter_map(|calendar| {
            resolve(base, &calendar.url).map(|url| DiscoveredCalendar { url, ..calendar })
        })
        .collect();
    if calendars.is_empty() {
        Err("no calendar collections found".to_owned())
    } else {
        Ok(calendars)
    }
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
    let mut rrule: Option<String> = None;
    let mut recurrence_id: Option<String> = None;
    let mut status: Option<String> = None;
    let mut organizer_email: Option<String> = None;
    let mut organizer_name: Option<String> = None;
    let mut attendees: Vec<Value> = Vec::new();
    let mut ms_busystatus: Option<String> = None;
    let mut ms_teams_url: Option<String> = None;
    let mut event_lines: Vec<String> = Vec::new();

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
            rrule = None;
            recurrence_id = None;
            status = None;
            organizer_email = None;
            organizer_name = None;
            attendees = Vec::new();
            ms_busystatus = None;
            ms_teams_url = None;
            event_lines = vec!["BEGIN:VEVENT".to_owned()];
            continue;
        }
        if upper.starts_with("END:VEVENT") {
            event_lines.push("END:VEVENT".to_owned());
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
                    rrule: rrule.take(),
                    rrule_uid: uid.clone(),
                    recurrence_id: recurrence_id.take(),
                    status: status.take(),
                    organizer_email: organizer_email.take(),
                    organizer_name: organizer_name.take(),
                    attendees_json: Some(Value::Array(std::mem::take(&mut attendees)).to_string()),
                    ms_busystatus: ms_busystatus.take(),
                    ms_teams_url: ms_teams_url.take(),
                    raw_ical: Some(format!(
                        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{}\r\nEND:VCALENDAR\r\n",
                        event_lines.join("\r\n")
                    )),
                });
            }
            in_event = false;
            continue;
        }
        if !in_event {
            continue;
        }
        event_lines.push(line.to_owned());
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
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
            "RRULE" => rrule = Some(value.to_owned()),
            "RECURRENCE-ID" => recurrence_id = Some(ical_to_iso(value).0),
            "STATUS" => status = Some(value.to_ascii_lowercase()),
            "ORGANIZER" => {
                organizer_email = Some(strip_mailto(value).to_owned());
                organizer_name = param_value(key, "CN").map(|value| unescape_ical(&value));
            }
            "ATTENDEE" => {
                attendees.push(serde_json::json!({
                    "email": strip_mailto(value),
                    "name": param_value(key, "CN").unwrap_or_else(|| strip_mailto(value).to_owned()),
                    "partstat": param_value(key, "PARTSTAT").unwrap_or_else(|| "NEEDS-ACTION".to_owned()),
                    "cutype": param_value(key, "CUTYPE").unwrap_or_else(|| "INDIVIDUAL".to_owned()),
                }));
            }
            "X-MICROSOFT-CDO-BUSYSTATUS" => ms_busystatus = Some(value.to_ascii_lowercase()),
            "X-MICROSOFT-SKYPETEAMSMEETINGURL" => ms_teams_url = Some(value.to_owned()),
            _ => {}
        }
    }
    events
}

pub fn parse_icalendar_events(ics: &str) -> Vec<ParsedEvent> {
    parse_vevents(ics)
}

pub fn parse_method(ics: &str) -> Option<String> {
    unfold(ics).lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.eq_ignore_ascii_case("METHOD")).then(|| value.trim().to_ascii_uppercase())
    })
}

fn strip_mailto(value: &str) -> &str {
    value
        .strip_prefix("mailto:")
        .or_else(|| value.strip_prefix("MAILTO:"))
        .unwrap_or(value)
}

fn param_value(key: &str, param: &str) -> Option<String> {
    key.split(';').skip(1).find_map(|part| {
        let (name, value) = part.split_once('=')?;
        name.eq_ignore_ascii_case(param)
            .then(|| value.trim_matches('"').to_owned())
    })
}

/// Convert a compact iCal date/datetime to an ISO-8601-ish string. Returns
/// (value, is_date_only).
fn ical_to_iso(v: &str) -> (String, bool) {
    let s = v.trim();
    let digit_count = s.chars().take_while(|c| c.is_ascii_digit()).count();
    if s.len() == 8 && digit_count == 8 {
        return (
            format!("{}-{}-{}T00:00:00", &s[0..4], &s[4..6], &s[6..8]),
            true,
        );
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
    v.replace("\\n", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
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
                if let Ok(t) = e.xml_content(quick_xml::XmlVersion::Implicit1_0) {
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
                            if etag.is_empty() {
                                None
                            } else {
                                Some(etag.clone())
                            },
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
                if let Ok(t) = e.xml_content(quick_xml::XmlVersion::Implicit1_0) {
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

fn first_text_in_elem(xml: &str, target: &[u8]) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut in_target = false;
    let mut buf = String::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if local_name(e.name().as_ref()) == target => {
                in_target = true;
                buf.clear();
            }
            Ok(Event::Text(e)) if in_target => {
                if let Ok(t) = e.xml_content(quick_xml::XmlVersion::Implicit1_0) {
                    buf.push_str(&t);
                }
            }
            Ok(Event::CData(e)) if in_target => {
                buf.push_str(&String::from_utf8_lossy(&e.into_inner()));
            }
            Ok(Event::End(e)) if local_name(e.name().as_ref()) == target => {
                let value = buf.trim();
                return (!value.is_empty()).then(|| value.to_owned());
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    None
}

fn calendars_with_resourcetype(xml: &str, rtype: &[u8]) -> Vec<DiscoveredCalendar> {
    let mut reader = Reader::from_str(xml);
    let (mut in_response, mut in_href, mut in_rtype) = (false, false, 0i32);
    let mut text_prop: Option<&'static [u8]> = None;
    let mut matched = false;
    let mut href: Option<String> = None;
    let mut display_name: Option<String> = None;
    let mut color: Option<String> = None;
    let mut buf = String::new();
    let mut out = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let ln = local_name(name.as_ref());
                if ln == b"response" {
                    in_response = true;
                    matched = false;
                    href = None;
                    display_name = None;
                    color = None;
                } else if in_response && ln == b"href" && href.is_none() {
                    in_href = true;
                    buf.clear();
                } else if ln == b"resourcetype" {
                    in_rtype += 1;
                } else if in_rtype > 0 && ln == rtype {
                    matched = true;
                } else if in_response && (ln == b"displayname" || ln == b"calendar-color") {
                    text_prop = Some(if ln == b"displayname" {
                        b"displayname"
                    } else {
                        b"calendar-color"
                    });
                    buf.clear();
                }
            }
            Ok(Event::Empty(e)) if in_rtype > 0 && local_name(e.name().as_ref()) == rtype => {
                matched = true;
            }
            Ok(Event::Text(e)) if in_href || text_prop.is_some() => {
                if let Ok(t) = e.xml_content(quick_xml::XmlVersion::Implicit1_0) {
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
                } else if text_prop == Some(ln) {
                    let value = buf.trim();
                    if !value.is_empty() {
                        if ln == b"displayname" {
                            display_name = Some(value.to_owned());
                        } else {
                            color = Some(value.to_owned());
                        }
                    }
                    text_prop = None;
                } else if ln == b"response" {
                    in_response = false;
                    if matched {
                        if let Some(url) = href.take() {
                            let name = display_name.take().unwrap_or_else(|| calendar_name(&url));
                            out.push(DiscoveredCalendar {
                                name,
                                url,
                                color: color.take(),
                            });
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    out
}

fn calendar_name(href: &str) -> String {
    href.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or("Calendar")
        .to_owned()
}

fn resolve(base: &str, href: &str) -> Option<String> {
    Url::parse(base)
        .ok()?
        .join(href)
        .ok()
        .map(|u| u.to_string())
}

#[cfg(test)]
mod tests {
    use super::error_chain_contains_certificate_failure;

    #[test]
    fn identifies_certificate_validation_failures() {
        let error = std::io::Error::other(
            "invalid peer certificate: certificate not valid for name mail.example.test",
        );

        assert!(error_chain_contains_certificate_failure(&error));
    }

    #[test]
    fn leaves_other_connection_failures_unclassified() {
        let error = std::io::Error::other("connection refused");

        assert!(!error_chain_contains_certificate_failure(&error));
    }
}
