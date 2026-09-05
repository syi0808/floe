#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
output="$root/target/eventkit-poc"
mkdir -p "$output"
swiftc -warnings-as-errors -target arm64-apple-macos14.0 \
  "$root/tools/eventkit-poc/main.swift" -o "$output/floe-eventkit-poc" \
  -Xlinker -sectcreate -Xlinker __TEXT -Xlinker __info_plist \
  -Xlinker "$root/tools/eventkit-poc/Info.plist"
codesign --force --sign - --identifier app.floe.eventkit-poc "$output/floe-eventkit-poc"
codesign --verify --strict "$output/floe-eventkit-poc"
printf '%s\n' "$output/floe-eventkit-poc"
