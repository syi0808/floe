#!/usr/bin/env python3
"""Regression fixtures for ConnectionObserve conformance rules."""

from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from check_connection_observe_conformance import check_tree


class ConnectionObserveConformanceTests(unittest.TestCase):
    def check_fixture(self, files: dict[str, str]) -> list[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for relative, contents in files.items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(contents, encoding="utf-8")
            return [violation.rule for violation in check_tree(root)]

    def test_forbidden_rule_families(self):
        examples = [
            ("manager-source-policy", "crates/modules/context/src/lib.rs", "fn manager_direct_remote_view() {}\n"),
            ("manager-source-policy", "crates/app/src/lib.rs", "manager_direct_native_connector(connector);\n"),
            ("assistant-standing-consumer", "crates/app/src/first_party_observe.rs", 'consumers.push(GrantConsumer::builtin("assistant"));\n'),
            ("assistant-standing-consumer", "crates/app/src/first_party_observe.rs", 'consumers.push(GrantConsumer::builtin(ASSISTANT_CONSUMER));\n'),
            ("legacy-observe-authority", "crates/app/src/lib.rs", "fn old() { remote_policies_for_target(); }\n"),
            ("standing-observe-field", "crates/app/src/connection_observe.rs", "pub struct ConnectionObserveOverview { pub granted_resources: Vec<String> }\n"),
            ("standing-observe-field", "crates/bindings/protocol/src/dto/connection_observe.rs", "pub struct ConnectionObserveMutationDto { pub calendar_ids: Vec<String> }\n"),
            ("standing-observe-field", "crates/app/src/connection_observe.rs", "pub enum ConnectionObserveOperation { Review { resource: String } }\n"),
            ("mutable-first-party-policy", "crates/app/src/first_party_observe.rs", "let registry = AgentRegistry::new();\n"),
            ("calendar-candidate-leaf", "crates/modules/context/src/application/source_candidates.rs", '"calendar.timeline" => {\n for calendar in connection.resources() { add(calendar); }\n}\nATTENTION_VIEW_ID => {}\n'),
            ("gateway-leaf-selector", "apps/client/lib/features/connections/application/connection_observe_gateway.dart", "Future<void> review({required String calendarIds});\n"),
        ]
        for rule, path, contents in examples:
            with self.subTest(rule=rule, path=path):
                self.assertIn(rule, self.check_fixture({path: contents}))

    def test_allowed_owner_values_and_test_literature(self):
        self.assertEqual(
            self.check_fixture({
                "crates/modules/access/src/feasibility.rs": 'let purpose = GrantPurpose::Assistant; let consumer = GrantConsumer::builtin("assistant");\n',
                "crates/modules/runtime/src/tools.rs": 'impl ToolPort for Example {} struct ToolDescriptor;\n',
                "crates/modules/connections/src/source.rs": "struct SourceConfig { calendar_ids: Vec<String>, selected_handles: Vec<String> }\n",
                "crates/app/src/connection_observe.rs": "pub struct ConnectionObserveReviewedMember { pub resource: String }\n",
                "crates/app/src/first_party_observe.rs": "let shipped = floe_experts_builtin::manifests(); let purpose = GrantPurpose::Assistant;\n#[cfg(test)]\nmod tests { let old = \"ConsumerPolicyAuthority\"; }\n",
                "crates/bindings/protocol/tests/negative.rs": "let old = \"selected_resources\";\n",
            }),
            [],
        )

    def test_cli_reports_rule_and_location(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "crates/app/src/first_party_observe.rs"
            path.parent.mkdir(parents=True)
            path.write_text("let okay = true;\nlet binding = current.binding;\n", encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(Path(__file__).with_name("check_connection_observe_conformance.py")), str(root)],
                capture_output=True, text=True, check=False,
            )
            self.assertEqual(result.returncode, 1)
            self.assertIn("mutable-first-party-policy: crates/app/src/first_party_observe.rs:2", result.stdout)


if __name__ == "__main__":
    unittest.main()
