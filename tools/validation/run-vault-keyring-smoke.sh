#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/../.."
: "${FLOE_CODESIGN_IDENTITY:?Set an explicit valid code-signing identity}"
case "${1:-}" in
  --probe|--exercise) [[ "$#" == 1 ]] || exit 2 ;;
  --cleanup) [[ "$#" == 3 ]] || exit 2 ;;
  *) printf '%s\n' 'Use --probe, --exercise, or --cleanup <retained temporary root> <Person UUID>.' >&2; exit 2 ;;
esac

CARGO_INCREMENTAL=0 cargo build -p floe-core --example vault_keyring_smoke
bundle="$PWD/target/validation/FloeVaultSmoke.app"
mkdir -p "$bundle/Contents/MacOS"
cp target/debug/examples/vault_keyring_smoke "$bundle/Contents/MacOS/FloeVaultSmoke"
cp tools/validation/vault-smoke-Info.plist "$bundle/Contents/Info.plist"
arguments=(--force --sign "$FLOE_CODESIGN_IDENTITY")
if [[ -n "${FLOE_VAULT_SMOKE_PROFILE:-}" || -n "${FLOE_VAULT_SMOKE_ENTITLEMENTS:-}" ]]; then
  : "${FLOE_VAULT_SMOKE_PROFILE:?A provisioning profile and matching entitlements must be supplied together}"
  : "${FLOE_VAULT_SMOKE_ENTITLEMENTS:?A provisioning profile and matching entitlements must be supplied together}"
  cp "$FLOE_VAULT_SMOKE_PROFILE" "$bundle/Contents/embedded.provisionprofile"
  arguments+=(--entitlements "$FLOE_VAULT_SMOKE_ENTITLEMENTS")
elif [[ -e "$bundle/Contents/embedded.provisionprofile" ]]; then
  printf '%s\n' 'Existing bundle has a profile. Use a matching profile/entitlements explicitly.' >&2
  exit 2
fi
codesign "${arguments[@]}" "$bundle"
codesign --verify --deep --strict "$bundle"
"$bundle/Contents/MacOS/FloeVaultSmoke" "$@"
