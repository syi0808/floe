#!/bin/zsh
set -euo pipefail

REPOSITORY_ROOT="${0:A:h}/../../.."
OUTPUT_ROOT="${FLOE_ANDROID_JNI_LIBS_DIR:-${0:A:h}/app/src/main/jniLibs}"
API_LEVEL="${ANDROID_API_LEVEL:-26}"
ABIS=( ${(s: :)FLOE_ANDROID_ABIS:-arm64-v8a\ x86_64} )

if [[ -n "${ANDROID_NDK_HOME:-}" ]]; then
  NDK_HOME="${ANDROID_NDK_HOME}"
elif [[ -n "${ANDROID_NDK_ROOT:-}" ]]; then
  NDK_HOME="${ANDROID_NDK_ROOT}"
else
  SDK_DIR="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}"
  if [[ -z "${SDK_DIR}" && -f "${0:A:h}/local.properties" ]]; then
    SDK_DIR="$(sed -n 's/^sdk\.dir=//p' "${0:A:h}/local.properties" | head -n 1)"
    SDK_DIR="${SDK_DIR//\\/\/}"
  fi
  if [[ -z "${SDK_DIR}" || ! -d "${SDK_DIR}/ndk" ]]; then
    print -u2 "Android NDK is unavailable; set ANDROID_NDK_HOME"
    exit 1
  fi
  NDK_HOME="$(find "${SDK_DIR}/ndk" -mindepth 1 -maxdepth 1 -type d | sort | tail -n 1)"
fi

case "$(uname -s):$(uname -m)" in
  Darwin:arm64) HOST_TAG=darwin-arm64 ;;
  Darwin:*) HOST_TAG=darwin-x86_64 ;;
  Linux:x86_64) HOST_TAG=linux-x86_64 ;;
  *) print -u2 "Unsupported Android NDK host: $(uname -s) $(uname -m)"; exit 1 ;;
esac
TOOLCHAIN="${NDK_HOME}/toolchains/llvm/prebuilt/${HOST_TAG}"
if [[ ! -d "${TOOLCHAIN}" && "${HOST_TAG}" == "darwin-arm64" ]]; then
  HOST_TAG=darwin-x86_64
  TOOLCHAIN="${NDK_HOME}/toolchains/llvm/prebuilt/${HOST_TAG}"
fi
if [[ ! -d "${TOOLCHAIN}" ]]; then
  print -u2 "Android NDK toolchain is unavailable: ${TOOLCHAIN}"
  exit 1
fi

PROFILE="debug"
CARGO_FLAGS=()
if [[ "${CONFIGURATION:-Debug}" != "Debug" ]]; then
  PROFILE="release"
  CARGO_FLAGS+=(--release)
fi
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-0}"
rm -rf "${OUTPUT_ROOT}"

for abi in ${(u)ABIS}; do
  case "${abi}" in
    arm64-v8a)
      target=aarch64-linux-android
      compiler="aarch64-linux-android${API_LEVEL}-clang"
      ;;
    x86_64)
      target=x86_64-linux-android
      compiler="x86_64-linux-android${API_LEVEL}-clang"
      ;;
    *) print -u2 "Unsupported Android ABI: ${abi}"; exit 1 ;;
  esac
  if [[ ! -x "${TOOLCHAIN}/bin/${compiler}" || ! -x "${TOOLCHAIN}/bin/llvm-ar" ]]; then
    print -u2 "Android ABI compiler is unavailable: ${compiler}"
    exit 1
  fi
  if [[ "${target}" == "aarch64-linux-android" ]]; then
    export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="${TOOLCHAIN}/bin/${compiler}"
    export CC_aarch64_linux_android="${TOOLCHAIN}/bin/${compiler}"
    export AR_aarch64_linux_android="${TOOLCHAIN}/bin/llvm-ar"
    export RANLIB_aarch64_linux_android="${TOOLCHAIN}/bin/llvm-ranlib"
  else
    export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="${TOOLCHAIN}/bin/${compiler}"
    export CC_x86_64_linux_android="${TOOLCHAIN}/bin/${compiler}"
    export AR_x86_64_linux_android="${TOOLCHAIN}/bin/llvm-ar"
    export RANLIB_x86_64_linux_android="${TOOLCHAIN}/bin/llvm-ranlib"
  fi
  "${HOME}/.cargo/bin/cargo" build \
    --manifest-path "${REPOSITORY_ROOT}/Cargo.toml" \
    --package floe-ffi \
    --target "${target}" \
    "${CARGO_FLAGS[@]}"
  destination="${OUTPUT_ROOT}/${abi}"
  mkdir -p "${destination}"
  cp "${REPOSITORY_ROOT}/target/${target}/${PROFILE}/libfloe_ffi.so" \
    "${destination}/libfloe_ffi.so"
done
