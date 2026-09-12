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

MODEL_SOURCE="${REPOSITORY_ROOT}/apps/client/macos/LocalModel/LocalModel.swift"
MODEL_LIBRARIES=()
IOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-16.0}"
IOS_SDK="$(xcrun --sdk "${PLATFORM_NAME}" --show-sdk-path)"
for target in ${(u)TARGETS}; do
  case "${target}" in
    aarch64-apple-ios)
      SWIFT_TARGET="arm64-apple-ios${IOS_DEPLOYMENT_TARGET}"
      ;;
    aarch64-apple-ios-sim)
      SWIFT_TARGET="arm64-apple-ios${IOS_DEPLOYMENT_TARGET}-simulator"
      ;;
    x86_64-apple-ios)
      SWIFT_TARGET="x86_64-apple-ios${IOS_DEPLOYMENT_TARGET}-simulator"
      ;;
    *)
      print -u2 "Unsupported Swift iOS architecture: ${target}"
      exit 1
      ;;
  esac
  MODEL_LIBRARY="${TARGET_BUILD_DIR}/libfloe_local_model_${target}.dylib"
  xcrun swiftc -emit-library -swift-version 6 -warnings-as-errors \
    -sdk "${IOS_SDK}" -target "${SWIFT_TARGET}" \
    -Xlinker -weak_framework -Xlinker FoundationModels \
    "${MODEL_SOURCE}" -o "${MODEL_LIBRARY}"
  MODEL_LIBRARIES+=("${MODEL_LIBRARY}")
done

MODEL_LIBRARY="${FRAMEWORKS_DIRECTORY}/libfloe_local_model.dylib"
if (( ${#MODEL_LIBRARIES[@]} == 1 )); then
  cp "${MODEL_LIBRARIES[1]}" "${MODEL_LIBRARY}"
else
  xcrun lipo -create "${MODEL_LIBRARIES[@]}" -output "${MODEL_LIBRARY}"
fi
install_name_tool -id "@rpath/libfloe_local_model.dylib" "${MODEL_LIBRARY}"
codesign --force --sign "${SIGNING_IDENTITY}" "${MODEL_LIBRARY}"
