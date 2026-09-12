#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVER_PID=""
CLIENT_PID=""

for command_name in go flutter; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'Required command not found: %s\n' "$command_name" >&2
    exit 127
  fi
done

cleanup() {
  trap - EXIT INT TERM

  for process_id in "$CLIENT_PID" "$SERVER_PID"; do
    if [[ -n "$process_id" ]] && kill -0 "$process_id" 2>/dev/null; then
      kill "$process_id" 2>/dev/null || true
    fi
  done

  for process_id in "$CLIENT_PID" "$SERVER_PID"; do
    if [[ -n "$process_id" ]]; then
      wait "$process_id" 2>/dev/null || true
    fi
  done
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

(
  cd "$ROOT/server"
  unset FLOE_INFERENCE_CONFIG FLOE_INFERENCE_TOKEN
  exec go run ./cmd/floe-server
) &
SERVER_PID=$!

flutter_arguments=("$@")
if [[ ${#flutter_arguments[@]} -eq 0 ]]; then
  flutter_arguments=(-d macos)
fi

(
  cd "$ROOT/apps/client"
  exec flutter run "${flutter_arguments[@]}"
) &
CLIENT_PID=$!

printf 'Floe server (PID %s) and Flutter client (PID %s) started.\n' \
  "$SERVER_PID" "$CLIENT_PID"

while true; do
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    if wait "$SERVER_PID"; then
      server_status=0
    else
      server_status=$?
    fi
    SERVER_PID=""
    printf 'Floe server stopped; shutting down the Flutter client.\n' >&2
    if [[ $server_status -eq 0 ]]; then
      exit 1
    fi
    exit "$server_status"
  fi

  if ! kill -0 "$CLIENT_PID" 2>/dev/null; then
    if wait "$CLIENT_PID"; then
      client_status=0
    else
      client_status=$?
    fi
    CLIENT_PID=""
    exit "$client_status"
  fi

  sleep 1
done
