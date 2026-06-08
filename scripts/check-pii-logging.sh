#!/usr/bin/env bash
# Reject tracing macros that directly interpolate known PII field names
# without wrapping in Pii(). Task 3.6.
set -euo pipefail

PII_FIELDS="from_addr|to_addrs|primary_email|email|subject"
MACRO_PATTERN="tracing::(info|warn|error|debug)\!"

BAD_LINES=$(grep -rn --include="*.rs" -P "${MACRO_PATTERN}" backend/core backend/api \
  | grep -P "(${PII_FIELDS})" \
  | grep -v "Pii(" \
  | grep -v "//.*tracing" || true)

if [ -n "$BAD_LINES" ]; then
  echo "ERROR: Direct PII interpolation in tracing macros (wrap with Pii()):"
  echo "$BAD_LINES"
  exit 1
fi

echo "OK: no raw PII interpolation found in tracing macros"
