#!/bin/zsh
set -euo pipefail

REPOSITORY_ROOT="${SRCROOT}/../../.."
PROFILE="debug"
CARGO_FLAGS=()
if [[ "${CONFIGURATION:-Debug}" != "Debug" ]]; then
  PROFILE="release"
  CARGO_FLAGS+=(--release)
fi

case "${PLATFORM_NAME:-}" in
  iphoneos)
    TARGETS=(aarch64-apple-ios)
    ;;
  iphonesimulator)
    TARGETS=()
    for architecture in ${(s: :)ARCHS}; do
      case "${architecture}" in
        arm64) TARGETS+=(aarch64-apple-ios-sim) ;;
        x86_64) TARGETS+=(x86_64-apple-ios) ;;
        *) print -u2 "Unsupported iOS simulator architecture: ${architecture}"; exit 1 ;;
      esac
    done
    ;;
  *)
    print -u2 "Unsupported iOS platform: ${PLATFORM_NAME:-unset}"
    exit 1
    ;;
esac

if (( ${#TARGETS[@]} == 0 )); then
  print -u2 "Xcode did not provide an iOS architecture"
  exit 1
fi

export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
for target in ${(u)TARGETS}; do
  "${HOME}/.cargo/bin/cargo" build \
    --manifest-path "${REPOSITORY_ROOT}/Cargo.toml" \
    --package floe-ffi \
    --target "${target}" \
    "${CARGO_FLAGS[@]}"
done

FRAMEWORKS_DIRECTORY="${TARGET_BUILD_DIR}/${FRAMEWORKS_FOLDER_PATH}"
DESTINATION_LIBRARY="${FRAMEWORKS_DIRECTORY}/libfloe_ffi.dylib"
mkdir -p "${FRAMEWORKS_DIRECTORY}"
LIBRARIES=()
for target in ${(u)TARGETS}; do
  LIBRARIES+=("${REPOSITORY_ROOT}/target/${target}/${PROFILE}/libfloe_ffi.dylib")
done
if (( ${#LIBRARIES[@]} == 1 )); then
  cp "${LIBRARIES[1]}" "${DESTINATION_LIBRARY}"
else
  xcrun lipo -create "${LIBRARIES[@]}" -output "${DESTINATION_LIBRARY}"
fi
install_name_tool -id "@rpath/libfloe_ffi.dylib" "${DESTINATION_LIBRARY}"

SIGNING_IDENTITY="${EXPANDED_CODE_SIGN_IDENTITY:--}"
codesign --force --sign "${SIGNING_IDENTITY}" "${DESTINATION_LIBRARY}"
