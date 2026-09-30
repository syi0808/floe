#!/usr/bin/env python3
"""Regression fixtures for Expert source-semantic architecture rules."""

from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from check_expert_extensibility import check_tree


class SourceSemanticTests(unittest.TestCase):
    def check_fixture(self, files: dict[str, str]) -> list[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative, contents in files.items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(contents, encoding="utf-8")
            return [violation.rule for violation in check_tree(root)]

    def test_each_forbidden_semantic_has_a_negative_fixture(self):
        examples = [
            ("manager-domain-tools", "crates/modules/context/src/lib.rs", "struct ContextToolService;\n"),
            ("manager-domain-tools", "crates/app/src/vault_host/conversation_turn.rs", "tools: manager_tool_descriptors(),\n"),
            ("manager-domain-tools", "crates/app/src/vault_host/conversation_turn.rs", 'tool_id: "mail.communication.read",\n'),
            ("manager-domain-tools", "crates/app/src/vault_host/publication.rs", "struct PublishingToolPort;\n"),
            (
                "deleted-source-read",
                "crates/adapters/providers/src/sources/server.rs",
                "fn read_communication_view() {}\n",
            ),
            (
                "builtin-source-in-app",
                "crates/app/src/vault_host/reader.rs",
                "use floe_experts_builtin::BuiltinContextSource;\n",
            ),
            (
                "builtin-dispatch-in-app",
                "crates/app/src/vault_host/dispatch.rs",
                "match BuiltinExpertKind::Schedule { _ => () }\n",
            ),
            (
                "common-depends-on-builtin",
                "crates/modules/context/src/lib.rs",
                "use floe_experts_builtin::BuiltinExpertKind;\n",
            ),
            (
                "tool-shaped-registry",
                "crates/modules/experts/src/registry.rs",
                "let kind = PackageKind::Tool;\n",
            ),
            (
                "removed-shared-result",
                "crates/contracts/agent/src/expert.rs",
                "pub struct ExpertBudget;\n",
            ),
            (
                "flutter-package-branch",
                "apps/client/lib/features/experts/presentation/agent_registry_dialog.dart",
                "if (packageId == 'floe.builtin.schedule') {}\n",
            ),
            (
                "removed-calendar-wire",
                "crates/bindings/protocol/src/lib.rs",
                "enum Command { ScheduleEndpoint }\n",
            ),
            (
                "legacy-source-forwarder",
                "crates/adapters/providers/src/sources/server.rs",
                "fn legacy_source_read() {}\n",
            ),
        ]
        for rule, path, contents in examples:
            with self.subTest(rule=rule):
                self.assertIn(rule, self.check_fixture({path: contents}))

    def test_deleted_read_cannot_hide_in_test_only_fixture(self):
        self.assertIn(
            "deleted-source-read",
            self.check_fixture({
                "crates/adapters/providers/tests/old_source.rs":
                    "client.read_calendar_context_view().await;\n"
            }),
        )

    def test_package_literal_dispatch_is_not_an_app_extension_point(self):
        self.assertIn(
            "builtin-dispatch-in-app",
            self.check_fixture({
                "crates/app/src/vault_host/dispatch.rs":
                    'match package_id { "floe.builtin.schedule" => run(), _ => () }\n'
            }),
        )

    def test_common_manifest_dependency_is_forbidden_but_dev_wiring_is_not(self):
        manifest = """[package]
name = "floe-context"
version = "0.1.0"
[dependencies]
builtin = { package = "floe-experts-builtin", version = "0.1" }
"""
        self.assertIn(
            "common-depends-on-builtin",
            self.check_fixture({"crates/modules/context/Cargo.toml": manifest}),
        )
        self.assertNotIn(
            "common-depends-on-builtin",
            self.check_fixture({
                "crates/modules/context/Cargo.toml": manifest.replace(
                    "[dependencies]", "[dev-dependencies]"
                )
            }),
        )

    def test_tool_presentation_and_legacy_route_variants_are_forbidden(self):
        self.assertIn(
            "tool-shaped-registry",
            self.check_fixture({
                "crates/modules/experts/src/registry.rs":
                    "enum PackageKind { Expert, Tool }\n"
            }),
        )
        self.assertIn(
            "tool-shaped-registry",
            self.check_fixture({
                "apps/client/lib/features/experts/presentation/agent_capability_label.dart":
                    "if (kind == .tool) return 'Connected information';\n"
            }),
        )
        self.assertIn(
            "removed-calendar-wire",
            self.check_fixture({
                "server/internal/transport/http/legacy.go":
                    'const route = "experts.calendar.install"\n'
            }),
        )
        self.assertIn(
            "deleted-source-read",
            self.check_fixture({
                "crates/adapters/providers/src/sources/server.rs": "fn read_view() {}\n"
            }),
        )

    def test_owner_scoped_package_and_generic_composition_are_allowed(self):
        self.assertEqual(
            self.check_fixture({
                "crates/experts/builtin/src/catalog.rs":
                    "enum BuiltinExpertKind { Schedule }\n"
                    "struct BuiltinContextSource;\n",
                "crates/app/src/vault_host/composition.rs":
                    "let registrations = floe_experts_builtin::registrations();\n"
                    "let shipped = floe_experts_builtin::manifests();\n",
                "crates/app/src/vault_host/production_with_tests.rs":
                    "fn dispatch() {}\n#[cfg(test)]\nmod tests {\n"
                    "  use floe_experts_builtin::BuiltinExpertKind;\n}\n",
                "apps/client/lib/features/experts/presentation/dialog.dart":
                    "final same = entry.packageId == installation.packageId;\n",
                "crates/app/src/vault_host/actions.rs":
                    "const media = \"application/vnd.floe.calendar-proposal+json\";\n",
                "crates/bindings/protocol/tests/negative.rs":
                    'let old = "experts.calendar.install";\n',
            }),
            [],
        )

    def test_cli_names_rule_and_offending_path_line(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "crates/app/src/dispatch.rs"
            path.parent.mkdir(parents=True)
            path.write_text("fn okay() {}\nBuiltinContextSource::Tasks;\n", encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(Path(__file__).with_name("check_expert_extensibility.py")), str(root)],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("builtin-source-in-app: crates/app/src/dispatch.rs:2", result.stdout)


if __name__ == "__main__":
    unittest.main()
