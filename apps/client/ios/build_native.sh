#!/bin/zsh
set -euo pipefail

source "${SRCROOT}/../apple/native_build.sh"
FRAMEWORKS_DIRECTORY="${TARGET_BUILD_DIR}/${FRAMEWORKS_FOLDER_PATH}"
MODEL_SOURCE="${SRCROOT}/../macos/LocalModel/LocalModel.swift"
MODEL_LIBRARIES=()
TARGETS=()
for architecture in ${(s: :)ARCHS}; do
  case "${architecture}:${PLATFORM_NAME}" in
    arm64:iphoneos) TARGETS+=(aarch64-apple-ios) ;;
    arm64:iphonesimulator) TARGETS+=(aarch64-apple-ios-sim) ;;
    x86_64:iphonesimulator) TARGETS+=(x86_64-apple-ios) ;;
    *) print -u2 "Unsupported iOS architecture/platform: ${architecture}:${PLATFORM_NAME}"; exit 1 ;;
  esac
done
if (( ${#TARGETS[@]} == 0 )); then
  print -u2 "Xcode did not provide an iOS architecture"
  exit 1
fi
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
  MODEL_LIBRARY="${NATIVE_CACHE_DIRECTORY}/libfloe_local_model_${target}.dylib"
  floe_native_artifact "${MODEL_LIBRARY}" swift -emit-library -swift-version 6 -warnings-as-errors \
    -sdk "${IOS_SDK}" -target "${SWIFT_TARGET}" \
    -Xlinker -weak_framework -Xlinker FoundationModels \
    "${MODEL_SOURCE}"
  MODEL_LIBRARIES+=("${MODEL_LIBRARY}")
done

MODEL_LIBRARY="${FRAMEWORKS_DIRECTORY}/libfloe_local_model.dylib"
floe_native_artifact "${MODEL_LIBRARY}" embed "${MODEL_LIBRARIES[@]}"
