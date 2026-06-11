//! CardDAV read-sync: fetch vCards from a CardDAV collection and parse them
//! into a lightweight contact model. Discovery follows RFC 6352/6764
//! (current-user-principal → addressbook-home-set → addressbook collection).

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
pub struct ParsedContact {
    pub display_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub company: Option<String>,
    pub job_title: Option<String>,
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
        return Err(format!("CardDAV {method} {url} -> {status}"));
    }
    Ok(text)
}

/// Fetch and parse all contacts from a CardDAV account. `base_url` may be the
/// addressbook collection itself or any DAV URL discovery can start from.
pub async fn sync_carddav(base_url: &str, auth: &DavAuth) -> Result<Vec<ParsedContact>, String> {
    let c = client()?;
    let addressbook = discover_addressbook(&c, base_url, auth)
        .await
        .unwrap_or_else(|| base_url.to_owned());

    let body = r#"<?xml version="1.0" encoding="utf-8"?>
<C:addressbook-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:carddav">
  <D:prop><D:getetag/><C:address-data/></D:prop>
</C:addressbook-query>"#;

    let xml = dav(&c, "REPORT", &addressbook, auth, "1", body).await?;
    let cards = extract_texts(&xml, b"address-data");
    Ok(cards.iter().filter_map(|v| parse_vcard(v)).collect())
}

async fn discover_addressbook(c: &reqwest::Client, base: &str, auth: &DavAuth) -> Option<String> {
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
        r#"<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:carddav"><d:prop><c:addressbook-home-set/></d:prop></d:propfind>"#,
    )
    .await
    .ok()?;
    let home = resolve(base, &first_href_in_elem(&home_xml, b"addressbook-home-set")?)?;

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
    let href = first_href_with_resourcetype(&list_xml, b"addressbook")?;
    resolve(base, &href)
}

// ── vCard parsing ──────────────────────────────────────────────────────────

fn parse_vcard(raw: &str) -> Option<ParsedContact> {
    let unfolded = unfold(raw);
    let (mut name, mut email, mut phone, mut org, mut title) = (None, None, None, None, None);
    for line in unfolded.lines() {
        let Some((key, value)) = line.split_once(':') else { continue };
        let prop = key.split(';').next().unwrap_or("").to_ascii_uppercase();
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match prop.as_str() {
            "FN" => name = Some(value.to_owned()),
            "EMAIL" if email.is_none() => email = Some(value.to_owned()),
            "TEL" if phone.is_none() => phone = Some(value.to_owned()),
            "ORG" => org = Some(value.split(';').next().unwrap_or(value).trim().to_owned()),
            "TITLE" => title = Some(value.to_owned()),
            _ => {}
        }
    }
    let display_name = name.or_else(|| email.clone())?;
    Some(ParsedContact { display_name, email, phone, company: org, job_title: title })
}

/// Join RFC 5322 folded continuation lines (leading space/tab).
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

/// Collect the text content of every element whose local name == `target`.
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

/// First `<href>` nested inside the first element whose local name == `target`.
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

/// The `<href>` of the first `<response>` whose `<resourcetype>` contains an
/// element with local name == `rtype`.
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
