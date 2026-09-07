#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/../.."
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
rm -f "$bundle/Contents/embedded.provisionprofile"
arguments=(--force --sign "${FLOE_CODESIGN_IDENTITY:--}")
codesign "${arguments[@]}" "$bundle"
codesign --verify --deep --strict "$bundle"
"$bundle/Contents/MacOS/FloeVaultSmoke" "$@"
