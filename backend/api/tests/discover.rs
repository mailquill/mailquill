//! Tests for autodiscovery: candidate ordering, ISPDB XML parsing, and the
//! MX parent-domain heuristic (`api::routes::discover`).

use api::routes::discover::{candidate_hosts, mx_parent_domain, parse_ispdb};

#[test]
fn candidates_prefer_convention_then_mail_then_mx() {
    let mx = vec![
        "mx1.hoster.example".to_string(),
        "mx2.hoster.example".to_string(),
    ];
    assert_eq!(
        candidate_hosts("imap", "fgehann.de", &mx),
        vec![
            "imap.fgehann.de",
            "mail.fgehann.de",
            "mx1.hoster.example",
            "mx2.hoster.example"
        ]
    );
}

#[test]
fn candidates_dedupe_mx_matching_convention() {
    // MX pointing at mail.<domain> must not appear twice.
    let mx = vec!["mail.fgehann.de".to_string()];
    assert_eq!(
        candidate_hosts("smtp", "fgehann.de", &mx),
        vec!["smtp.fgehann.de", "mail.fgehann.de"]
    );
}

#[test]
fn candidates_skip_empty_mx_targets() {
    let mx = vec![String::new()];
    assert_eq!(
        candidate_hosts("imap", "example.org", &mx),
        vec!["imap.example.org", "mail.example.org"]
    );
}

const ISPDB_SAMPLE: &str = r#"<?xml version="1.0"?>
<clientConfig version="1.1">
  <emailProvider id="googlemail.com">
    <domain>gmail.com</domain>
    <domain>googlemail.com</domain>
    <displayName>Google Mail</displayName>
    <displayShortName>GMail</displayShortName>
    <incomingServer type="pop3">
      <hostname>pop.gmail.com</hostname>
      <port>995</port>
      <socketType>SSL</socketType>
    </incomingServer>
    <incomingServer type="imap">
      <hostname>imap.gmail.com</hostname>
      <port>993</port>
      <socketType>SSL</socketType>
      <username>%EMAILADDRESS%</username>
      <authentication>OAuth2</authentication>
    </incomingServer>
    <outgoingServer type="smtp">
      <hostname>smtp.gmail.com</hostname>
      <port>465</port>
      <socketType>SSL</socketType>
    </outgoingServer>
  </emailProvider>
</clientConfig>"#;

#[test]
fn ispdb_parses_first_imap_and_smtp_servers() {
    let cfg = parse_ispdb(ISPDB_SAMPLE);
    assert_eq!(cfg.provider.as_deref(), Some("Google Mail"));
    let imap = cfg.imap.expect("imap endpoint");
    assert_eq!(
        (imap.host.as_str(), imap.port, imap.security),
        ("imap.gmail.com", 993, "ssl")
    );
    let smtp = cfg.smtp.expect("smtp endpoint");
    assert_eq!(
        (smtp.host.as_str(), smtp.port, smtp.security),
        ("smtp.gmail.com", 465, "ssl")
    );
}

#[test]
fn ispdb_skips_servers_with_invalid_values() {
    let xml = r#"<clientConfig><emailProvider>
      <incomingServer type="imap">
        <hostname>bad host name</hostname><port>993</port><socketType>SSL</socketType>
      </incomingServer>
      <outgoingServer type="smtp">
        <hostname>smtp.ok.example</hostname><port>0</port><socketType>SSL</socketType>
      </outgoingServer>
    </emailProvider></clientConfig>"#;
    let cfg = parse_ispdb(xml);
    assert!(cfg.imap.is_none());
    assert!(cfg.smtp.is_none());
}

#[test]
fn ispdb_handles_garbage_input() {
    let cfg = parse_ispdb("not xml at all");
    assert!(cfg.provider.is_none() && cfg.imap.is_none() && cfg.smtp.is_none());
}

#[test]
fn mx_parent_strips_one_label() {
    assert_eq!(
        mx_parent_domain("mail.code-works.de", "fgehann.de"),
        Some("code-works.de".to_string())
    );
    // Parent collapsing back to the mail domain itself is pointless.
    assert_eq!(mx_parent_domain("mx.fgehann.de", "fgehann.de"), None);
    // Single-label parents (TLD only) are not lookup-worthy.
    assert_eq!(mx_parent_domain("example.de", "fgehann.de"), None);
}
