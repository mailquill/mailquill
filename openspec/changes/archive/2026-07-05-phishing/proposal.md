## Why

Phishing is the most common vector for credential theft and malware delivery via email. By running rule-based analysis at sync time — before the user opens a message — Mailquill can surface warnings without adding any latency to the read experience. The analysis is entirely local and self-contained: no message content leaves the server, no third-party reputation API is called.

## What Changes

- Backend rule engine that scores every new message at IMAP sync time across 10 checks: DMARC/SPF/DKIM auth header results, Return-Path/Reply-To domain mismatches, display-name spoofing against a bundled brand list, domain lookalike detection (Levenshtein ≤ 2), IDN/Punycode homograph detection, and HTML link text/href mismatch
- Bundled `brands.json` (~200 canonical brand domains); user-extensible via settings
- `phishing_analysis` table: one row per message with score, verdict, and per-check explanations
- `phishing_verdict` denormalized onto `messages` table for list query performance
- Reanalysis API for applying updated brand lists to existing messages
- Frontend: shield icon on message list rows (grey/yellow/red), phishing banner in message detail with collapsible check list, sender domain always shown next to display name, custom brands settings page

## Capabilities

### New Capabilities

- `phishing-detection`: Rule-based scoring at sync time — auth header parsing, sender/content validation, domain lookalikes, link mismatch, IDN/Punycode; verdict shown in message list and detail

### Modified Capabilities

## Impact

- Depends on `email-core` (IMAP sync pipeline, message list, message detail view)
- Rust additions: `strsim` crate (Levenshtein distance), `idna` crate (Punycode normalization), `scraper` crate (HTML link extraction), `mailparse` crate (Authentication-Results header parsing)
- Bundled asset: `core/src/assets/brands.json` (~200 brand entries)
- New DB tables: `phishing_analysis`, `user_brand_entries` (already created as stubs in `foundation`)
- Modified DB: `phishing_verdict` column on `messages` (denormalized)
- New API routes: `POST /messages/:id/reanalyse`, `GET /settings/brands`, `POST /settings/brands`, `DELETE /settings/brands/:id`
- No external network calls — analysis is fully local
