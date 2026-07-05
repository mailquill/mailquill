# phishing-detection Specification

## Purpose
TBD - created by archiving change phishing. Update Purpose after archive.
## Requirements
### Requirement: Phishing analysis at sync time
The system SHALL run phishing analysis on every incoming message immediately after it is stored during IMAP sync. Results SHALL be stored in `phishing_analysis` (message_id, score, verdict, checks_json) and available before the message is first opened.

#### Scenario: Analysis runs on new message
- **WHEN** a new message is stored during sync
- **THEN** phishing analysis runs synchronously, results are written to `phishing_analysis`, and the message's `phishing_verdict` field is set (clean / suspicious / phishing)

---

### Requirement: Authentication header parsing
The system SHALL parse the `Authentication-Results` header to extract SPF, DKIM, and DMARC results reported by the upstream mail server. A missing `Authentication-Results` header SHALL be treated as unknown (not fail).

#### Scenario: DMARC fail detected
- **WHEN** `Authentication-Results` contains `dmarc=fail`
- **THEN** the DMARC-fail check is recorded with +40 points and included in checks_json

#### Scenario: All auth checks pass
- **WHEN** `Authentication-Results` contains `spf=pass dkim=pass dmarc=pass`
- **THEN** no points added for these checks; verdict leans clean

---

### Requirement: Display name spoofing detection
The system SHALL compare the `From` display name against the `From` domain. If the display name contains a known brand name (from the bundled brands list) but the sending domain does not match that brand's canonical domain, a spoofing flag SHALL be raised.

#### Scenario: Display name brand mismatch
- **WHEN** `From: "PayPal Security" <notification@evil-domain.com>`
- **THEN** display-name-spoof check fires (+40 points), recorded with the mismatched brand and actual domain

#### Scenario: Display name matches domain
- **WHEN** `From: "PayPal" <service@paypal.com>`
- **THEN** no points added for this check

---

### Requirement: Reply-To mismatch detection
The system SHALL flag messages where the `Reply-To` domain differs from the `From` domain.

#### Scenario: Reply-To hijack
- **WHEN** `From: ceo@company.com` and `Reply-To: ceo@gmail.com`
- **THEN** reply-to-mismatch check fires (+20 points)

---

### Requirement: Return-Path mismatch detection
The system SHALL flag messages where the `Return-Path` (envelope sender) domain differs from the `From` header domain.

#### Scenario: Return-Path mismatch
- **WHEN** `From: support@paypal.com` and `Return-Path: bounce@bulk-mailer.com`
- **THEN** return-path-mismatch check fires (+20 points)

---

### Requirement: Domain lookalike detection
The system SHALL compare the `From` domain against all entries in the known brands list using Levenshtein edit distance. A distance of ≤ 2 between the sender domain (excluding TLD) and a known brand domain (excluding TLD) SHALL flag a lookalike.

#### Scenario: Typosquatting domain
- **WHEN** `From: support@paypa1.com` (edit distance 1 from paypal.com)
- **THEN** domain-lookalike check fires (+50 points), recorded with the matched brand and actual domain

#### Scenario: Legitimate domain not near any brand
- **WHEN** `From: alice@example.com`
- **THEN** no lookalike check fires

---

### Requirement: IDN / Punycode homograph detection
The system SHALL decode IDN domains in the `From` address using IDNA normalization. If a domain uses non-ASCII characters that visually resemble ASCII (homograph attack), it SHALL be flagged.

#### Scenario: Punycode homograph
- **WHEN** `From: service@xn--pypal-4ve.com` (paypal with Cyrillic 'а')
- **THEN** IDN-homograph check fires (+35 points); decoded domain shown in UI

---

### Requirement: Link text / href mismatch detection
The system SHALL parse all `<a>` tags in the HTML body. When the link display text is a URL (contains a domain) and its domain differs from the `href` domain, the check SHALL fire.

#### Scenario: Misleading link
- **WHEN** HTML contains `<a href="https://evil.com/steal">https://paypal.com</a>`
- **THEN** link-mismatch check fires (+20 points per link, capped at +60 total)

#### Scenario: Normal descriptive link text
- **WHEN** `<a href="https://paypal.com">Click here to log in</a>` (text is not a URL)
- **THEN** no link-mismatch check fires

---

### Requirement: Scoring and verdict
The system SHALL sum points from all triggered checks and assign a verdict:
- `clean` (0–20 points): no indicator shown
- `suspicious` (21–55 points): yellow warning banner
- `phishing` (56+ points): red warning banner

#### Scenario: Multiple checks triggered
- **WHEN** DMARC fail (+40) and display name spoof (+40) both fire
- **THEN** score = 80, verdict = phishing

---

### Requirement: Phishing banner in message detail
The system SHALL display a collapsible banner at the top of the message detail view whenever verdict is suspicious or phishing. The banner SHALL show the verdict, a plain-language summary, and an expandable list of triggered checks with per-check explanations. The raw `From` domain SHALL always be shown next to the display name, regardless of verdict.

#### Scenario: Red phishing banner
- **WHEN** verdict is phishing
- **THEN** a red banner displays "This message is likely phishing" with expandable check details; links in the body are visually highlighted as potentially unsafe

#### Scenario: Yellow suspicious banner
- **WHEN** verdict is suspicious
- **THEN** a yellow banner displays "This message has suspicious characteristics" with expandable details

#### Scenario: Clean message — no banner
- **WHEN** verdict is clean
- **THEN** no banner is shown; shield icon in message list is grey (neutral)

---

### Requirement: Shield indicator in message list
The system SHALL show a shield icon per message in the message list indicating verdict: grey (clean/unknown), yellow (suspicious), red (phishing).

#### Scenario: Shield colour matches verdict
- **WHEN** a message has verdict phishing
- **THEN** the message list row shows a red shield icon

---

### Requirement: Known brands list
The system SHALL bundle a `brands.json` file listing ~200 canonical brand domains used for display-name spoofing and lookalike checks. Users SHALL be able to add custom entries via settings (e.g. their own company domain). Custom entries SHALL take precedence over the bundled list.

#### Scenario: Custom brand entry
- **WHEN** a user adds `acme-corp.com` to their custom brands list
- **THEN** emails claiming to be from ACME Corp but sent from other domains are flagged as display-name spoofing

---

### Requirement: Re-analyse on demand
The system SHALL allow re-running phishing analysis on a specific message via `POST /messages/:id/reanalyse`. Used after a brands list update or for manual review.

#### Scenario: Re-analyse after brands update
- **WHEN** a user adds a new custom brand and triggers re-analysis on a message
- **THEN** the analysis runs again with the updated brands list and the verdict may change

