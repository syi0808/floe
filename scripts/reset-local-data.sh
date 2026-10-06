#!/bin/bash
set -euo pipefail

usage() {
  cat <<'HELP'
Usage: scripts/reset-local-data.sh [--profile development|production] [--yes]

Default: move only the isolated development client/server data to the Trash.
Production requires --profile production and preserves development data/logs.
This script never deletes Keychain entries or changes macOS privacy permissions.
Recovered encrypted data still needs its original keys. No live app/server may
be using either profile. Custom FLOE_SERVER_DATA locations are not included.
HELP
}

profile=development
assume_yes=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile)
      [[ $# -ge 2 ]] || { usage >&2; exit 2; }
      profile=$2; shift 2 ;;
    --yes) assume_yes=true; shift ;;
    --help|-h) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
done
[[ "$profile" == development || "$profile" == production ]] || { usage >&2; exit 2; }
[[ "$(uname -s)" == Darwin ]] || { printf '%s\n' 'This script supports macOS only.' >&2; exit 1; }

for process_name in floe_client floe-server; do
  if pgrep -x "$process_name" >/dev/null; then
    printf 'Stop %s before resetting local data.\n' "$process_name" >&2
    exit 1
  else
    status=$?
    [[ $status -eq 1 ]] || { printf 'Cannot verify whether %s is stopped.\n' "$process_name" >&2; exit 1; }
  fi
done

client_roots=(
  "$HOME/Library/Application Support/app.floe.floeClient"
  "$HOME/Library/Containers/app.floe.floeClient/Data/Library/Application Support/app.floe.floeClient"
)
data_sources=()
data_labels=()
for index in "${!client_roots[@]}"; do
  root=${client_roots[$index]}
  if [[ "$profile" == development ]]; then
    for entry in development-storage FloeDevelopmentNative; do
      data_sources+=("$root/$entry")
      data_labels+=("client-$index-$entry")
    done
  else
    # Move only known production installation artifacts. Diagnostics and
    # development namespaces remain where they are, and keys remain recoverable.
    for entry in local_installation.json local_installation.lock local_installation.create-attempt local_installation.ready local_installation.reset local_device_id selected_profile.json selected_profile.json.tmp people floe.db floe.db-wal floe.db-shm floe.db.agent-vaults; do
      data_sources+=("$root/$entry")
      data_labels+=("client-$index-$entry")
    done
  fi
done
if [[ "$profile" == development ]]; then
  data_sources+=("$HOME/Library/Application Support/FloeServerDevelopment")
else
  data_sources+=("$HOME/Library/Application Support/FloeServer")
fi
data_labels+=(server)

# Reject aliases before moving anything; never broaden a reset through a link.
for source in "${data_sources[@]}"; do
  ancestor=$source
  while [[ "$ancestor" != / && "$ancestor" != . ]]; do
    [[ ! -L "$ancestor" ]] || { printf 'Refusing symbolic-link reset path: %s\n' "$ancestor" >&2; exit 1; }
    ancestor=$(dirname "$ancestor")
  done
done
printf 'Move %s Floe data to the Trash. Keychain, diagnostics and OS permissions are preserved.\n' "$profile"
if [[ "$assume_yes" == false ]]; then
  printf 'Type RESET %s to continue: ' "$profile"
  read -r confirmation
  [[ "$confirmation" == "RESET $profile" ]] || { printf '%s\n' 'Reset cancelled.'; exit 1; }
fi

trash_directory=''
moved=0
for index in "${!data_sources[@]}"; do
  source=${data_sources[$index]}
  [[ -e "$source" ]] || continue
  if [[ -z "$trash_directory" ]]; then
    trash_directory=$(mktemp -d "$HOME/.Trash/Floe-$profile-reset-XXXXXX")
  fi
  mv "$source" "$trash_directory/${data_labels[$index]}"
  moved=$((moved + 1))
done
printf 'Reset complete: moved %d known %s data entries. No Keychain entries were deleted.\n' "$moved" "$profile"
if [[ -n "$trash_directory" ]]; then printf 'Recoverable data: %s\n' "$trash_directory"; fi
