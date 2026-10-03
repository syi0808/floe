#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."
case "${1:-}" in
  --exercise-learner|--exercise-learner-expiry)
    [[ "$#" == 1 || ( "$#" == 3 && "$2" == --profile && -n "$3" ) ]] || exit 2 ;;
  --availability|--exercise|--exercise-optional-memory|--exercise-manager-guidance|--exercise-manager-guidance-server)
    [[ "$#" == 1 ]] || exit 2 ;;
  *) printf '%s\n' 'Use --availability, --exercise, --exercise-optional-memory, --exercise-learner [--profile PATH], --exercise-learner-expiry [--profile PATH], --exercise-manager-guidance, or --exercise-manager-guidance-server (synthetic only).' >&2; exit 2 ;;
esac
case "$1" in
  --exercise-manager-guidance|--exercise-manager-guidance-server)
    if [[ "${FLOE_MANAGER_EVAL_APPROVED:-}" != 1 ]]; then
      printf '%s\n' '{"status":"UNVERIFIED","reason":"explicit_live_opt_in_required","personal_data":false}'
      exit 1
    fi ;;
esac
CARGO_INCREMENTAL=0 cargo build -p floe-app --example local_model_smoke
bundle="$PWD/target/validation/FloeLocalModelSmoke.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Frameworks"
cp target/debug/examples/local_model_smoke "$bundle/Contents/MacOS/FloeLocalModelSmoke"
cp tools/validation/local-model-smoke-Info.plist "$bundle/Contents/Info.plist"
library="$bundle/Contents/Frameworks/libfloe_local_model.dylib"
xcrun swiftc -emit-library -swift-version 6 -warnings-as-errors \
  -target "$(uname -m)-apple-macosx12.0" \
  apps/client/apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthPrivacyTransform.swift \
  apps/client/macos/LocalModel/LocalModel.swift -o "$library"
install_name_tool -id '@rpath/libfloe_local_model.dylib' "$library"
codesign --force --sign - "$library"
codesign --force --sign - "$bundle"
codesign --verify --deep --strict "$bundle"
"$bundle/Contents/MacOS/FloeLocalModelSmoke" "$@"
