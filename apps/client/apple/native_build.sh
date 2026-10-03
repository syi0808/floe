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
      swift-package)
        local package_root="$1"
        local target_triple="$2"
        local target_sdk="$3"
        local package_scratch="${NATIVE_CACHE_DIRECTORY}/swiftpm-${target_triple}"
        local -a package_options=(
          --package-path "${package_root}"
          --scratch-path "${package_scratch}"
          --cache-path "${NATIVE_CACHE_DIRECTORY}/swiftpm-cache"
          --config-path "${NATIVE_CACHE_DIRECTORY}/swiftpm-config"
          --security-path "${NATIVE_CACHE_DIRECTORY}/swiftpm-security"
          --configuration debug
          --triple "${target_triple}"
          --sdk "${target_sdk}"
          --product floe_local_model
          -Xswiftc -warnings-as-errors
          -Xswiftc -module-cache-path
          -Xswiftc "${NATIVE_CACHE_DIRECTORY}/module-cache"
          -Xlinker -weak_framework
          -Xlinker FoundationModels
        )
        local package_bin
        if xcrun swift build "${package_options[@]}" \
          && package_bin="$(xcrun swift build "${package_options[@]}" --show-bin-path)"; then
          cp "${package_bin}/libfloe_local_model.dylib" "${temporary}"
        else
          false
        fi
        ;;
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

# Every transitive source of the local native package participates in the cache
# key. Live hosts are linked only into this dynamic product, never into Runner.
floe_native_model_package() {
  local destination="$1"
  local target_triple="$2"
  local target_sdk="$3"
  local package_root="${SRCROOT}/../native/FloeNative"
  local -a package_inputs=(
    "${package_root}/Package.swift"
    "${package_root}"/Sources/**/*.swift(N.)
  )
  floe_native_artifact "${destination}" swift-package \
    "${package_root}" "${target_triple}" "${target_sdk}" "${package_inputs[@]}"
}
