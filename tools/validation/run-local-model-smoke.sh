#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."
[[ "$#" == 1 ]] || exit 2
case "$1" in
  --availability|--exercise) ;;
  *) printf '%s\n' 'Use --availability or --exercise (one synthetic request).' >&2; exit 2 ;;
esac
CARGO_INCREMENTAL=0 cargo build -p floe-ffi --example local_model_smoke
bundle="$PWD/target/validation/FloeLocalModelSmoke.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Frameworks"
cp target/debug/examples/local_model_smoke "$bundle/Contents/MacOS/FloeLocalModelSmoke"
cp tools/validation/local-model-smoke-Info.plist "$bundle/Contents/Info.plist"
library="$bundle/Contents/Frameworks/libfloe_local_model.dylib"
xcrun swiftc -emit-library -swift-version 6 -warnings-as-errors \
  -target "$(uname -m)-apple-macosx12.0" \
  apps/client/macos/LocalModel/LocalModel.swift -o "$library"
install_name_tool -id '@rpath/libfloe_local_model.dylib' "$library"
codesign --force --sign - "$library"
codesign --force --sign - "$bundle"
codesign --verify --deep --strict "$bundle"
"$bundle/Contents/MacOS/FloeLocalModelSmoke" "$1"
