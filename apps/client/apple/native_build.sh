#!/bin/zsh
set -euo pipefail

NATIVE_BUILD_HELPER="${${(%):-%x}:A}"
NATIVE_SIGNING_IDENTITY="${EXPANDED_CODE_SIGN_IDENTITY:--}"
if [[ -z "${NATIVE_SIGNING_IDENTITY}" ]]; then
  NATIVE_SIGNING_IDENTITY="-"
fi
NATIVE_CACHE_DIRECTORY="${DERIVED_FILE_DIR}/floe-native"
mkdir -p "${NATIVE_CACHE_DIRECTORY}"
NATIVE_TOOLCHAIN="$(xcrun --find swiftc):$(xcrun swiftc --version):$(xcrun --show-sdk-path):${DEVELOPER_DIR:-}:${SDKROOT:-}"

floe_native_artifact() {
  local destination="$1"
  local operation="$2"
  shift 2
  local cache_key="$(print -rn -- "${destination}" | shasum -a 256 | cut -d ' ' -f 1)"
  local stamp="${NATIVE_CACHE_DIRECTORY}/${cache_key}"
  local fingerprint="$(
    print -rl -- "${operation}" "${destination}" "${NATIVE_SIGNING_IDENTITY}" "${NATIVE_TOOLCHAIN}" "$@"
    shasum -a 256 "${NATIVE_BUILD_HELPER}" "${SRCROOT}/build_rust.sh" "${SRCROOT}/build_native.sh"
    for argument in "$@"; do
      if [[ -f "${argument}" ]]; then
        shasum -a 256 "${argument}"
      fi
    done
  )"
  fingerprint="$(print -rn -- "${fingerprint}" | shasum -a 256 | cut -d ' ' -f 1)"
  if [[ -f "${destination}" && -f "${stamp}" ]]; then
    local expected="${fingerprint}:$(shasum -a 256 "${destination}" | cut -d ' ' -f 1)"
    if [[ "$(<"${stamp}")" == "${expected}" ]]; then
      print -- "Floe native unchanged: ${destination:t}"
      return
    fi
  fi
  print -- "Floe native rebuild: ${destination:t}"
  mkdir -p "${destination:h}"
  local temporary_directory="$(mktemp -d "${NATIVE_CACHE_DIRECTORY}/build.XXXXXX")"
  local temporary="${temporary_directory}/${destination:t}"
  local build_status=0
  if {
    case "${operation}" in
      swift) xcrun swiftc "$@" -o "${temporary}" ;;
      embed)
        if (( $# == 1 )); then
          cp "$1" "${temporary}"
        else
          xcrun lipo -create "$@" -output "${temporary}"
        fi
        ;;
      *) print -u2 -- "Unknown native build operation: ${operation}"; false ;;
    esac
  } && install_name_tool -id "@rpath/${destination:t}" "${temporary}" \
    && codesign --force --sign "${NATIVE_SIGNING_IDENTITY}" "${temporary}" \
    && mv -f "${temporary}" "${destination}"; then
    print -r -- "${fingerprint}:$(shasum -a 256 "${destination}" | cut -d ' ' -f 1)" > "${stamp}.tmp" \
      && mv -f "${stamp}.tmp" "${stamp}" || build_status=$?
  else
    build_status=$?
  fi
  rm -rf "${temporary_directory}"
  return "${build_status}"
}
