use crate::error::AppError;

const TLS_CERTIFICATE_INVALID: &str = "caldav_tls_certificate_invalid";
const AUTHENTICATION_FAILED: &str = "caldav_authentication_failed";
const ACCESS_DENIED: &str = "caldav_access_denied";
const TIMEOUT: &str = "caldav_timeout";
const CONNECTION_FAILED: &str = "caldav_connection_failed";
const ENDPOINT_NOT_FOUND: &str = "caldav_endpoint_not_found";
const RATE_LIMITED: &str = "caldav_rate_limited";
const SERVER_ERROR: &str = "caldav_server_error";
const INVALID_RESPONSE: &str = "caldav_invalid_response";
const NO_CALENDARS: &str = "caldav_no_calendars";
const DISCOVERY_FAILED: &str = "caldav_discovery_failed";

pub(super) fn curated_caldav_error(error: String) -> AppError {
    let primary = primary_attempt(&error).to_ascii_lowercase();
    let (code, message) = if primary.contains(calendar_sync::TLS_CERTIFICATE_ERROR_PREFIX) {
        (
            TLS_CERTIFICATE_INVALID,
            "CalDAV server certificate validation failed",
        )
    } else if primary.contains("401 unauthorized") {
        (AUTHENTICATION_FAILED, "CalDAV authentication failed")
    } else if primary.contains("403 forbidden") {
        (ACCESS_DENIED, "CalDAV access denied")
    } else if primary.contains("request timed out") {
        (TIMEOUT, "CalDAV server timed out")
    } else if primary.contains("connection failed") {
        (CONNECTION_FAILED, "CalDAV server connection failed")
    } else if primary.contains("404 not found") {
        (ENDPOINT_NOT_FOUND, "CalDAV endpoint not found")
    } else if primary.contains("429 too many requests") {
        (RATE_LIMITED, "CalDAV server rate limit exceeded")
    } else if contains_server_error_status(&primary) {
        (SERVER_ERROR, "CalDAV server returned an error")
    } else if primary.contains("no calendars found") {
        (NO_CALENDARS, "No accessible CalDAV calendars found")
    } else if [
        "url is invalid",
        "principal not found",
        "calendar-home-set not found",
        "xml",
        "invalid response",
    ]
    .iter()
    .any(|needle| primary.contains(needle))
    {
        (
            INVALID_RESPONSE,
            "CalDAV server returned an invalid response",
        )
    } else {
        (DISCOVERY_FAILED, "CalDAV discovery failed")
    };

    AppError::BadGatewayWithCode {
        code,
        message: message.to_owned(),
    }
}

fn primary_attempt(error: &str) -> &str {
    let attempts = error
        .strip_prefix("caldav discover failed: ")
        .or_else(|| error.strip_prefix("CalDAV discovery failed ("))
        .unwrap_or(error);
    attempts.split("; https://").next().unwrap_or(attempts)
}

fn contains_server_error_status(error: &str) -> bool {
    (500..=599).any(|status| error.contains(&format!("{status} ")))
}

#[cfg(test)]
mod tests {
    use super::{curated_caldav_error, primary_attempt};
    use crate::error::AppError;

    fn code(error: &str) -> &'static str {
        match curated_caldav_error(error.to_owned()) {
            AppError::BadGatewayWithCode { code, .. } => code,
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn classifies_primary_discovery_failures() {
        assert_eq!(
            code("caldav discover failed: https://dav.example.test/: tls certificate validation failed: request; https://example.test/: CalDAV PROPFIND -> 404 Not Found"),
            "caldav_tls_certificate_invalid"
        );
        assert_eq!(
            code("caldav discover failed: https://dav.example.test/: CalDAV PROPFIND -> 401 Unauthorized"),
            "caldav_authentication_failed"
        );
        assert_eq!(
            code("caldav discover failed: https://dav.example.test/: request timed out"),
            "caldav_timeout"
        );
        assert_eq!(
            code("caldav discover failed: https://dav.example.test/: no calendars found"),
            "caldav_no_calendars"
        );
        assert_eq!(
            code("caldav discover failed: https://dav.example.test/: CalDAV PROPFIND -> 429 Too Many Requests"),
            "caldav_rate_limited"
        );
    }

    #[test]
    fn ignores_failures_from_later_fallback_hosts() {
        let error = "caldav discover failed: https://example.test/.well-known/caldav: CalDAV PROPFIND -> 404 Not Found; https://mail.example.test/: connection failed";

        assert_eq!(
            primary_attempt(error),
            "https://example.test/.well-known/caldav: CalDAV PROPFIND -> 404 Not Found"
        );
        assert_eq!(code(error), "caldav_endpoint_not_found");
    }
}
