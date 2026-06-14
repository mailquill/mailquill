//! Rule-based phishing analysis (openspec change `phishing`).
//!
//! Transport-agnostic: analyses raw RFC 2822 messages regardless of how they
//! arrived (IMAP sync, on-demand fetch, future POP/JMAP). Works on the full
//! message or a header block only — body-dependent checks simply don't fire
//! then. Heuristics are local; the only network access is the optional
//! OpenPhish feed download. Scores are additive; thresholds map to a verdict
//! that is denormalized onto `messages.phishing_verdict` for cheap list
//! rendering, with the full check breakdown in `phishing_analysis`.

use mailparse::MailHeaderMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};
use std::time::Duration;
use tracing::{info, warn};

#[derive(Debug, Serialize)]
pub struct Check {
    pub id: &'static str,
    pub points: i32,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub score: i32,
    pub verdict: &'static str,
    pub checks: Vec<Check>,
}

const VERDICT_CLEAN: &str = "clean";
const VERDICT_SUSPICIOUS: &str = "suspicious";
const VERDICT_PHISHING: &str = "phishing";

// ── externally supplied data: brands file + OpenPhish feed ──────────────────

#[derive(Serialize, Deserialize)]
struct BrandEntry {
    domain: String,
    name: String,
}

/// OpenPhish community feed (one phishing URL per line), held in memory as
/// exact URLs plus their domains.
#[derive(Debug, Default, Clone)]
pub struct OpenPhishFeed {
    pub urls: HashSet<String>,
    pub domains: HashSet<String>,
}

static FILE_BRANDS: OnceLock<Vec<(String, String)>> = OnceLock::new();
static FEED: OnceLock<RwLock<OpenPhishFeed>> = OnceLock::new();

/// File name of the brand list, in both `<data_dir>` (operator override) and
/// the shipped default locations.
const BRANDS_FILE: &str = "brands.json";

/// Load a brands file by path, returning an empty list on any error. Exposed
/// for callers/tests that want to analyse with a specific brand list.
pub fn load_brands_file(path: impl AsRef<Path>) -> Vec<(String, String)> {
    read_brands_file(path.as_ref()).unwrap_or_default()
}

/// Read and parse a brands file, lowercasing the domain.
fn read_brands_file(path: &Path) -> Option<Vec<(String, String)>> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<Vec<BrandEntry>>(&text) {
        Ok(entries) => Some(
            entries
                .into_iter()
                .map(|e| (e.domain.to_lowercase(), e.name))
                .collect(),
        ),
        Err(e) => {
            warn!("phishing: {} is not valid brands JSON: {e}", path.display());
            None
        }
    }
}

