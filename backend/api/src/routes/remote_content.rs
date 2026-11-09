//! Proxy for remote mail images after the user explicitly allows loading them.
//!
//! Browser-side image loads cannot attach the API Authorization header and are
//! still subject to mixed-content and certificate UI behaviour. The reader
//! rewrites approved remote image URLs to this same-origin endpoint and passes
//! the short-lived access token as a query parameter, matching the SSE auth
//! pattern.

use axum::{
    body::Body,
    extract::{Query, State},
    http::{header, HeaderValue, Response, StatusCode},
};
use bytes::BytesMut;
use reqwest::{redirect::Policy, Url};
use serde::Deserialize;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::net::lookup_host;

use crate::{error::AppError, state::AppState};

const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_REDIRECTS: usize = 3;

#[derive(Deserialize)]
pub struct RemoteImageQuery {
    token: String,
    url: String,
}

pub async fn remote_image(
    State(state): State<AppState>,
    Query(query): Query<RemoteImageQuery>,
) -> Result<Response<Body>, AppError> {
    state
        .jwt_key
        .validate(&query.token)
        .map_err(|_| AppError::Unauthorized)?;

    let mut url = match parse_allowed_url(&query.url) {
        Ok(url) => url,
        Err(err) => {
            tracing::warn!(
                "remote image proxy rejected url: reason={err} url={}",
                sanitized_url(&query.url)
            );
            return Ok(empty_image_response());
        }
    };
    let mut addrs = match ensure_public_host(&url).await {
        Ok(addrs) => addrs,
        Err(err) => return Ok(proxy_blocked_response(&url, err)),
    };
    for _ in 0..=MAX_REDIRECTS {
        // The client connects to exactly the addresses that passed the check,
        // so a DNS answer that changes between the check and the request (DNS
        // rebinding) cannot redirect the fetch to an internal host.
        let client = pinned_client(&url, &addrs)?;
        let response = match client.get(url.clone()).send().await {
            Ok(response) => response,
            Err(err) => {
                return Ok(proxy_failure_response(
                    &url,
                    AppError::BadGateway(format!("image fetch failed: {err}")),
                ));
            }
        };

        if response.status().is_redirection() {
            let Some(location) = response
                .headers()
                .get(header::LOCATION)
                .and_then(|value| value.to_str().ok())
            else {
                return Ok(proxy_failure_response(
                    &url,
                    AppError::BadGateway("image redirect without location".into()),
                ));
            };
            let next_url = match url.join(location) {
                Ok(next_url) => next_url,
                Err(err) => {
                    return Ok(proxy_failure_response(
                        &url,
                        AppError::BadGateway(format!("invalid image redirect: {err}")),
                    ));
                }
            };
            url = match parse_allowed_url(next_url.as_str()) {
                Ok(url) => url,
                Err(err) => return Ok(proxy_failure_response(&url, err)),
            };
            addrs = match ensure_public_host(&url).await {
                Ok(addrs) => addrs,
                Err(err) => return Ok(proxy_blocked_response(&url, err)),
            };
            continue;
        }

        if !response.status().is_success() {
            return Ok(proxy_failure_response(
                &url,
                AppError::BadGateway(format!("image fetch failed with {}", response.status())),
            ));
        }

        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_owned();
        if !content_type
            .split(';')
            .next()
            .is_some_and(|kind| kind.trim().starts_with("image/"))
        {
            return Ok(proxy_failure_response(
                &url,
                AppError::BadGateway(format!("remote content is not an image: {content_type}")),
            ));
        }

        if response
            .content_length()
            .is_some_and(|len| len > MAX_IMAGE_BYTES as u64)
        {
            return Ok(proxy_failure_response(
                &url,
                AppError::BadGateway("remote image is too large".into()),
            ));
        }

        let mut body = BytesMut::new();
        let mut response = response;
        loop {
            let chunk = match response.chunk().await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break,
                Err(err) => {
                    return Ok(proxy_failure_response(
                        &url,
                        AppError::BadGateway(format!("image read failed: {err}")),
                    ));
                }
            };
            if body.len() + chunk.len() > MAX_IMAGE_BYTES {
                return Ok(proxy_failure_response(
                    &url,
                    AppError::BadGateway("remote image is too large".into()),
                ));
            }
            body.extend_from_slice(&chunk);
        }

        // The bytes are mail-controlled. `<img>` loads ignore both headers,
        // but if the proxied URL is ever opened as a document (an SVG with
        // script, say), the sandbox keeps it off our origin and nosniff
        // stops the browser from reinterpreting the type.
        return Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&content_type)
                    .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
            )
            .header(header::CACHE_CONTROL, "private, max-age=3600")
            .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
            .header(header::CONTENT_SECURITY_POLICY, "sandbox; default-src 'none'")
            .body(Body::from(body.freeze()))
            .map_err(|err| AppError::Internal(err.to_string()));
    }

    Ok(proxy_failure_response(
        &url,
        AppError::BadGateway("too many image redirects".into()),
    ))
}

fn parse_allowed_url(raw: &str) -> Result<Url, AppError> {
    let url =
        Url::parse(raw).map_err(|err| AppError::BadGateway(format!("invalid image url: {err}")))?;
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err(AppError::BadGateway("unsupported image url scheme".into())),
    }

    let host = url
        .host_str()
        .ok_or_else(|| AppError::BadGateway("image url has no host".into()))?;
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
        return Err(AppError::BadGateway(
            "local image hosts are not allowed".into(),
        ));
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_ip(ip) {
            return Err(AppError::BadGateway(
                "private image hosts are not allowed".into(),
            ));
        }
    }

    Ok(url)
}

