## Context

Builds on `email-core`. The IMAP sync pipeline stores messages; this change hooks into that pipeline to run analysis after each new message is written. All analysis is sync-time — zero latency on message open. Results are written to `phishing_analysis` and denormalized to `messages.phishing_verdict` for cheap list queries.

## Decisions

### D1: Rule-based scorer — 10 checks, additive points

| Check | Points |
|---|---|
| `Authentication-Results: dmarc=fail` | +40 |
| `Authentication-Results: spf=fail` | +30 |
| `Authentication-Results: dkim=fail` | +25 |
| `Return-Path` domain ≠ `From` domain | +20 |
| `Reply-To` domain ≠ `From` domain | +20 |
| Display name contains known brand + `From` domain doesn't match brand's canonical domain | +40 |
| From domain lookalike of known brand domain (Levenshtein ≤ 2, sans TLD) | +50 |
| From domain is IDN/Punycode (decoded form differs from raw — non-ASCII present) | +35 |
| Link href domain ≠ link display text domain (when text looks like URL) | +20 per link, max +60 |
| Sender domain not in user's contacts or previous sent mail | +5 |

**Thresholds:**
- 0–20: Clean (no indicator shown)
- 21–55: Suspicious (yellow shield)
- 56+: Likely phishing (red shield, banner)

### D2: Brands list — bundled JSON, user-extensible

`core/src/assets/brands.json`: ~200 entries of canonical brand domains (PayPal, Amazon, Apple, Microsoft, Google, major banks, etc.). Bundled at compile time (no network fetch). Used for display-name spoofing and domain lookalike checks.

User additions stored in `user_brand_entries` table. Both lists merged at analysis time. Custom entries can trigger reanalysis of recent messages via `POST /messages/:id/reanalyse`.

### D3: Libraries

- `strsim`: Levenshtein distance for domain lookalike check
- `idna`: Punycode/IDN decode for homograph check
- `scraper`: HTML link extraction from message body for href/text mismatch check
- `mailparse`: `Authentication-Results` header parsing (SPF/DKIM/DMARC token extraction)

All are pure Rust libraries, no external network calls.

### D4: Storage — separate table + denormalized verdict

`phishing_analysis`: full analysis result (score, verdict, `checks_json` array of triggered checks with per-check explanations for UI).

`messages.phishing_verdict`: denormalized TEXT column (`clean`/`suspicious`/`phishing`/NULL) for fast list rendering without joining `phishing_analysis` on every row. Updated atomically with `phishing_analysis` insert.

### D5: Sender domain display — unconditional

Raw `From` domain shown in parentheses next to display name in **both** message list and message detail header, regardless of verdict. This is a baseline transparency feature — not conditional on a phishing score.
