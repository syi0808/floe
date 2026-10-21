#!/bin/zsh
set -euo pipefail

source "${SRCROOT}/../apple/native_build.sh"
FRAMEWORKS_DIRECTORY="${TARGET_BUILD_DIR}/${FRAMEWORKS_FOLDER_PATH}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
SWIFT_TARGET="$(uname -m)-apple-macosx${MACOSX_DEPLOYMENT_TARGET}"
floe_native_artifact "${FRAMEWORKS_DIRECTORY}/libfloe_eventkit.dylib" swift \
  -emit-library -warnings-as-errors -target "${SWIFT_TARGET}" \
  "${SRCROOT}/CalendarActions/EventKitActions.swift" \
  "${SRCROOT}/Runner/CalendarObservationFingerprint.swift"
floe_native_model_package "${FRAMEWORKS_DIRECTORY}/libfloe_local_model.dylib" \
  "${SWIFT_TARGET}" "$(xcrun --sdk macosx --show-sdk-path)"
LIVEKIT_FRAMEWORK="$(floe_native_package_framework \
  "${SWIFT_TARGET}" "$(xcrun --sdk macosx --show-sdk-path)")"
floe_native_embed_framework "${FRAMEWORKS_DIRECTORY}/LiveKitWebRTC.framework" \
  "${LIVEKIT_FRAMEWORK}"