/// Addresses the proxy must never connect to: anything that is not a public
/// unicast address, including the IPv6 forms that embed an IPv4 address
/// (`::ffff:10.0.0.1`, NAT64) which would otherwise slip past the IPv4 rules.
fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_private_ipv4(ip),
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_private_ipv4(mapped);
            }
            let segments = ip.segments();
            // NAT64 well-known prefix 64:ff9b::/96 carries an IPv4 address.
            if segments[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
                let [a, b] = [segments[6], segments[7]];
                return is_private_ipv4(Ipv4Addr::new(
                    (a >> 8) as u8,
                    a as u8,
                    (b >> 8) as u8,
                    b as u8,
                ));
            }
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_multicast()
                // 2001:db8::/32 documentation
                || (segments[0] == 0x2001 && segments[1] == 0xdb8)
        }
    }
}

fn is_private_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || octets[0] == 0
        // 100.64.0.0/10 shared address space (carrier-grade NAT)
        || (octets[0] == 100 && (octets[1] & 0xc0) == 64)
        // 240.0.0.0/4 reserved
        || octets[0] >= 240
}

/// Resolve the URL's host and reject it unless every address is public.
/// Returns the vetted addresses so the request can be pinned to them.
async fn ensure_public_host(url: &Url) -> Result<Vec<SocketAddr>, AppError> {
    let host = url
        .host_str()
        .ok_or_else(|| AppError::BadGateway("image url has no host".into()))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| AppError::BadGateway("image url has no port".into()))?;
    let literal = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<IpAddr>()
        .ok();
    let addrs: Vec<SocketAddr> = match literal {
        Some(ip) => vec![SocketAddr::new(ip, port)],
        None => lookup_host((host, port))
            .await
            .map_err(|err| AppError::BadGateway(format!("image host lookup failed: {err}")))?
            .collect(),
    };
    if addrs.is_empty() {
        return Err(AppError::BadGateway("image host did not resolve".into()));
    }
    if addrs.iter().any(|addr| is_private_ip(addr.ip())) {
        return Err(AppError::BadGateway(
            "private image hosts are not allowed".into(),
        ));
    }
    Ok(addrs)
}

/// An HTTP client that connects to `addrs` for the URL's host, bypassing a
/// second DNS lookup.
fn pinned_client(url: &Url, addrs: &[SocketAddr]) -> Result<reqwest::Client, AppError> {
    let host = url
        .host_str()
        .ok_or_else(|| AppError::BadGateway("image url has no host".into()))?;
    reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .redirect(Policy::none())
        .user_agent("Mailquill remote image proxy")
        .resolve_to_addrs(host, addrs)
        .build()
        .map_err(|err| AppError::Internal(err.to_string()))
}

/// Upstream didn't deliver a usable image (4xx/5xx, timeout, wrong content
/// type, oversized). Routine for tracking pixels and expired links — nothing
/// an operator can act on, so keep it out of the warn log.
fn proxy_failure_response(url: &Url, err: AppError) -> Response<Body> {
    tracing::debug!(
        "remote image proxy failed: host={} url={} reason={err}",
        url.host_str().unwrap_or("<none>"),
        sanitized_url(url.as_str())
    );
    empty_image_response()
}

/// The proxy refused to fetch (private/unresolvable host). Unlike upstream
/// failures this can indicate a probe against internal addresses, so it stays
/// on warn.
fn proxy_blocked_response(url: &Url, err: AppError) -> Response<Body> {
    tracing::warn!(
        "remote image proxy blocked request: host={} url={} reason={err}",
        url.host_str().unwrap_or("<none>"),
        sanitized_url(url.as_str())
    );
    empty_image_response()
}

/// A 1x1 transparent GIF. Upstream failures (dead link, blocked host, wrong
/// content type, ...) must still resolve as a *successful* image load —
/// `204 No Content` has no decodable body, so the browser fires the `<img>`
/// `error` event and renders the native broken-image icon instead of the
/// blank space a failed/expired remote image should be.
const TRANSPARENT_GIF: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00,
    0x00, 0xFF, 0xFF, 0xFF, 0x21, 0xF9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2C, 0x00, 0x00,
    0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3B,
];

fn empty_image_response() -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/gif")
        .header(header::CACHE_CONTROL, "private, max-age=300")
        .body(Body::from(TRANSPARENT_GIF))
        .unwrap()
}

fn sanitized_url(raw: &str) -> String {
    match Url::parse(raw) {
        Ok(mut url) => {
            url.set_query(None);
            url.to_string()
        }
        Err(_) => raw.split('?').next().unwrap_or(raw).to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A dead link, blocked host, or non-image upstream response must still
    // resolve as a *successful* <img> load — 204 has no decodable body, so
    // the browser fires the error event and shows a broken-image icon
    // instead of the blank space a failed remote image should render as.
    #[test]
    fn embedded_ipv4_forms_are_treated_like_their_ipv4_address() {
        for blocked in [
            "::ffff:127.0.0.1",
            "::ffff:10.1.2.3",
            "::ffff:169.254.169.254",
            "64:ff9b::7f00:1",
            "fc00::1",
            "2001:db8::1",
        ] {
            assert!(is_private_ip(blocked.parse().unwrap()), "{blocked}");
        }
        for blocked in ["100.64.0.1", "0.0.0.0", "240.0.0.1", "224.0.0.1", "169.254.169.254"] {
            assert!(is_private_ip(blocked.parse().unwrap()), "{blocked}");
        }
        for allowed in ["93.184.216.34", "2606:4700::6810:84e5", "::ffff:93.184.216.34"] {
            assert!(!is_private_ip(allowed.parse().unwrap()), "{allowed}");
        }
    }

    #[test]
    fn empty_image_response_is_a_decodable_image_not_no_content() {
        let response = empty_image_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/gif"
        );
    }
}
