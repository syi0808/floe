#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "Usage: build_ffi.sh <Debug> <linux-x64|linux-arm64> <staged-library-path>" >&2
  exit 2
fi

BUILD_MODE="$1"
FLUTTER_TARGET="$2"
STAGED_LIBRARY="$3"
SCRIPT_DIRECTORY="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
CLIENT_DIRECTORY="$(cd -- "${SCRIPT_DIRECTORY}/.." && pwd)"
REPOSITORY_ROOT="$(cd -- "${CLIENT_DIRECTORY}/../.." && pwd)"
FFI_MANIFEST="${REPOSITORY_ROOT}/crates/bindings/ffi/Cargo.toml"

if [[ "${BUILD_MODE}" != "Debug" ]]; then
  echo "Linux native builds are supported only in Debug QA mode." >&2
  exit 2
fi

case "${FLUTTER_TARGET}" in
  linux-x64) EXPECTED_HOST_ARCH="x86_64" ;;
  linux-arm64) EXPECTED_HOST_ARCH="aarch64" ;;
  *)
    echo "Unsupported Flutter Linux target: ${FLUTTER_TARGET}." >&2
    exit 2
    ;;
esac

if [[ "$(uname -m)" != "${EXPECTED_HOST_ARCH}" ]]; then
  echo "The Linux native core must be built on the same architecture as the Flutter target (${EXPECTED_HOST_ARCH})." >&2
  exit 2
fi

if [[ "${STAGED_LIBRARY}" != /* || "${STAGED_LIBRARY##*/}" != "libfloe_ffi.so" ]]; then
  echo "The staged native-library path must be absolute and end in libfloe_ffi.so." >&2
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
if ! command -v python3 >/dev/null 2>&1; then
  echo "Python 3 is required to select Cargo's exact cdylib artifact." >&2
  exit 127
fi

python3 - "${REPOSITORY_ROOT}" "${FEATURES}" "${FLUTTER_TARGET}" "${STAGED_LIBRARY}" <<'PY'
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile

repository_root = Path(sys.argv[1]).resolve()
features = sys.argv[2]
flutter_target = sys.argv[3]
staged_library = Path(sys.argv[4])
manifest = repository_root / "Cargo.toml"

metadata = subprocess.run(
    [
        "cargo",
        "metadata",
        "--no-deps",
        "--format-version",
        "1",
        "--manifest-path",
        str(manifest),
    ],
    cwd=repository_root,
    capture_output=True,
    text=True,
)
if metadata.returncode != 0:
    if metadata.stderr:
        print(metadata.stderr, file=sys.stderr, end="")
    else:
        print(f"cargo metadata failed with exit code {metadata.returncode}.", file=sys.stderr)
    raise SystemExit(metadata.returncode)
try:
    metadata_output = json.loads(metadata.stdout)
    target_directory_value = metadata_output["target_directory"]
    if not isinstance(target_directory_value, str) or not target_directory_value:
        raise ValueError("target_directory is not a non-empty string")
    target_directory = Path(target_directory_value).resolve()
except (KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
    raise SystemExit(f"Cargo metadata did not report a target directory: {error}")

build = subprocess.Popen(
    [
        "cargo",
        "build",
        "--manifest-path",
        str(manifest),
        "--package",
        "floe-ffi",
        "--no-default-features",
        "--features",
        features,
        "--message-format=json-render-diagnostics",
    ],
    cwd=repository_root,
    stdout=subprocess.PIPE,
    text=True,
)
artifacts = []
malformed_messages = 0
assert build.stdout is not None
for line in build.stdout:
    if not line.strip():
        continue
    try:
        message = json.loads(line)
    except json.JSONDecodeError:
        malformed_messages += 1
        continue
    if message.get("reason") == "compiler-message":
        diagnostic = message.get("message", {})
        rendered = diagnostic.get("rendered")
        if rendered and diagnostic.get("level") in {"warning", "error"}:
            print(rendered, file=sys.stderr, end="" if rendered.endswith("\n") else "\n")
    if message.get("reason") != "compiler-artifact":
        continue
    package_id = message.get("package_id", "").rsplit("#", 1)[-1].split("@", 1)[0]
    target = message.get("target", {})
    if package_id != "floe-ffi" or target.get("name") != "floe_ffi":
        continue
    if "cdylib" not in target.get("kind", []):
        continue
    artifacts.extend(
        Path(filename)
        for filename in message.get("filenames", [])
        if Path(filename).name == "libfloe_ffi.so"
    )

build_status = build.wait()
if build_status != 0:
    raise SystemExit(build_status)
if malformed_messages:
    raise SystemExit(
        f"Cargo emitted {malformed_messages} non-JSON build messages; refusing to guess the cdylib path."
    )
unique_artifacts = {path.resolve() for path in artifacts}
if len(unique_artifacts) != 1:
    raise SystemExit(
        "Cargo did not report exactly one floe-ffi cdylib artifact named libfloe_ffi.so."
    )
artifact = unique_artifacts.pop()
if not artifact.is_file():
    raise SystemExit(f"Cargo reported a missing cdylib artifact: {artifact}")

expected_profile_directory = (target_directory / "debug").resolve()
if artifact.parent != expected_profile_directory:
    raise SystemExit(
        "The Cargo artifact uses a target-specific output directory "
        f"({artifact.parent}); Linux Flutter cross-target builds are unsupported."
    )

expected_machine = {"linux-x64": 62, "linux-arm64": 183}[flutter_target]
with artifact.open("rb") as library:
    header = library.read(20)
if len(header) != 20 or header[:4] != b"\x7fELF" or header[4] != 2:
    raise SystemExit(f"Cargo artifact is not a 64-bit ELF library: {artifact}")
byte_order = {1: "<", 2: ">"}.get(header[5])
if byte_order is None or struct.unpack(byte_order + "H", header[18:20])[0] != expected_machine:
    raise SystemExit(
        f"Cargo artifact architecture does not match {flutter_target}: {artifact}"
    )
if struct.unpack(byte_order + "H", header[16:18])[0] != 3:
    raise SystemExit(f"Cargo artifact is not an ELF shared library: {artifact}")

staged_library.parent.mkdir(parents=True, exist_ok=True)
staged_fd, temporary_path = tempfile.mkstemp(
    prefix=".libfloe_ffi.so.",
    dir=staged_library.parent,
)
os.close(staged_fd)
try:
    shutil.copyfile(artifact, temporary_path)
    os.chmod(temporary_path, 0o755)
    os.replace(temporary_path, staged_library)
except BaseException:
    try:
        os.unlink(temporary_path)
    except FileNotFoundError:
        pass
    raise

print(f"Staged {artifact} as {staged_library}")
PY
