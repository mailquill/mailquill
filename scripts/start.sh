#!/usr/bin/env bash
# Single-command startup for Mailquill.
#
# Builds the frontend (Vite) and then runs the backend in release mode. In
# release builds rust-embed bundles frontend/dist into the `mailquill` binary, so
# the server self-serves the UI on its own — no separate web server or `pnpm`
# process is needed at runtime.
#
# The backend reads its configuration (CREDENTIAL_ENCRYPTION_KEY, JWT_SECRET,
# DATA_DIR, …) from .env via cargo's configured runner (scripts/dev-runner.sh).
#
# Usage:
#   scripts/start.sh                  # build UI, then build + run release server
#   SKIP_FRONTEND=1 scripts/start.sh  # reuse the existing frontend/dist
#   scripts/start.sh -- --some-flag   # forward extra args to the binary
set -euo pipefail

repo_root=$(cd "$(dirname "$0")/.." && pwd)

if [ "${SKIP_FRONTEND:-0}" != "1" ]; then
    echo "==> Building frontend (release)…"
    cd "$repo_root/frontend"
    if [ ! -d node_modules ]; then
        pnpm install --frozen-lockfile
    fi
    pnpm run build
    cd "$repo_root"
else
    echo "==> Skipping frontend build (SKIP_FRONTEND=1)"
fi

if [ ! -f "$repo_root/frontend/dist/index.html" ]; then
    echo "error: frontend/dist/index.html missing — cannot embed the UI." >&2
    echo "       run without SKIP_FRONTEND=1 to build the frontend first." >&2
    exit 1
fi

echo "==> Building & starting backend (release)…"
cd "$repo_root"
# cargo's configured runner (scripts/dev-runner.sh) loads .env before exec.
exec cargo run --release -p api -- "$@"
