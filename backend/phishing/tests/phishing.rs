use phishing::{analyse, load_brands_file, OpenPhishFeed};

/// The shipped brand list, loaded from the crate's brands.json at test time.
fn bundled_brands() -> Vec<(String, String)> {
    load_brands_file(concat!(env!("CARGO_MANIFEST_DIR"), "/brands.json"))
}

fn raw(headers: &str, html: Option<&str>) -> Vec<u8> {
    match html {
        Some(body) => format!("{headers}Content-Type: text/html; charset=utf-8\r\n\r\n{body}\r\n")
            .into_bytes(),
        None => format!("{headers}\r\n").into_bytes(),
    }
}

#[test]
fn clean_message_scores_zero() {
    let msg = raw(
        "From: Alice <alice@example.com>\r\nSubject: Hello\r\n",
        Some("<p>Hi there</p>"),
    );
    let report = analyse(&msg, &bundled_brands(), &OpenPhishFeed::default());
    assert_eq!(report.verdict, "clean");
    assert!(report.checks.is_empty());
}

#[test]
fn display_name_spoof_fires() {
    // The reported real-world case: ELSTER display name, unrelated domain.
    let msg = raw(
        "From: ELSTER-Benachrichtigung <news@bombeirosgondomar.pt>\r\nSubject: Einkommensteuer 2025\r\n",
        None,
    );
    let report = analyse(&msg, &bundled_brands(), &OpenPhishFeed::default());
    assert!(report.checks.iter().any(|c| c.id == "display_name_spoof"));
    assert_ne!(report.verdict, "clean");
}

#[test]
fn legitimate_brand_domain_does_not_fire() {
    let msg = raw("From: PayPal <service@paypal.com>\r\n", None);
    let report = analyse(&msg, &bundled_brands(), &OpenPhishFeed::default());
    assert!(report.checks.is_empty(), "checks: {:?}", report.checks);
}

#[test]
fn brand_subdomain_does_not_fire() {
    let msg = raw("From: PayPal <service@mail.paypal.com>\r\n", None);
    let report = analyse(&msg, &bundled_brands(), &OpenPhishFeed::default());
    assert!(!report.checks.iter().any(|c| c.id == "display_name_spoof"));
}

#[test]
fn regional_brand_domain_does_not_fire() {
    // paypal.de is a legitimate PayPal domain; claiming "PayPal" from it must
    // not flag as spoofing, even though the list also has paypal.com.
    let msg = raw("From: PayPal <service@paypal.de>\r\n", None);
    let report = analyse(&msg, &bundled_brands(), &OpenPhishFeed::default());
    assert!(report.checks.is_empty(), "checks: {:?}", report.checks);
}

#[test]
fn token_matching_avoids_substring_false_positive() {
    // "Marketing" must not match the brand "ING".
    let msg = raw("From: Marketing Team <team@example.com>\r\n", None);
    let report = analyse(&msg, &bundled_brands(), &OpenPhishFeed::default());
    assert!(!report.checks.iter().any(|c| c.id == "display_name_spoof"));
}

#[test]
fn auth_failures_score() {
    let msg = raw(
        "From: Bob <bob@example.com>\r\nAuthentication-Results: mx.example.com; spf=fail; dkim=fail; dmarc=fail\r\n",
        None,
    );
    let report = analyse(&msg, &bundled_brands(), &OpenPhishFeed::default());
    assert_eq!(report.score, 95);
    assert_eq!(report.verdict, "phishing");
}

#[test]
fn reply_to_and_return_path_mismatch() {
    let msg = raw(
        "From: ceo@company.com\r\nReply-To: ceo@gmail.com\r\nReturn-Path: <bounce@bulk-mailer.com>\r\n",
        None,
    );
    let report = analyse(&msg, &bundled_brands(), &OpenPhishFeed::default());
    assert!(report.checks.iter().any(|c| c.id == "reply_to_mismatch"));
    assert!(report.checks.iter().any(|c| c.id == "return_path_mismatch"));
    assert_eq!(report.verdict, "suspicious");
}

#[test]
fn same_org_subdomains_do_not_fire() {
    // Reported false positive: psrp.animexx.de / ssl.animexx.de are the same
    // organisation as animexx.de.
    let body = r#"<a href="https://ssl.animexx.de/login">www.animexx.de</a>"#;
    let msg = raw(
        "From: Animexx <noreply@animexx.de>\r\nReturn-Path: <bounce@psrp.animexx.de>\r\nReply-To: kontakt@mail.animexx.de\r\n",
        Some(body),
    );
    let report = analyse(&msg, &bundled_brands(), &OpenPhishFeed::default());
    assert!(report.checks.is_empty(), "checks: {:?}", report.checks);
    assert_eq!(report.verdict, "clean");
}

#[test]
fn multi_part_tld_not_treated_as_org() {
    // evil.co.uk and bank.co.uk share only the public suffix — must fire.
    let msg = raw("From: x@bank.co.uk\r\nReply-To: y@evil.co.uk\r\n", None);
    let report = analyse(&msg, &bundled_brands(), &OpenPhishFeed::default());
    assert!(report.checks.iter().any(|c| c.id == "reply_to_mismatch"));
}

