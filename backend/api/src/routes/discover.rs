//! Mail server autodiscovery for the account wizard.
//!
//! Resolution order, per endpoint (IMAP / SMTP submission):
//! 1. RFC 6186 SRV records (`_imaps._tcp`, `_imap._tcp`, `_submissions._tcp`,
//!    `_submission._tcp`) — published by the domain owner.
//! 2. Thunderbird ISPDB (`autoconfig.thunderbird.net`), the community-curated
//!    database of provider settings — keyed by the mail domain, and as a
//!    second chance by the parent domain of the MX target (catches custom
//!    domains hosted at a known provider).
//! 3. TCP reachability probe over candidate hosts: the conventional
//!    subdomains (`imap.`/`smtp.`, `mail.`) plus the domain's MX targets.
//!    Probing actual ports avoids false positives from wildcard DNS, where
//!    `imap.<domain>` resolves but nothing listens there.
//!
//! Hostnames coming back from DNS or the ISPDB are external input and are
//! re-validated before they are returned to the client.

use std::collections::HashSet;
use std::time::Duration;

use axum::{extract::Query, response::IntoResponse, Json};
use hickory_resolver::TokioResolver;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::{error::AppError, validate};

// Covers TCP connect plus the (STARTTLS-)handshake of a single probe.
const PROBE_TIMEOUT: Duration = Duration::from_secs(6);
const ISPDB_BASE: &str = "https://autoconfig.thunderbird.net/v1.1";
const ISPDB_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Deserialize)]
pub struct DiscoverQuery {
    email: String,
}

#[derive(Serialize, Clone, PartialEq, Eq, Hash)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    /// `ssl` (implicit TLS) or `starttls`, matching the values the account
    /// form already uses.
    pub security: &'static str,
    /// `srv`, `ispdb`, `mx`, or `probe`, for UI display and debugging.
    pub source: &'static str,
}

#[derive(Default)]
pub struct IspdbConfig {
    pub provider: Option<String>,
    pub imap: Option<Endpoint>,
    pub smtp: Option<Endpoint>,
}

fn endpoint(host: &str, port: u16, security: &'static str, source: &'static str) -> Endpoint {
    Endpoint {
        host: host.to_owned(),
        port,
        security,
        source,
    }
}

fn hosted_provider_from_mx(mx: &[String]) -> Option<IspdbConfig> {
    let has = |needle: &str| mx.iter().any(|host| host.ends_with(needle));
    if has(".google.com") || has(".googlemail.com") {
        return Some(IspdbConfig {
            provider: Some("Google Workspace".to_owned()),
            imap: Some(endpoint("imap.gmail.com", 993, "ssl", "mx")),
            smtp: Some(endpoint("smtp.gmail.com", 465, "ssl", "mx")),
        });
    }
    if has(".protection.outlook.com") {
        return Some(IspdbConfig {
            provider: Some("Microsoft 365".to_owned()),
            imap: Some(endpoint("outlook.office365.com", 993, "ssl", "mx")),
            smtp: Some(endpoint("smtp.office365.com", 587, "starttls", "mx")),
        });
    }
    None
}

fn security_from_socket_type(socket_type: &str) -> Option<&'static str> {
    match socket_type {
        "SSL" => Some("ssl"),
        "STARTTLS" => Some("starttls"),
        "plain" => Some("none"),
        _ => None,
    }
}

