#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEVICE="${1:-macos}"
LOG_DIRECTORY="$ROOT/.floe-debug"
LOG_FILE="$LOG_DIRECTORY/agent-$(date -u +%Y%m%dT%H%M%SZ).log"

mkdir -p "$LOG_DIRECTORY"
cd "$ROOT"
cargo build -p floe-ffi

printf 'Writing combined Flutter and Rust logs to %s\n' "$LOG_FILE"
cd "$ROOT/apps/client"
FLOE_LOG="${FLOE_LOG:-warn,floe_ffi=info}" \
  flutter run -d "$DEVICE" 2>&1 | tee "$LOG_FILE"
