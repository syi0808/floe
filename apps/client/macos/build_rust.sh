#!/bin/zsh
set -euo pipefail

REPOSITORY_ROOT="${SRCROOT}/../../.."
PROFILE="debug"
CARGO_FLAGS=()
if [[ "${CONFIGURATION}" != "Debug" ]]; then
  PROFILE="release"
  CARGO_FLAGS+=(--release)
fi

export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
"${HOME}/.cargo/bin/cargo" build \
  --manifest-path "${REPOSITORY_ROOT}/Cargo.toml" \
  --package floe-ffi \
  "${CARGO_FLAGS[@]}"

SOURCE_LIBRARY="${REPOSITORY_ROOT}/target/${PROFILE}/libfloe_ffi.dylib"
FRAMEWORKS_DIRECTORY="${TARGET_BUILD_DIR}/${FRAMEWORKS_FOLDER_PATH}"
DESTINATION_LIBRARY="${FRAMEWORKS_DIRECTORY}/libfloe_ffi.dylib"
mkdir -p "${FRAMEWORKS_DIRECTORY}"
cp "${SOURCE_LIBRARY}" "${DESTINATION_LIBRARY}"
install_name_tool -id "@rpath/libfloe_ffi.dylib" "${DESTINATION_LIBRARY}"

SIGNING_IDENTITY="${EXPANDED_CODE_SIGN_IDENTITY:--}"
if [[ -z "${SIGNING_IDENTITY}" ]]; then
  SIGNING_IDENTITY="-"
fi
codesign --force --sign "${SIGNING_IDENTITY}" "${DESTINATION_LIBRARY}"
