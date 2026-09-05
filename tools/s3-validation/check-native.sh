#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
cp "$ROOT/tools/s3-validation/native-tests.swift" "$WORK/main.swift"
xcrun swiftc -warnings-as-errors "$ROOT/apps/client/macos/CalendarActions/EventKitActions.swift" "$WORK/main.swift" -o "$WORK/check"
"$WORK/check"
