//! Integration tests for the zero-trust input validators (`api::validate`).

use api::validate::{email, host, http_url, one_of, port, text, AUTH_SCHEMES};

#[test]
fn email_accepts_valid_and_rejects_hostile() {
    assert!(email("e", "user@example.com").is_ok());
    assert!(email("e", "a.b+c@sub.example.co").is_ok());
    for bad in [
        "",
        "no-at",
        "two@@example.com",
        "user@nodot",
        "user@.example.com",
        " user@example.com",
        "user@exa mple.com",
        "user@example.com\n",
        "a@b.c<script>",
    ] {
        assert!(email("e", bad).is_err(), "should reject: {bad:?}");
    }
}

#[test]
fn host_rejects_injection_and_whitespace() {
    assert!(host("h", "imap.example.com").is_ok());
    assert!(host("h", "127.0.0.1").is_ok());
    for bad in [
        "",
        "has space",
        "bad_host",
        "http://x",
        "a;b",
        "-lead",
        ".lead",
        "x\n",
    ] {
        assert!(host("h", bad).is_err(), "should reject: {bad:?}");
    }
}

#[test]
fn port_rejects_zero() {
    assert!(port("p", 993).is_ok());
    assert!(port("p", 0).is_err());
}

#[test]
fn one_of_is_allow_list() {
    assert!(one_of("a", "plain", AUTH_SCHEMES).is_ok());
    assert!(one_of("a", "../etc/passwd", AUTH_SCHEMES).is_err());
}

#[test]
fn http_url_scheme_restricted() {
    assert!(http_url("u", "https://dav.example.com/").is_ok());
    for bad in [
        "ftp://x",
        "javascript:alert(1)",
        "file:///etc/passwd",
        "https://x y",
    ] {
        assert!(http_url("u", bad).is_err(), "should reject: {bad:?}");
    }
}

#[test]
fn text_rejects_empty_and_control_chars() {
    assert!(text("t", "Frank Gehann", 200).is_ok());
    assert!(text("t", "   ", 200).is_err());
    assert!(text("t", "line\nbreak", 200).is_err());
    assert!(text("t", &"x".repeat(201), 200).is_err());
}
