## 1. Phishing Detection — Backend

- [ ] 1.1 Add deps to `core/`: `strsim` (Levenshtein), `idna` (Punycode normalization), `scraper` (HTML link extraction), `mailparse` (header parsing)
- [ ] 1.2 Bundle `brands.json` (~200 canonical brand domain entries) in `core/` crate assets
- [ ] 1.3 Implement `Authentication-Results` header parser: extract `spf=`, `dkim=`, `dmarc=` result tokens
- [ ] 1.4 Implement display-name spoofing check: extract display name, fuzzy-match against brands list, compare brand's canonical domain against `From` domain
- [ ] 1.5 Implement Reply-To mismatch check: compare `Reply-To` domain vs `From` domain
- [ ] 1.6 Implement Return-Path mismatch check: compare `Return-Path` domain vs `From` domain
- [ ] 1.7 Implement domain lookalike check: Levenshtein distance ≤ 2 between sender domain (sans TLD) and each brands-list domain (sans TLD)
- [ ] 1.8 Implement IDN/Punycode homograph check: decode domain via `idna::domain_to_unicode`, flag if decoded form differs from raw (non-ASCII chars present)
- [ ] 1.9 Implement link text/href mismatch check: parse HTML body with `scraper`, extract `<a>` tags, detect when link text is a URL with domain differing from `href` domain; cap contribution at +60
- [ ] 1.10 Implement scorer: sum check points, assign verdict (clean ≤ 20, suspicious 21–55, phishing ≥ 56), write to `phishing_analysis`, update `messages.phishing_verdict`
- [ ] 1.11 Wire phishing analysis into IMAP sync: call after every new message is stored
- [ ] 1.12 Implement `POST /messages/:id/reanalyse` — re-run analysis with current brands list (including user custom entries)
- [ ] 1.13 Implement `GET /settings/brands`, `POST /settings/brands`, `DELETE /settings/brands/:id` for user custom brand entries

## 2. Phishing Detection — Frontend

- [ ] 2.1 Show shield icon in message list row: grey (clean/null), yellow (suspicious), red (phishing); placed next to sender name
- [ ] 2.2 Always render raw `From` domain in parentheses next to display name in message list and message detail header (regardless of verdict)
- [ ] 2.3 Build phishing banner component: red (phishing) or yellow (suspicious) bar at top of message detail, above body; shows verdict headline, collapsible check list with per-check plain-language explanations
- [ ] 2.4 In phishing banner: highlight links in red when verdict is phishing, with tooltip showing actual `href` vs displayed text
- [ ] 2.5 Build custom brands settings page: list user brand entries (domain + brand name), add/delete entries, trigger re-analysis button for recent messages
- [ ] 2.6 Wire `POST /messages/:id/reanalyse` to a "Re-analyse" action in message detail menu
