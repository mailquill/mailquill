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
use std::net::IpAddr;
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

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .redirect(Policy::none())
        .user_agent("Mailquill remote image proxy")
        .build()
        .map_err(|err| AppError::Internal(err.to_string()))?;

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
    if let Err(err) = ensure_public_host(&url).await {
        return Ok(proxy_blocked_response(&url, err));
    }
    for _ in 0..=MAX_REDIRECTS {
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
            if let Err(err) = ensure_public_host(&url).await {
                return Ok(proxy_blocked_response(&url, err));
            }
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

        return Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&content_type)
                    .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
            )
            .header(header::CACHE_CONTROL, "private, max-age=3600")
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

fn is_private_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.octets()[0] == 0
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
        }
    }
}

async fn ensure_public_host(url: &Url) -> Result<(), AppError> {
    let host = url
        .host_str()
        .ok_or_else(|| AppError::BadGateway("image url has no host".into()))?;
    if host.parse::<IpAddr>().is_ok() {
        return Ok(());
    }

    let port = url
        .port_or_known_default()
        .ok_or_else(|| AppError::BadGateway("image url has no port".into()))?;
    let addrs = lookup_host((host, port))
        .await
        .map_err(|err| AppError::BadGateway(format!("image host lookup failed: {err}")))?;
    for addr in addrs {
        if is_private_ip(addr.ip()) {
            return Err(AppError::BadGateway(
                "private image hosts are not allowed".into(),
            ));
        }
    }

    Ok(())
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

fn empty_image_response() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header(header::CACHE_CONTROL, "private, max-age=300")
        .body(Body::empty())
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
