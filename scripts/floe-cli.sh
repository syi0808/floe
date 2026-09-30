#!/bin/zsh
set -euo pipefail

ROOT="${0:A:h:h}"
cd "${ROOT}"
if [[ "$(uname -s)" != Darwin ]]; then
  print -u2 -- 'The Floe debug CLI currently supports macOS only.'
  exit 1
fi

cargo build -p floe-app --example floe_cli >&2
BUNDLE="${ROOT}/target/cli/FloeDebugCLI.app"
mkdir -p "${BUNDLE}/Contents/MacOS" "${BUNDLE}/Contents/Frameworks"
if ! cmp -s target/debug/examples/floe_cli "${BUNDLE}/Contents/MacOS/floe_cli"; then
  cp target/debug/examples/floe_cli "${BUNDLE}/Contents/MacOS/floe_cli"
fi
cp tools/cli/Info.plist "${BUNDLE}/Contents/Info.plist"

export SRCROOT="${ROOT}/apps/client/macos"
export DERIVED_FILE_DIR="${ROOT}/target/cli/native-cache"
export TARGET_BUILD_DIR="${BUNDLE}/Contents"
export FRAMEWORKS_FOLDER_PATH=Frameworks
zsh "${SRCROOT}/build_native.sh" >&2
codesign --force --sign - "${BUNDLE}" >&2
codesign --verify --deep --strict "${BUNDLE}" >&2
exec "${BUNDLE}/Contents/MacOS/floe_cli" "$@"
