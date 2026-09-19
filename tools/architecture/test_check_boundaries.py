#!/usr/bin/env python3
"""Focused tests for the read-only architecture checker."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from check_boundaries import dev_deps, graph_errors, production_deps


class DependencyExtractionTests(unittest.TestCase):
    def test_workspace_alias_and_target_build_are_production_edges(self):
        document = {
            "dependencies": {"runtime": {"workspace": True}},
            "target": {
                "cfg(unix)": {
                    "build-dependencies": {"generator": {"package": "floe-generator"}}
                }
            },
            "dev-dependencies": {"fixture": "1"},
        }
        workspace = {"runtime": {"package": "floe-runtime"}}
        self.assertEqual(
            production_deps(document, workspace), ["floe-runtime", "floe-generator"]
        )
        self.assertEqual(dev_deps(document, workspace), ["fixture"])

    def test_target_specific_dev_wiring_is_separate(self):
        document = {
            "target": {
                "cfg(test)": {
                    "dev-dependencies": {"fixture": {"workspace": True}}
                }
            }
        }
        self.assertEqual(dev_deps(document, {"fixture": {"package": "floe-fixture"}}), ["floe-fixture"])
        self.assertEqual(production_deps(document, {}), [])


class GraphTests(unittest.TestCase):
    def test_dag(self):
        self.assertEqual(graph_errors({"a": {"b"}, "b": set()}, []), [])

    def test_cycle(self):
        self.assertTrue(graph_errors({"a": {"b"}, "b": {"a"}}, []))

    def test_transitive_forbidden_path(self):
        self.assertTrue(
            graph_errors({"a": {"b"}, "b": {"c"}, "c": set()}, [("a", "c")])
        )


class CliTests(unittest.TestCase):
    checker = Path(__file__).with_name("check_boundaries.py")

    def run_check(self, cargo_manifests, extra_args=(), include_repo=True, workspace_members=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            members = [path for _, path, _ in cargo_manifests] if workspace_members is None else workspace_members
            member_lines = ", ".join(json.dumps(path) for path in members)
            (root / "Cargo.toml").write_text(
                f"[workspace]\nmembers = [{member_lines}]\n[workspace.dependencies]\n"
                'shared = { package = "floe-shared", version = "0.1" }\n',
                encoding="utf-8",
            )
            for name, path, manifest in cargo_manifests:
                destination = root / path
                destination.mkdir(parents=True)
                (destination / "Cargo.toml").write_text(manifest, encoding="utf-8")
            command = [sys.executable, str(self.checker)]
            if include_repo:
                command.append(str(root))
            command.extend(extra_args)
            result = subprocess.run(
                command,
                cwd=root,
                capture_output=True,
                text=True,
                check=False,
            )
            return result, json.loads(result.stdout)

    def test_default_repo_and_policy_paths_work(self):
        result = subprocess.run(
            [sys.executable, str(self.checker)],
            cwd=tempfile.gettempdir(),
            capture_output=True,
            text=True,
            check=False,
        )
        report = json.loads(result.stdout)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(report["mode"], "final")
        self.assertGreater(report["nodes"], 0)

    def test_dev_wiring_is_reported_but_not_gated(self):
        manifest = """[package]
name = "floe-kernel"
version = "0.1.0"

[dev-dependencies]
fixture = "1"

[target.'cfg(unix)'.dev-dependencies]
platform-fixture = "1"
"""
        result, report = self.run_check(
            [("floe-kernel", "crates/contracts/kernel", manifest)],
            workspace_members=[],
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            report["dev_wiring"]["floe-kernel"], ["fixture", "platform-fixture"]
        )

    def test_target_crate_cannot_depend_on_unmigrated_legacy_crate(self):
        conversation = """[package]
name = "floe-conversation"
version = "0.1.0"

[dependencies]
floe-agent-contract = "0.1"
"""
        legacy_contract = """[package]
name = "floe-agent-contract"
version = "0.1.0"
"""
        result, report = self.run_check(
            [
                ("floe-conversation", "crates/modules/conversation", conversation),
                ("floe-agent-contract", "crates/floe-agent-contract", legacy_contract),
            ]
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(
            any("unmigrated/legacy crate floe-agent-contract" in error for error in report["errors"])
        )

    def test_final_mode_requires_all_target_crates(self):
        result, report = self.run_check([])
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(any("missing target crate" in error for error in report["errors"]))

    def test_target_manifest_must_be_a_workspace_member(self):
        manifest = """[package]
name = "floe-kernel"
version = "0.1.0"
"""
        result, report = self.run_check(
            [("floe-kernel", "crates/contracts/kernel", manifest)]
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(
            "floe-kernel: target crate is not a workspace member", report["errors"]
        )


if __name__ == "__main__":
    unittest.main()
