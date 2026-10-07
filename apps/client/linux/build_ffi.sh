#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIRECTORY="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
CLIENT_DIRECTORY="$(cd -- "${SCRIPT_DIRECTORY}/.." && pwd)"
REPOSITORY_ROOT="$(cd -- "${CLIENT_DIRECTORY}/../.." && pwd)"
FFI_MANIFEST="${REPOSITORY_ROOT}/crates/bindings/ffi/Cargo.toml"

if [[ "${1:-Debug}" != "Debug" ]]; then
  echo "Linux native builds are supported only in Debug QA mode." >&2
  exit 2
fi

case "${2:-}" in
  linux-x64) EXPECTED_HOST_ARCH="x86_64" ;;
  linux-arm64) EXPECTED_HOST_ARCH="aarch64" ;;
  *)
    echo "Unsupported Flutter Linux target: ${2:-unset}." >&2
    exit 2
    ;;
esac

if [[ "$(uname -m)" != "${EXPECTED_HOST_ARCH}" ]]; then
  echo "The Linux native core must be built on the same architecture as the Flutter target (${EXPECTED_HOST_ARCH})." >&2
  exit 2
fi

# This option is additive to the single development-storage runtime profile.
# The fixture feature is optional and must exist in floe-ffi before it is used.
FEATURES="development-storage"
case "${FLOE_LINUX_QA_FEATURE:-}" in
  "") ;;
  qa-fixtures)
    if ! grep -Eq '^[[:space:]]*qa-fixtures[[:space:]]*=' "${FFI_MANIFEST}"; then
      echo "FLOE_LINUX_QA_FEATURE=qa-fixtures was requested, but floe-ffi does not declare that feature yet." >&2
      exit 2
    fi
    FEATURES="development-storage,qa-fixtures"
    ;;
  *)
    echo "FLOE_LINUX_QA_FEATURE must be unset or qa-fixtures." >&2
    exit 2
    ;;
esac

if ! command -v cargo >/dev/null 2>&1; then
  echo "Cargo is required to build the Linux Floe native library." >&2
  exit 127
fi

cargo build \
  --manifest-path "${REPOSITORY_ROOT}/Cargo.toml" \
  --package floe-ffi \
  --no-default-features \
  --features "${FEATURES}"
