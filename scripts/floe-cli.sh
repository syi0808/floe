#!/bin/zsh
set -euo pipefail

ROOT="${0:A:h:h}"
cd "${ROOT}"
if [[ "$(uname -s)" != Darwin ]]; then
  print -u2 -- 'The Floe debug CLI currently supports macOS only.'
  exit 1
fi

STORAGE_PROFILE="development"
BUILD_PROFILE="debug"
CARGO_FLAGS=(--no-default-features --features development-storage)
if [[ "${1:-}" == "--production" ]]; then
  STORAGE_PROFILE="production"
  BUILD_PROFILE="release"
  CARGO_FLAGS=(--release)
  shift
fi
cargo build -p floe-app --example floe_cli "${CARGO_FLAGS[@]}" >&2
BUNDLE="${ROOT}/target/cli/${STORAGE_PROFILE}/FloeDebugCLI.app"
mkdir -p "${BUNDLE}/Contents/MacOS" "${BUNDLE}/Contents/Frameworks"
if ! cmp -s target/${BUILD_PROFILE}/examples/floe_cli "${BUNDLE}/Contents/MacOS/floe_cli"; then
  cp target/${BUILD_PROFILE}/examples/floe_cli "${BUNDLE}/Contents/MacOS/floe_cli"
fi
cp tools/cli/Info.plist "${BUNDLE}/Contents/Info.plist"

export CONFIGURATION="Debug"
[[ "$STORAGE_PROFILE" == "production" ]] && export CONFIGURATION="Release"
export SRCROOT="${ROOT}/apps/client/macos"
export DERIVED_FILE_DIR="${ROOT}/target/cli/${STORAGE_PROFILE}/native-cache"
export TARGET_BUILD_DIR="${BUNDLE}/Contents"
export FRAMEWORKS_FOLDER_PATH=Frameworks
zsh "${SRCROOT}/build_native.sh" >&2
codesign --force --sign - "${BUNDLE}" >&2
codesign --verify --deep --strict "${BUNDLE}" >&2
exec "${BUNDLE}/Contents/MacOS/floe_cli" "$@"