/// Parse a Thunderbird ISPDB / autoconfig `config-v1.1.xml` document into the
/// first usable IMAP and SMTP endpoint. Hostnames and ports are validated;
/// servers with unusable values are skipped.
pub fn parse_ispdb(xml: &str) -> IspdbConfig {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut cfg = IspdbConfig::default();
    // Which server element we are inside: Some("imap") / Some("smtp").
    let mut server: Option<&'static str> = None;
    let mut field: Option<String> = None;
    let mut host = String::new();
    let mut port: Option<u16> = None;
    let mut security: Option<&'static str> = None;
    let mut in_display_name = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name();
                let tag = String::from_utf8_lossy(name.as_ref()).into_owned();
                let server_type = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"type")
                    .map(|a| String::from_utf8_lossy(&a.value).into_owned());
                match tag.as_str() {
                    "incomingServer" if server_type.as_deref() == Some("imap") => {
                        server = Some("imap");
                        (host, port, security) = (String::new(), None, None);
                    }
                    "outgoingServer" if server_type.as_deref() == Some("smtp") => {
                        server = Some("smtp");
                        (host, port, security) = (String::new(), None, None);
                    }
                    "displayName" => in_display_name = true,
                    _ if server.is_some() => field = Some(tag),
                    _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                let text = t.xml_content(quick_xml::XmlVersion::Implicit1_0).unwrap_or_default().into_owned();
                if in_display_name && cfg.provider.is_none() {
                    cfg.provider = Some(text);
                } else if server.is_some() {
                    match field.as_deref() {
                        Some("hostname") => host = text,
                        Some("port") => port = text.parse().ok(),
                        Some("socketType") => security = security_from_socket_type(&text),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name();
                let tag = name.as_ref();
                if tag == b"displayName" {
                    in_display_name = false;
                } else if tag == b"incomingServer" || tag == b"outgoingServer" {
                    if let (Some(kind), Some(port), Some(security)) = (server, port, security) {
                        let valid = validate::host("host", &host).is_ok()
                            && validate::port("port", port).is_ok();
                        let slot = if kind == "imap" {
                            &mut cfg.imap
                        } else {
                            &mut cfg.smtp
                        };
                        if valid && slot.is_none() {
                            *slot = Some(Endpoint {
                                host: host.clone(),
                                port,
                                security,
                                source: "ispdb",
                            });
                        }
                    }
                    server = None;
                } else {
                    field = None;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    cfg
}

async fn ispdb_lookup(domain: &str) -> Option<IspdbConfig> {
    let client = reqwest::Client::builder()
        .timeout(ISPDB_TIMEOUT)
        .build()
        .ok()?;
    let res = client
        .get(format!("{ISPDB_BASE}/{domain}"))
        .send()
        .await
        .ok()?;
    if !res.status().is_success() {
        return None;
    }
    let body = res.text().await.ok()?;
    let cfg = parse_ispdb(&body);
    (cfg.imap.is_some() || cfg.smtp.is_some()).then_some(cfg)
}

/// Parent domain of an MX target, used for a second-chance ISPDB lookup when
/// a custom domain is hosted at a known provider (mail.example-isp.com →
/// example-isp.com). Returns `None` when stripping a label leaves nothing
/// useful or just the original mail domain again.
pub fn mx_parent_domain(mx_host: &str, mail_domain: &str) -> Option<String> {
    let parent = mx_host.split_once('.')?.1;
    (parent.contains('.') && parent != mail_domain).then(|| parent.to_string())
}

/// Ordered candidate hosts for the reachability probe: conventional
/// subdomain first, then `mail.<domain>`, then MX targets (already sorted by
/// preference). Duplicates are dropped while preserving order.
pub fn candidate_hosts(prefix: &str, domain: &str, mx: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    [format!("{prefix}.{domain}"), format!("mail.{domain}")]
        .into_iter()
        .chain(mx.iter().cloned())
        .filter(|h| !h.is_empty() && seen.insert(h.clone()))
        .collect()
}

async fn srv_endpoint(
    resolver: &TokioResolver,
    domain: &str,
    services: &[(&str, &'static str)],
) -> Option<Endpoint> {
    for &(service, security) in services {
        let Ok(lookup) = resolver.srv_lookup(format!("{service}.{domain}.")).await else {
            continue;
        };
        // A single record with target "." means "service decidedly absent"
        // (RFC 2782); skip those.
        let best = lookup
            .answers()
            .iter()
            .filter_map(|rec| match &rec.data {
                hickory_resolver::proto::rr::RData::SRV(srv) => Some(srv),
                _ => None,
            })
            .filter(|r| r.target.to_utf8() != ".")
            .min_by_key(|r| (r.priority, u16::MAX - r.weight))?;
        let host = best.target.to_utf8().trim_end_matches('.').to_string();
        if validate::host("host", &host).is_ok() && validate::port("port", best.port).is_ok() {
            return Some(Endpoint {
                host,
                port: best.port,
                security,
                source: "srv",
            });
        }
    }
    None
}

/// Upgrade a plaintext IMAP (143) or SMTP submission (587) connection via
/// STARTTLS far enough to attempt the TLS handshake.
async fn starttls_upgrade(stream: TcpStream, port: u16) -> Option<TcpStream> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if port == 143 {
        reader.read_line(&mut line).await.ok()?; // "* OK ..." greeting
        reader.get_mut().write_all(b"a STARTTLS\r\n").await.ok()?;
        loop {
            line.clear();
            if reader.read_line(&mut line).await.ok()? == 0 {
                return None;
            }
            if line.starts_with("a ") {
                break;
            }
        }
        line.starts_with("a OK").then(|| reader.into_inner())
    } else {
        reader.read_line(&mut line).await.ok()?;
        if !line.starts_with("220") {
            return None;
        }
        reader
            .get_mut()
            .write_all(b"EHLO autodiscover.invalid\r\n")
            .await
            .ok()?;
        loop {
            line.clear();
            if reader.read_line(&mut line).await.ok()? == 0 || !line.starts_with("250") {
                return None;
            }
            if line.starts_with("250 ") {
                break;
            }
        }
        reader.get_mut().write_all(b"STARTTLS\r\n").await.ok()?;
        line.clear();
        reader.read_line(&mut line).await.ok()?;
        line.starts_with("220").then(|| reader.into_inner())
    }
}

/// Fetch the certificate a server presents, without trusting it — used to
/// show the user the details of an untrusted certificate before they decide
/// to add a trust exception. Returns the DER-encoded leaf certificate.
/// Port 143/587 are upgraded via STARTTLS first.
pub async fn fetch_peer_cert(host: &str, port: u16) -> Option<Vec<u8>> {
    let attempt = async {
        let stream = TcpStream::connect((host, port)).await.ok()?;
        let stream = match port {
            143 | 587 => starttls_upgrade(stream, port).await?,
            _ => stream,
        };
        let tls = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true)
            .build()
            .ok()?;
        let tls = tokio_native_tls::TlsConnector::from(tls);
        let stream = tls.connect(host, stream).await.ok()?;
        stream
            .get_ref()
            .peer_certificate()
            .ok()
            .flatten()?
            .to_der()
            .ok()
    };
    timeout(PROBE_TIMEOUT, attempt).await.ok().flatten()
}

/// Hex-encoded SHA-256 fingerprint of a DER certificate, as shown in the
/// trust-exception UI.
pub fn cert_fingerprint_sha256(der: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(der))
}

/// A candidate only counts as reachable when the TLS handshake verifies the
/// certificate against the probed hostname (directly for implicit TLS, after
/// the protocol upgrade for STARTTLS). A plain TCP check is not enough: with
/// wildcard DNS plus a shared mail host, `imap.<domain>` can accept the
/// connection but present a certificate for a different name, which then
/// fails account setup.
async fn endpoint_usable(host: String, port: u16, security: &'static str) -> bool {
    let attempt = async {
        let stream = TcpStream::connect((host.as_str(), port)).await.ok()?;
        let tls = native_tls::TlsConnector::new().ok()?;
        let tls = tokio_native_tls::TlsConnector::from(tls);
        let stream = match security {
            "ssl" => stream,
            "starttls" => starttls_upgrade(stream, port).await?,
            _ => return Some(()),
        };
        tls.connect(&host, stream).await.ok().map(|_| ())
    };
    matches!(timeout(PROBE_TIMEOUT, attempt).await, Ok(Some(())))
}

/// Probe all candidate host/port pairs in parallel, then pick the winner by
/// candidate order (host first, then port preference).
async fn probe_endpoint(hosts: &[String], ports: &[(u16, &'static str)]) -> Option<Endpoint> {
    let mut tasks = Vec::new();
    for host in hosts {
        for &(port, security) in ports {
            let host = host.clone();
            tasks.push(tokio::spawn(async move {
                let usable = endpoint_usable(host.clone(), port, security).await;
                (host, port, usable)
            }));
        }
    }
    let mut usable = HashSet::new();
    for task in tasks {
        if let Ok((host, port, true)) = task.await {
            usable.insert((host, port));
        }
    }
    for host in hosts {
        for &(port, security) in ports {
            if usable.contains(&(host.clone(), port)) {
                return Some(Endpoint {
                    host: host.clone(),
                    port,
                    security,
                    source: "probe",
                });
            }
        }
    }
    None
}

async fn mx_targets(resolver: &TokioResolver, domain: &str) -> Vec<String> {
    let Ok(lookup) = resolver.mx_lookup(domain).await else {
        return Vec::new();
    };
    let mut records: Vec<(u16, String)> = lookup
        .answers()
        .iter()
        .filter_map(|rec| match &rec.data {
            hickory_resolver::proto::rr::RData::MX(mx) => Some(mx),
            _ => None,
        })
        .map(|r| {
            (
                r.preference,
                r.exchange.to_utf8().trim_end_matches('.').to_string(),
            )
        })
        .filter(|(_, h)| !h.is_empty() && validate::host("host", h).is_ok())
        .collect();
    records.sort();
    records.into_iter().map(|(_, h)| h).collect()
}

pub async fn discover(Query(q): Query<DiscoverQuery>) -> Result<impl IntoResponse, AppError> {
    validate::email("email", &q.email)?;
    let domain = q
        .email
        .split('@')
        .nth(1)
        .unwrap_or_default()
        .to_ascii_lowercase();

    let resolver = TokioResolver::builder_tokio()
        .map_err(|e| AppError::Internal(format!("dns resolver: {e}")))?
        .build()
        .map_err(|e| AppError::Internal(format!("dns resolver: {e}")))?;

    let imap_srv = srv_endpoint(
        &resolver,
        &domain,
        &[("_imaps._tcp", "ssl"), ("_imap._tcp", "starttls")],
    );
    let smtp_srv = srv_endpoint(
        &resolver,
        &domain,
        &[
            ("_submissions._tcp", "ssl"),
            ("_submission._tcp", "starttls"),
        ],
    );
    let (mut imap, mut smtp) = tokio::join!(imap_srv, smtp_srv);
    let mut provider: Option<String> = None;

    if imap.is_none() || smtp.is_none() {
        if let Some(cfg) = ispdb_lookup(&domain).await {
            provider = cfg.provider;
            imap = imap.or(cfg.imap);
            smtp = smtp.or(cfg.smtp);
        }
    }

    if imap.is_none() || smtp.is_none() {
        let mx = mx_targets(&resolver, &domain).await;
        if let Some(cfg) = hosted_provider_from_mx(&mx) {
            provider = provider.or(cfg.provider);
            imap = imap.or(cfg.imap);
            smtp = smtp.or(cfg.smtp);
        }

        // Custom domain hosted at a known provider: ask the ISPDB about the
        // MX target's parent domain (mail.example-isp.com → example-isp.com).
        if let Some(parent) = mx.first().and_then(|h| mx_parent_domain(h, &domain)) {
            if let Some(cfg) = ispdb_lookup(&parent).await {
                provider = provider.or(cfg.provider);
                imap = imap.or(cfg.imap);
                smtp = smtp.or(cfg.smtp);
            }
        }
        let imap_probe = async {
            match imap.take() {
                Some(e) => Some(e),
                None => {
                    probe_endpoint(
                        &candidate_hosts("imap", &domain, &mx),
                        &[(993, "ssl"), (143, "starttls")],
                    )
                    .await
                }
            }
        };
        let smtp_probe = async {
            match smtp.take() {
                Some(e) => Some(e),
                None => {
                    probe_endpoint(
                        &candidate_hosts("smtp", &domain, &mx),
                        &[(465, "ssl"), (587, "starttls")],
                    )
                    .await
                }
            }
        };
        (imap, smtp) = tokio::join!(imap_probe, smtp_probe);
    }

    Ok(Json(
        json!({ "domain": domain, "provider": provider, "imap": imap, "smtp": smtp }),
    ))
}

#[cfg(test)]
mod tests {
    use super::hosted_provider_from_mx;

    #[test]
    fn detects_google_workspace_from_mx() {
        let mx = vec![
            "aspmx.l.google.com".to_owned(),
            "alt1.aspmx.l.google.com".to_owned(),
        ];
        let cfg = hosted_provider_from_mx(&mx).unwrap();
        assert_eq!(cfg.provider.as_deref(), Some("Google Workspace"));
        assert_eq!(cfg.imap.unwrap().host, "imap.gmail.com");
        assert_eq!(cfg.smtp.unwrap().host, "smtp.gmail.com");
    }
}
