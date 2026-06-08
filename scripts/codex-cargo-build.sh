#!/usr/bin/env bash
# Agent Rust build helper — run cargo check/build from repo root.
# Usage: scripts/codex-cargo-build.sh [extra cargo args...]
set -euo pipefail
cd "$(dirname "$0")/.."
exec cargo check "$@"
