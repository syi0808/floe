import os
from pathlib import Path
import subprocess
import tempfile
import unittest


HELPER = Path(__file__).resolve().parents[2] / "apps/client/apple/native_build.sh"


class NativeBuildTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="floe native build ")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / "Model.swift"
        self.source.write_text("model")
        self.output = self.root / "bundle/libmodel.dylib"
        self.log = self.root / "operations"
        for name in ("build_rust.sh", "build_native.sh"):
            (self.root / name).write_text(name)
        binaries = self.root / "bin"
        binaries.mkdir()
        executable = binaries / "tool"
        executable.write_text(
            "#!/usr/bin/env python3\n"
            "import os, pathlib, sys\n"
            "name = pathlib.Path(sys.argv[0]).name\n"
            "arguments = sys.argv[1:]\n"
            "if name == 'xcrun':\n"
            "    if arguments[0] == '--find':\n"
            "        print('/toolchain/swiftc'); sys.exit()\n"
            "    if arguments[0] == '--show-sdk-path':\n"
            "        print(os.environ.get('SDKROOT', '/sdk')); sys.exit()\n"
            "    if arguments == ['swiftc', '--version']:\n"
            "        print('Swift test toolchain'); sys.exit()\n"
            "    name = arguments.pop(0)\n"
            "with open(os.environ['TEST_LOG'], 'a') as log:\n"
            "    log.write(name + '\\n')\n"
            "if name == 'swiftc':\n"
            "    source = next(pathlib.Path(value) for value in arguments if value.endswith('.swift'))\n"
            "    if source.read_text() == 'fail': sys.exit(9)\n"
            "    pathlib.Path(arguments[arguments.index('-o') + 1]).write_bytes(source.read_bytes())\n"
            "elif name == 'lipo':\n"
            "    inputs = arguments[1:arguments.index('-output')]\n"
            "    pathlib.Path(arguments[-1]).write_bytes(b''.join(pathlib.Path(value).read_bytes() for value in inputs))\n"
            "elif name == 'codesign':\n"
            "    with open(arguments[-1], 'ab') as output:\n"
            "        output.write(arguments[arguments.index('--sign') + 1].encode())\n"
        )
        executable.chmod(0o755)
        for name in ("xcrun", "install_name_tool", "codesign"):
            (binaries / name).symlink_to(executable)
        self.environment = {
            **os.environ,
            "PATH": f"{binaries}:{os.environ['PATH']}",
            "SRCROOT": str(self.root),
            "DERIVED_FILE_DIR": str(self.root / "derived"),
            "TEST_LOG": str(self.log),
            "EXPANDED_CODE_SIGN_IDENTITY": "-",
        }

    def build(self, operation="swift", inputs=None, success=True):
        arguments = inputs or ["-emit-library", str(self.source)]
        result = subprocess.run(
            ["zsh", "-c", 'source "$1"; shift; floe_native_artifact "$@"',
             "native-test", str(HELPER), str(self.output), operation, *arguments],
            env=self.environment, capture_output=True, text=True,
        )
        self.assertEqual(result.returncode == 0, success, result.stderr)
        return result.stdout

    def operations(self):
        return self.log.read_text().splitlines()

    def test_unchanged_build_skips_compile_copy_and_sign(self):
        self.build()
        self.assertIn("unchanged", self.build())
        self.assertEqual(self.operations(), ["swiftc", "install_name_tool", "codesign"])

    def test_source_flags_sdk_identity_and_script_invalidate(self):
        self.build()
        self.source.write_text("new model")
        self.build()
        self.build(inputs=["-emit-library", "-swift-version", "6", str(self.source)])
        self.environment["SDKROOT"] = "/different-sdk"
        self.build()
        self.environment["EXPANDED_CODE_SIGN_IDENTITY"] = "new identity"
        self.build()
        (self.root / "build_native.sh").write_text("changed script")
        self.build()
        self.assertEqual(self.operations().count("swiftc"), 6)

    def test_missing_or_modified_output_rebuilds(self):
        self.build()
        self.output.unlink()
        self.build()
        self.output.write_text("modified output")
        self.build()
        self.assertEqual(self.operations().count("swiftc"), 3)

    def test_failed_build_preserves_output_and_does_not_mark_fresh(self):
        self.build()
        previous = self.output.read_bytes()
        self.source.write_text("fail")
        self.build(success=False)
        self.assertEqual(self.output.read_bytes(), previous)
        self.source.write_text("repaired")
        self.build()
        self.assertIn(b"repaired", self.output.read_bytes())
        self.assertFalse(list((self.root / "derived/floe-native").glob("build.*")))

    def test_embed_tracks_each_architecture_and_skips_unchanged(self):
        other = self.root / "other.dylib"
        other.write_text("other architecture")
        self.build(operation="embed", inputs=[str(self.source), str(other)])
        self.build(operation="embed", inputs=[str(self.source), str(other)])
        other.write_text("changed architecture")
        self.build(operation="embed", inputs=[str(self.source), str(other)])
        self.assertEqual(self.operations().count("lipo"), 2)

    def test_single_library_embed_tracks_source_content(self):
        self.build(operation="embed", inputs=[str(self.source)])
        self.build(operation="embed", inputs=[str(self.source)])
        self.source.write_text("new library")
        self.build(operation="embed", inputs=[str(self.source)])
        self.assertEqual(self.operations().count("codesign"), 2)


if __name__ == "__main__":
    unittest.main()
