import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[2]


def digest_file(path):
    checksum = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            checksum.update(chunk)
    return checksum.hexdigest()


def capture(command, directory):
    return subprocess.check_output(command, cwd=directory, text=True).strip()


def specification(kind):
    if kind == "calendar":
        sdk = capture(["xcrun", "--sdk", "macosx", "--show-sdk-path"], ROOT)
        architecture = os.uname().machine
        source = ROOT / "crates/adapters/providers/tests/fixtures/NativeCalendarFixture.swift"
        command = ["xcrun", "swiftc", "-emit-library", "-warnings-as-errors",
                   "-sdk", sdk, "-target", f"{architecture}-apple-macosx12.0", str(source)]
        identity = capture(["xcrun", "swiftc", "--version"], ROOT)
        inputs = [source]
        for name in ("SDKSettings.json", "SDKSettings.plist"):
            settings = Path(sdk) / name
            if settings.exists():
                inputs.append(settings)
        return "libfloe_eventkit.dylib", inputs, command, identity, ROOT
    directory = ROOT / "server"
    listing = capture(["go", "list", "-deps", "-json", "./cmd/floe-server"], directory)
    decoder = json.JSONDecoder()
    inputs = {directory / "go.mod"}
    if (directory / "go.sum").exists():
        inputs.add(directory / "go.sum")
    while listing.strip():
        package, offset = decoder.raw_decode(listing.lstrip())
        listing = listing.lstrip()[offset:]
        package_directory = Path(package["Dir"])
        if package_directory.is_relative_to(ROOT):
            for field in ("GoFiles", "CgoFiles", "CFiles", "HFiles", "SFiles", "SysoFiles", "EmbedFiles"):
                inputs.update(package_directory / name for name in package.get(field, []))
    settings = capture(["go", "env", "-json", "GOOS", "GOARCH", "CGO_ENABLED", "GOFLAGS",
                        "GOTOOLCHAIN", "CC", "CXX", "GOEXPERIMENT", "GOWORK",
                        "CGO_CFLAGS", "CGO_CPPFLAGS", "CGO_CXXFLAGS", "CGO_LDFLAGS"], directory)
    environment = json.loads(settings)
    work = environment["GOWORK"]
    if work and work != "off":
        inputs.add(Path(work))
        work_sum = Path(work + ".sum")
        if work_sum.exists():
            inputs.add(work_sum)
    identity = capture(["go", "version"], directory) + settings
    if environment["CGO_ENABLED"] == "1":
        identity += capture([*shlex.split(environment["CC"]), "--version"], directory)
    identity += json.dumps({name: os.environ.get(name) for name in
                            ("SDKROOT", "DEVELOPER_DIR", "MACOSX_DEPLOYMENT_TARGET")}, sort_keys=True)
    return "floe-server", sorted(inputs), ["go", "build", "./cmd/floe-server"], identity, directory


def cached_build(cache, name, inputs, command, identity, directory):
    cache.mkdir(parents=True, exist_ok=True)
    fingerprint = hashlib.sha256()
    fingerprint.update(json.dumps([command, identity, str(directory)], sort_keys=True).encode())
    for source in sorted([*inputs, Path(__file__).resolve()]):
        fingerprint.update(json.dumps([str(source), digest_file(source)]).encode())
    key = fingerprint.hexdigest()
    artifact_directory = cache / key
    output = artifact_directory / name
    stamp = artifact_directory / "sha256"
    with (cache / f"{key}.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if output.is_file() and stamp.is_file() and stamp.read_text() == digest_file(output):
            return output
        artifact_directory.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="build-", dir=artifact_directory) as temporary:
            candidate = Path(temporary) / name
            if command[0] == "go":
                build = [*command[:-1], "-o", str(candidate), command[-1]]
            else:
                build = [*command, "-o", str(candidate)]
            subprocess.run(build, cwd=directory, check=True, stdout=sys.stderr)
            if name.endswith(".dylib"):
                subprocess.run(["install_name_tool", "-id", f"@rpath/{name}", str(candidate)], check=True)
                subprocess.run(["codesign", "--force", "--sign", "-", str(candidate)], check=True,
                               stdout=sys.stderr)
            checksum = digest_file(candidate)
            candidate.replace(output)
            temporary_stamp = Path(temporary) / "sha256"
            temporary_stamp.write_text(checksum)
            temporary_stamp.replace(stamp)
    return output


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("fixture", choices=("calendar", "server"))
    arguments = parser.parse_args()
    name, inputs, command, identity, directory = specification(arguments.fixture)
    cache = ROOT / "target/test-fixtures" / arguments.fixture
    print(cached_build(cache, name, inputs, command, identity, directory))


if __name__ == "__main__":
    main()
