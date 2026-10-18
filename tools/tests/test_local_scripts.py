"""Isolated shell fixtures: no real macOS data, processes or Keychain access."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ResetLocalDataTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name) / 'home'
        self.home.mkdir()
        self.bin = Path(self.tmp.name) / 'bin'
        self.bin.mkdir()
        self.stub('uname', 'echo Darwin')
        self.stub('pgrep', 'exit 1')
        self.stub('security', 'exit 99')
        self.root = self.home / 'Library/Application Support/app.floe.floeClient'
        self.root.mkdir(parents=True)
        self.data = self.root / 'development-storage'
        self.data.mkdir()
        (self.data / 'fixture').write_text('preserve me')

    def stub(self, name, body):
        path = self.bin / name
        path.write_text('#!/bin/bash\n' + body + '\n')
        path.chmod(0o755)

    def run_reset(self, *args, input=''):
        env = {'PATH': f'{self.bin}:/usr/bin:/bin', 'HOME': str(self.home)}
        return subprocess.run(
            [os.environ.get('FLOE_TEST_BASH', '/bin/bash'),
             str(ROOT / 'scripts/reset-local-data.sh'), *args],
            env=env, input=input, text=True, capture_output=True, timeout=10)

    def test_missing_trash_is_created_and_data_is_recoverable(self):
        result = self.run_reset('--yes')
        self.assertEqual(result.returncode, 0, result.stderr)
        trash = self.home / '.Trash'
        self.assertEqual(trash.stat().st_mode & 0o777, 0o700)
        backups = list(trash.glob('Floe-development-reset-*'))
        self.assertEqual(len(backups), 1)
        self.assertEqual((backups[0] / 'client-0-development-storage/fixture').read_text(), 'preserve me')
        self.assertIn(str(backups[0]), result.stdout)
        self.assertFalse(self.data.exists())

    def test_no_data_does_not_create_trash(self):
        (self.data / 'fixture').unlink()
        self.data.rmdir()
        result = self.run_reset('--yes')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.home / '.Trash').exists())

    def test_trash_link_is_refused(self):
        target = Path(self.tmp.name) / 'other'
        target.mkdir()
        (self.home / '.Trash').symlink_to(target, target_is_directory=True)
        result = self.run_reset('--yes')
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(self.data.exists())
        self.assertEqual(list(target.iterdir()), [])

    def test_trash_file_is_refused(self):
        (self.home / '.Trash').write_text('unrelated')
        result = self.run_reset('--yes')
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(self.data.exists())
        self.assertEqual((self.home / '.Trash').read_text(), 'unrelated')

    def test_development_reset_preserves_production_data(self):
        production = self.root / 'people'
        production.mkdir()
        result = self.run_reset('--yes')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(production.exists())
        self.assertFalse(self.data.exists())

    def test_process_check_failure_and_cancel_preserve_data(self):
        for body in ['exit 0', 'exit 2']:
            self.stub('pgrep', body)
            result = self.run_reset('--yes')
            self.assertNotEqual(result.returncode, 0)
            self.assertTrue(self.data.exists())
        self.stub('pgrep', 'exit 1')
        result = self.run_reset(input='no\n')
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(self.data.exists())
        self.assertFalse((self.home / '.Trash').exists())

    def test_recovery_path_is_printed_before_failed_move(self):
        self.stub('mv', 'exit 1')
        result = self.run_reset('--yes')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Recoverable data:', result.stdout)
        self.assertTrue(self.data.exists())


if __name__ == '__main__':
    unittest.main()
