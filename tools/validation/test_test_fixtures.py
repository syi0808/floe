import importlib.util
from pathlib import Path
import subprocess
import tempfile
import threading
import unittest
from concurrent.futures import ThreadPoolExecutor
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location("fixtures", Path(__file__).with_name("build_test_fixtures.py"))
FIXTURES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FIXTURES)


class FixtureBuildTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / "input.go"
        self.source.write_text("source")
        self.cache = self.root / "cache"
        self.builds = 0

    def compile(self, command, **arguments):
        self.builds += 1
        Path(command[command.index("-o") + 1]).write_bytes(self.source.read_bytes())

    def build(self, identity="toolchain"):
        return FIXTURES.cached_build(self.cache, "server", [self.source],
                                    ["go", "build", "./server"], identity, self.root)

    def test_same_inputs_build_once_and_missing_output_rebuilds(self):
        with patch.object(FIXTURES.subprocess, "run", side_effect=self.compile):
            first = self.build()
            self.assertEqual(first, self.build())
            self.assertEqual(self.builds, 1)
            first.unlink()
            self.build()
            self.assertEqual(self.builds, 2)

    def test_source_toolchain_and_modified_output_invalidate(self):
        with patch.object(FIXTURES.subprocess, "run", side_effect=self.compile):
            first = self.build()
            first.write_text("modified")
            self.build()
            self.source.write_text("changed source")
            second = self.build()
            third = self.build("changed toolchain")
            self.assertNotEqual(first, second)
            self.assertNotEqual(second, third)
            self.assertEqual(self.builds, 4)

    def test_failure_does_not_publish_an_artifact(self):
        with patch.object(FIXTURES.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "go")):
            with self.assertRaises(subprocess.CalledProcessError):
                self.build()
        self.assertFalse(list(self.cache.glob("*/server")))
        self.assertFalse(list(self.cache.glob("*/sha256")))
        self.assertFalse(list(self.cache.glob("*/build-*")))

    def test_concurrent_callers_share_one_complete_build(self):
        started = threading.Event()
        release = threading.Event()

        def blocked_compile(command, **arguments):
            started.set()
            self.assertTrue(release.wait(5))
            self.compile(command, **arguments)

        with patch.object(FIXTURES.subprocess, "run", side_effect=blocked_compile):
            with ThreadPoolExecutor(max_workers=2) as executor:
                first = executor.submit(self.build)
                self.assertTrue(started.wait(5))
                second = executor.submit(self.build)
                release.set()
                self.assertEqual(first.result(timeout=5), second.result(timeout=5))
        self.assertEqual(self.builds, 1)


if __name__ == "__main__":
    unittest.main()
