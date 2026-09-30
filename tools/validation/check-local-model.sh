#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."
mkdir -p target/validation
xcrun swiftc -swift-version 6 -warnings-as-errors \
  -target "$(uname -m)-apple-macosx12.0" \
  apps/client/macos/LocalModel/LocalModel.swift \
  tools/validation/LocalModelHostTests.swift \
  -o target/validation/local-model-host-tests
target/validation/local-model-host-tests
cargo test -p floe-provider-adapters
cargo test -p floe-inference
