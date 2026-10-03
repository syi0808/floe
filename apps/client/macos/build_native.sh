#!/bin/zsh
set -euo pipefail

source "${SRCROOT}/../apple/native_build.sh"
FRAMEWORKS_DIRECTORY="${TARGET_BUILD_DIR}/${FRAMEWORKS_FOLDER_PATH}"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-12.0}"
SWIFT_TARGET="$(uname -m)-apple-macosx${MACOSX_DEPLOYMENT_TARGET}"
floe_native_artifact "${FRAMEWORKS_DIRECTORY}/libfloe_eventkit.dylib" swift \
  -emit-library -warnings-as-errors -target "${SWIFT_TARGET}" \
  "${SRCROOT}/CalendarActions/EventKitActions.swift"
floe_native_artifact "${FRAMEWORKS_DIRECTORY}/libfloe_local_model.dylib" swift \
  -emit-library -swift-version 6 -warnings-as-errors -target "${SWIFT_TARGET}" \
  -Xlinker -weak_framework -Xlinker FoundationModels \
  "${SRCROOT}/LocalModel/LocalModel.swift" \
  "${SRCROOT}/../apple/FloeAppleHealth/Sources/FloeAppleHealth/HealthPrivacyTransform.swift"
