# Phishing Detection — Auxiliary Data Files

The phishing detector keeps two files in the data directory, initialized at
startup by `phishing::init`
([backend/phishing/src/lib.rs](../backend/phishing/src/lib.rs#L116)). For where
these sit relative to everything else, see
[filesystem-layout.md](filesystem-layout.md).

- **`<data_dir>/brands.json`** — the brand list used for lookalike-domain
  checks. If absent, the shipped default is copied in; an operator can replace
  this file to override the brand set.
- **`<data_dir>/openphish.txt`** — cached copy of the OpenPhish community feed.
  Refreshed every 12 hours when `openphish_enabled` is set (outbound GET only;
  no user data leaves the server). Re-downloaded on the next refresh if deleted.

Both files are regenerable and safe to delete: `brands.json` is re-seeded from
the shipped default, and `openphish.txt` is re-downloaded.
