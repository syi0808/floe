#!/bin/bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/reset-local-data.sh [--yes]

Reset local Floe databases, server state, and Floe-owned Keychain entries.
Application data is moved to the Trash so it can be recovered. Keychain entries
are deleted permanently. macOS privacy permissions are not changed.

Options:
  --yes   Skip the interactive confirmation.
  --help  Show this help.
EOF
}

assume_yes=false
case "${1:-}" in
  "") ;;
  --yes) assume_yes=true ;;
  --help|-h) usage; exit 0 ;;
  *) usage >&2; exit 2 ;;
esac
[[ "$#" -le 1 ]] || { usage >&2; exit 2; }

if [[ "$(uname -s)" != Darwin ]]; then
  printf '%s\n' 'This script supports macOS only.' >&2
  exit 1
fi

for process_name in floe_client floe-server; do
  if pgrep -x "$process_name" >/dev/null; then
    printf 'Stop %s before resetting local data.\n' "$process_name" >&2
    exit 1
  fi
done

cat <<'EOF'
This will reset:
  - Floe client databases, encrypted conversation vaults, and local device ID
  - Floe server state, connector indexes, and admin token
  - Floe vault keys, OAuth credentials, and local-server pairing in Keychain

Existing application data will be moved to the Trash. Keychain entries cannot
be recovered from the Trash. macOS privacy permissions will remain unchanged.
EOF

if [[ "$assume_yes" == false ]]; then
  printf '\nType RESET to continue: '
  read -r confirmation
  [[ "$confirmation" == RESET ]] || { printf '%s\n' 'Reset cancelled.'; exit 1; }
fi

keychain_services=(
  com.floe.agent-vault.v1
  app.floe.local-server
  app.floe.contacts-handles
  app.floe.server.credentials
)

deleted_keychain_items=0
for service in "${keychain_services[@]}"; do
  while security find-generic-password -s "$service" >/dev/null 2>&1; do
    security delete-generic-password -s "$service" >/dev/null
    deleted_keychain_items=$((deleted_keychain_items + 1))
  done
done

data_sources=(
  "$HOME/Library/Application Support/app.floe.floeClient"
  "$HOME/Library/Containers/app.floe.floeClient/Data/Library/Application Support/app.floe.floeClient"
  "$HOME/Library/Application Support/FloeServer"
)
data_labels=(client-unsandboxed client-sandboxed server)

trash_directory=''
moved_directories=0
for index in "${!data_sources[@]}"; do
  source_directory="${data_sources[$index]}"
  [[ -e "$source_directory" ]] || continue
  if [[ -z "$trash_directory" ]]; then
    trash_directory=$(mktemp -d "$HOME/.Trash/Floe-reset-XXXXXX")
  fi
  mv "$source_directory" "$trash_directory/${data_labels[$index]}"
  moved_directories=$((moved_directories + 1))
done

printf '\nReset complete: removed %d Keychain item(s) and moved %d data directories.\n' \
  "$deleted_keychain_items" "$moved_directories"
if [[ -n "$trash_directory" ]]; then
  printf 'Recoverable application data: %s\n' "$trash_directory"
fi