#[test]
fn domain_lookalike_fires() {
    let msg = raw("From: Support <support@paypa1.com>\r\n", None);
    let report = analyse(&msg, &bundled_brands(), &OpenPhishFeed::default());
    assert!(report.checks.iter().any(|c| c.id == "domain_lookalike"));
}

#[test]
fn punycode_domain_fires() {
    let msg = raw("From: Service <service@xn--pypal-4ve.com>\r\n", None);
    let report = analyse(&msg, &bundled_brands(), &OpenPhishFeed::default());
    assert!(report.checks.iter().any(|c| c.id == "idn_homograph"));
}

#[test]
fn link_text_href_mismatch_fires_and_caps() {
    let body = r#"
        <a href="https://evil.com/a">https://paypal.com</a>
        <a href="https://evil.com/b">www.amazon.de</a>
        <a href="https://evil.com/c">https://sparkasse.de/login</a>
        <a href="https://evil.com/d">https://dkb.de</a>
        <a href="https://example.com/ok">Click here</a>
    "#;
    let msg = raw("From: Newsletter <sender@example.com>\r\n", Some(body));
    let report = analyse(&msg, &bundled_brands(), &OpenPhishFeed::default());
    let link_points: i32 = report
        .checks
        .iter()
        .filter(|c| c.id == "link_mismatch")
        .map(|c| c.points)
        .sum();
    assert_eq!(link_points, 60, "capped at +60: {:?}", report.checks);
}

#[test]
fn social_handles_in_link_text_do_not_fire() {
    // Reported false positive: Instagram story-recap mails render usernames
    // like `hebamme.aachen` as anchor text while every link points to
    // instagram.com. A dotted handle must not be read as a spoofed domain.
    let body = r#"
        <a href="https://www.instagram.com/hebamme.aachen">hebamme.aachen</a>
        <a href="https://www.instagram.com/dany.thewarning">dany.thewarning</a>
        <a href="https://www.instagram.com/paulina.thewarning">paulina.thewarning</a>
    "#;
    let msg = raw(
        "From: Instagram <stories-recap@mail.instagram.com>\r\nReturn-Path: <stories-recap@mail.instagram.com>\r\n",
        Some(body),
    );
    let report = analyse(&msg, &bundled_brands(), &OpenPhishFeed::default());
    assert!(
        !report.checks.iter().any(|c| c.id == "link_mismatch"),
        "checks: {:?}",
        report.checks
    );
    assert_eq!(report.verdict, "clean");
}

#[test]
fn custom_brand_entry_is_used() {
    let mut brands = vec![("acme-corp.com".to_string(), "ACME Corp".to_string())];
    brands.extend(bundled_brands());
    let msg = raw("From: ACME Corp Billing <billing@randomhost.net>\r\n", None);
    let report = analyse(&msg, &brands, &OpenPhishFeed::default());
    assert!(report.checks.iter().any(|c| c.id == "display_name_spoof"));
}

fn feed_with(urls: &[&str]) -> OpenPhishFeed {
    let mut feed = OpenPhishFeed::default();
    for url in urls {
        feed.urls.insert(url.trim_end_matches('/').to_lowercase());
        if let Some(rest) = url.split("://").nth(1) {
            if let Some(host) = rest.split(['/', '?', '#']).next() {
                feed.domains.insert(host.to_lowercase());
            }
        }
    }
    feed
}

#[test]
fn openphish_exact_url_hit_is_phishing() {
    let body = r#"<a href="https://evil.example/steal/login">Click here</a>"#;
    let msg = raw("From: Newsletter <sender@example.com>\r\n", Some(body));
    let feed = feed_with(&["https://evil.example/steal/login"]);
    let report = analyse(&msg, &bundled_brands(), &feed);
    assert!(report.checks.iter().any(|c| c.id == "openphish_url"));
    assert_eq!(report.verdict, "phishing");
}

#[test]
fn openphish_domain_hit_is_suspicious() {
    let body = r#"<a href="https://evil.example/other/path">Click here</a>"#;
    let msg = raw("From: Newsletter <sender@example.com>\r\n", Some(body));
    let feed = feed_with(&["https://evil.example/steal/login"]);
    let report = analyse(&msg, &bundled_brands(), &feed);
    assert!(report.checks.iter().any(|c| c.id == "openphish_domain"));
    assert!(!report.checks.iter().any(|c| c.id == "openphish_url"));
}

#[test]
fn openphish_clean_link_no_hit() {
    let body = r#"<a href="https://example.com/news">Click here</a>"#;
    let msg = raw("From: Newsletter <sender@example.com>\r\n", Some(body));
    let feed = feed_with(&["https://evil.example/steal/login"]);
    let report = analyse(&msg, &bundled_brands(), &feed);
    assert!(report.checks.is_empty(), "checks: {:?}", report.checks);
}