/// Locate the shipped default `brands.json`. It is part of the repo/release,
/// not compiled into the binary, so it is found by path at runtime. Honours an
/// explicit override via the `MAILQUILL_BRANDS_FILE` env var, then tries paths
/// relative to the working directory and the executable.
fn shipped_brands_path() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(explicit) = std::env::var("MAILQUILL_BRANDS_FILE") {
        candidates.push(PathBuf::from(explicit));
    }
    candidates.push(PathBuf::from(BRANDS_FILE));
    candidates.push(PathBuf::from("backend/phishing").join(BRANDS_FILE));
    candidates.push(PathBuf::from("phishing").join(BRANDS_FILE));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(BRANDS_FILE));
            candidates.push(dir.join("share/mailquill").join(BRANDS_FILE));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Initialise external threat data. Loads brands (operator override in
/// `<data_dir>/brands.json`, else the shipped default — copied into data_dir
/// on first start so the operator has an editable copy), loads the cached
/// OpenPhish feed, and — when `feed_url` is set — spawns a 12-hour refresh.
pub async fn init(data_dir: &str, feed_url: Option<String>) {
    let dir = Path::new(data_dir);
    let _ = std::fs::create_dir_all(dir);
    let _ = FILE_BRANDS.set(load_brands(dir));

    let feed_lock = FEED.get_or_init(|| RwLock::new(OpenPhishFeed::default()));
    let cache_path = dir.join("openphish.txt");
    if let Ok(text) = std::fs::read_to_string(&cache_path) {
        let feed = parse_feed(&text);
        info!("openphish: loaded {} cached URLs", feed.urls.len());
        *feed_lock.write().unwrap() = feed;
    }

    match feed_url {
        Some(url) => {
            tokio::spawn(feed_refresh_loop(url, cache_path));
        }
        None => info!("openphish: feed download disabled"),
    }
}

fn load_brands(dir: &Path) -> Vec<(String, String)> {
    // 1. Operator override in the data dir wins.
    let override_path = dir.join(BRANDS_FILE);
    if let Some(brands) = read_brands_file(&override_path) {
        info!("phishing: loaded {} brands from {}", brands.len(), override_path.display());
        return brands;
    }

    // 2. Shipped default (repo/release). Seed a copy into the data dir so the
    //    operator has an editable file.
    if let Some(shipped) = shipped_brands_path() {
        if let Some(brands) = read_brands_file(&shipped) {
            if let Err(e) = std::fs::copy(&shipped, &override_path) {
                warn!("phishing: could not seed {}: {e}", override_path.display());
            }
            info!(
                "phishing: loaded {} brands from shipped {}",
                brands.len(),
                shipped.display()
            );
            return brands;
        }
    }

    warn!("phishing: no brands.json found; brand checks disabled");
    Vec::new()
}

fn parse_feed(text: &str) -> OpenPhishFeed {
    let mut feed = OpenPhishFeed::default();
    for line in text.lines() {
        let url = line.trim().trim_end_matches('/');
        if url.is_empty() || !url.starts_with("http") {
            continue;
        }
        if let Some(domain) = url_domain(url) {
            feed.domains.insert(domain);
        }
        feed.urls.insert(url.to_lowercase());
    }
    feed
}

async fn feed_refresh_loop(url: String, cache_path: PathBuf) {
    loop {
        match fetch_feed(&url).await {
            Ok(text) => {
                let feed = parse_feed(&text);
                info!("openphish: refreshed feed, {} URLs", feed.urls.len());
                if let Some(lock) = FEED.get() {
                    *lock.write().unwrap() = feed;
                }
                if let Err(e) = std::fs::write(&cache_path, &text) {
                    warn!("openphish: could not cache feed: {e}");
                }
            }
            Err(e) => warn!("openphish: feed download failed: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(12 * 3600)).await;
    }
}

async fn fetch_feed(url: &str) -> Result<String, reqwest::Error> {
    reqwest::Client::new()
        .get(url)
        .timeout(Duration::from_secs(60))
        .send()
        .await?
        .error_for_status()?
        .text()
        .await
}

fn current_brands() -> Vec<(String, String)> {
    FILE_BRANDS.get().cloned().unwrap_or_default()
}

fn verdict_for(score: i32) -> &'static str {
    match score {
        ..=20 => VERDICT_CLEAN,
        21..=55 => VERDICT_SUSPICIOUS,
        _ => VERDICT_PHISHING,
    }
}

fn domain_of(addr: &str) -> Option<String> {
    let addr = addr.trim().trim_end_matches('>');
    addr.rsplit_once('@').map(|(_, d)| d.trim().to_lowercase())
}

/// Domain without its last (TLD) label, for lookalike comparison.
fn sans_tld(domain: &str) -> &str {
    domain.rsplit_once('.').map(|(head, _)| head).unwrap_or(domain)
}

/// Registrable domain (eTLD+1-ish): `psrp.animexx.de` → `animexx.de`.
/// Subdomains of the same organisation are not a mismatch. Uses a small list
/// of common multi-part public suffixes instead of the full PSL.
fn registrable_domain(domain: &str) -> String {
    const MULTI_PART_SUFFIXES: &[&str] = &[
        "co.uk", "org.uk", "ac.uk", "gov.uk", "me.uk", "com.au", "net.au", "org.au",
        "co.nz", "com.br", "com.mx", "com.ar", "co.jp", "or.jp", "ne.jp", "co.kr",
        "com.tr", "com.pl", "com.cn", "com.hk", "com.sg", "com.tw", "co.in", "co.za",
    ];
    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() <= 2 {
        return domain.to_string();
    }
    let last_two = labels[labels.len() - 2..].join(".");
    if MULTI_PART_SUFFIXES.contains(&last_two.as_str()) && labels.len() >= 3 {
        labels[labels.len() - 3..].join(".")
    } else {
        last_two
    }
}

fn same_org(a: &str, b: &str) -> bool {
    registrable_domain(a) == registrable_domain(b)
}

/// Analyse a raw RFC 2822 message (full or header-only).
/// `brands` is the complete (domain, brand_name) list to check against;
/// `feed` is the current OpenPhish URL feed (empty feed = checks skipped).
pub fn analyse(raw: &[u8], brands: &[(String, String)], feed: &OpenPhishFeed) -> Report {
    let mut checks: Vec<Check> = Vec::new();

    let parsed = match mailparse::parse_mail(raw) {
        Ok(p) => p,
        Err(_) => {
            return Report { score: 0, verdict: VERDICT_CLEAN, checks }
        }
    };
    let headers = parsed.get_headers();

    // ── Authentication-Results (SPF / DKIM / DMARC) ─────────────────────────
    if let Some(auth) = headers.get_first_value("Authentication-Results") {
        let auth = auth.to_lowercase();
        if auth.contains("dmarc=fail") {
            checks.push(Check { id: "dmarc_fail", points: 40, detail: "DMARC validation failed".into() });
        }
        if auth.contains("spf=fail") {
            checks.push(Check { id: "spf_fail", points: 30, detail: "SPF validation failed".into() });
        }
        if auth.contains("dkim=fail") {
            checks.push(Check { id: "dkim_fail", points: 25, detail: "DKIM signature invalid".into() });
        }
    }

    // ── From header: display name + domain ──────────────────────────────────
    let from_raw = headers.get_first_value("From").unwrap_or_default();
    let (display_name, from_domain) = match mailparse::addrparse(&from_raw)
        .ok()
        .and_then(|list| list.extract_single_info())
    {
        Some(info) => (
            info.display_name.unwrap_or_default(),
            domain_of(&info.addr).unwrap_or_default(),
        ),
        None => (String::new(), domain_of(&from_raw).unwrap_or_default()),
    };

    // ── Reply-To / Return-Path domain mismatch ──────────────────────────────
    if !from_domain.is_empty() {
        if let Some(reply_to) = headers.get_first_value("Reply-To") {
            if let Some(rd) = domain_of(&reply_to) {
                if !same_org(&rd, &from_domain) {
                    checks.push(Check {
                        id: "reply_to_mismatch",
                        points: 20,
                        detail: format!("Reply-To domain ({rd}) differs from sender domain ({from_domain})"),
                    });
                }
            }
        }
        if let Some(return_path) = headers.get_first_value("Return-Path") {
            if let Some(rd) = domain_of(&return_path) {
                if !same_org(&rd, &from_domain) {
                    checks.push(Check {
                        id: "return_path_mismatch",
                        points: 20,
                        detail: format!("Return-Path domain ({rd}) differs from sender domain ({from_domain})"),
                    });
                }
            }
        }
    }

    // ── Brand checks: display-name spoofing + domain lookalike ──────────────
    let name_tokens: Vec<String> = display_name
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_owned())
        .collect();

    // A brand can have several legitimate domains (paypal.com + paypal.de,
    // amazon.com + amazon.de). Group them by brand name so a sender on ANY of
    // a brand's domains is not flagged as spoofing that brand.
    let mut domains_by_brand: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (brand_domain, brand_name) in brands {
        domains_by_brand
            .entry(brand_name.to_lowercase())
            .or_default()
            .push(brand_domain.to_lowercase());
    }

    let on_domain = |domain: &str| from_domain == domain || from_domain.ends_with(&format!(".{domain}"));

    for (brand_name_lower, brand_domains) in &domains_by_brand {
        // Sender legitimately on one of this brand's domains — never a spoof.
        if brand_domains.iter().any(|d| on_domain(d)) {
            continue;
        }

        let claims_brand = if brand_name_lower.contains(' ') {
            display_name.to_lowercase().contains(brand_name_lower.as_str())
        } else {
            // Token match avoids substring false positives ("ing" in "Marketing").
            name_tokens.iter().any(|t| t == brand_name_lower)
        };
        if claims_brand && !from_domain.is_empty() {
            let expected = brand_domains.join(", ");
            checks.push(Check {
                id: "display_name_spoof",
                points: 40,
                detail: format!(
                    "Display name claims \"{brand_name_lower}\" but the message was sent from {from_domain} (expected {expected})"
                ),
            });
        }
    }

    // Typosquatting: small edit distance between the sender domain and any
    // known brand domain (sans TLD). Skip when the sender is on a same-org
    // domain to avoid flagging legitimate regional variants.
    for (brand_domain, _) in brands {
        let brand_domain = brand_domain.to_lowercase();
        if on_domain(&brand_domain) || same_org(&from_domain, &brand_domain) {
            continue;
        }
        let from_head = sans_tld(&from_domain);
        let brand_head = sans_tld(&brand_domain);
        if from_head.len() >= 5 && brand_head.len() >= 5 {
            let dist = strsim::levenshtein(from_head, brand_head);
            if dist > 0 && dist <= 2 {
                checks.push(Check {
                    id: "domain_lookalike",
                    points: 50,
                    detail: format!("Sender domain {from_domain} looks like {brand_domain}"),
                });
            }
        }
    }

    // The same display name can claim several brand variants; count once each.
    dedup_by_id(&mut checks, "display_name_spoof");
    dedup_by_id(&mut checks, "domain_lookalike");

    // ── IDN / Punycode homograph ─────────────────────────────────────────────
    if from_domain.contains("xn--") {
        checks.push(Check {
            id: "idn_homograph",
            points: 35,
            detail: format!("Sender domain {from_domain} uses internationalized (Punycode) characters"),
        });
    }

    // ── Link checks (HTML body only) ─────────────────────────────────────────
    let html = extract_html(&parsed);
    if let Some(html) = html {
        // Link text / href domain mismatch.
        let mut link_points = 0;
        for (href_domain, text_domain) in mismatched_links(&html) {
            if link_points >= 60 {
                break;
            }
            link_points += 20;
            checks.push(Check {
                id: "link_mismatch",
                points: 20,
                detail: format!("Link text shows {text_domain} but points to {href_domain}"),
            });
        }

        // OpenPhish feed: exact URL hit is near-certain phishing; a domain hit
        // is strong evidence.
        if !feed.urls.is_empty() {
            for href in all_hrefs(&html) {
                let normalized = href.trim_end_matches('/').to_lowercase();
                if feed.urls.contains(&normalized) {
                    checks.push(Check {
                        id: "openphish_url",
                        points: 70,
                        detail: format!("Link {href} is listed in the OpenPhish phishing feed"),
                    });
                } else if let Some(domain) = url_domain(&href) {
                    if feed.domains.contains(&domain) {
                        checks.push(Check {
                            id: "openphish_domain",
                            points: 50,
                            detail: format!("Link domain {domain} is listed in the OpenPhish phishing feed"),
                        });
                    }
                }
            }
            dedup_by_id(&mut checks, "openphish_url");
            dedup_by_id(&mut checks, "openphish_domain");
        }
    }

    let score: i32 = checks.iter().map(|c| c.points).sum();
    Report { score, verdict: verdict_for(score), checks }
}

fn dedup_by_id(checks: &mut Vec<Check>, id: &str) {
    let mut seen = false;
    checks.retain(|c| {
        if c.id == id {
            if seen {
                return false;
            }
            seen = true;
        }
        true
    });
}

fn extract_html(parsed: &mailparse::ParsedMail) -> Option<String> {
    if parsed.ctype.mimetype.eq_ignore_ascii_case("text/html") {
        return parsed.get_body().ok();
    }
    for sub in &parsed.subparts {
        if let Some(html) = extract_html(sub) {
            return Some(html);
        }
    }
    None
}

/// Find anchors whose display text is itself a URL/domain that differs from
/// the href domain — the classic "shows paypal.com, goes to evil.com" trick.
fn mismatched_links(html: &str) -> Vec<(String, String)> {
    static ANCHOR: OnceLock<Regex> = OnceLock::new();
    static TAGS: OnceLock<Regex> = OnceLock::new();
    static TEXT_URL: OnceLock<Regex> = OnceLock::new();
    let anchor = ANCHOR.get_or_init(|| {
        Regex::new(r#"(?is)<a\b[^>]*?href\s*=\s*["']?(https?://[^"'\s>]+)["']?[^>]*>(.*?)</a>"#).unwrap()
    });
    let tags = TAGS.get_or_init(|| Regex::new(r"(?s)<[^>]*>").unwrap());
    // The visible text only counts as a navigable domain when it is presented
    // like a link — an explicit http(s):// scheme or a www. prefix. A bare
    // dotted token (Instagram handles like `hebamme.aachen`, filenames, …) is
    // too ambiguous to treat as a spoofed destination.
    let text_url = TEXT_URL.get_or_init(|| {
        Regex::new(r"(?i)(?:https?://|www\.)((?:[a-z0-9-]+\.)+[a-z]{2,})").unwrap()
    });

    let mut out = Vec::new();
    for cap in anchor.captures_iter(html) {
        let href = &cap[1];
        let text = tags.replace_all(&cap[2], "");
        let text = text.trim();

        let Some(href_domain) = url_domain(href) else { continue };
        // Only flag when the visible text itself names a domain.
        let Some(m) = text_url.captures(text) else { continue };
        let text_domain = m[1].to_lowercase();

        // Same organisation (registrable domain) is not a mismatch —
        // www.animexx.de linking to ssl.animexx.de is fine.
        if !same_org(&href_domain, &text_domain) {
            out.push((href_domain, text_domain));
        }
    }
    out
}

/// All anchor hrefs in the HTML, for feed lookups.
fn all_hrefs(html: &str) -> Vec<String> {
    static HREF: OnceLock<Regex> = OnceLock::new();
    let href = HREF.get_or_init(|| {
        Regex::new(r#"(?i)<a\b[^>]*?href\s*=\s*["']?(https?://[^"'\s>]+)"#).unwrap()
    });
    href.captures_iter(html).map(|c| c[1].to_string()).collect()
}

fn url_domain(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.split('@').next_back()?.split(':').next()?;
    Some(host.to_lowercase())
}

/// Run analysis and persist: upsert into `phishing_analysis` and denormalize
/// the verdict onto `messages.phishing_verdict`.
pub async fn analyse_and_store(db: &SqlitePool, message_id: &str, raw: &[u8]) {
    // Custom DB entries first: they take precedence over the file list.
    let mut brands: Vec<(String, String)> =
        sqlx::query_as("SELECT domain, brand_name FROM user_brand_entries")
            .fetch_all(db)
            .await
            .unwrap_or_default();
    brands.extend(current_brands());

    let report = {
        let guard = FEED.get().map(|lock| lock.read().unwrap());
        let empty = OpenPhishFeed::default();
        let feed: &OpenPhishFeed = guard.as_deref().unwrap_or(&empty);
        analyse(raw, &brands, feed)
    };
    let checks_json = serde_json::to_string(&report.checks).unwrap_or_else(|_| "[]".into());

    if let Err(e) = sqlx::query(
        "INSERT INTO phishing_analysis (message_id, score, verdict, checks_json, analysed_at) VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) ON CONFLICT(message_id) DO UPDATE SET score = excluded.score, verdict = excluded.verdict, checks_json = excluded.checks_json, analysed_at = excluded.analysed_at",
    )
    .bind(message_id)
    .bind(report.score)
    .bind(report.verdict)
    .bind(&checks_json)
    .execute(db)
    .await
    {
        warn!("phishing analysis store failed for {message_id}: {e}");
        return;
    }

    let _ = sqlx::query("UPDATE messages SET phishing_verdict = ? WHERE id = ?")
        .bind(report.verdict)
        .bind(message_id)
        .execute(db)
        .await;
}
