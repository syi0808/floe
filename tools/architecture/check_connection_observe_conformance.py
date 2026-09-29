#!/usr/bin/env python3
"""Reject source-level regressions in Connection-owned Observe authority."""

from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path

from check_expert_extensibility import _production_lines


@dataclass(frozen=True)
class Violation:
    rule: str
    path: str
    line: int

    def __str__(self) -> str:
        return f"{self.rule}: {self.path}:{self.line}"


LEGACY = re.compile(
    r"\b(?:ConsumerPolicyAuthority|selected_shipped_consumers|"
    r"native_calendar_policy_for_target|remote_policies_for_target|"
    r"remote_member_policy_digest_for_target|native_member_policy_digest_for_target|"
    r"RemoteCalendarQuery|SignedCalendarPreview|RemoteCalendarSourceReference|"
    r"CalendarAccessChange|CalendarAccessOverview|PersonalAccessChange|"
    r"ContactsAccessChange|PersonalAccessOverview|remote_calendar_grant(?:_\w+)?|"
    r"consumer_policy|policy_authority|policy_incarnation|policy_epoch)\b"
)
FORBIDDEN_POLICY = re.compile(
    r"\b(?:AgentRegistry|registry|assignments?|binding|installations?|"
    r"selected_resources|source_resources|connection_id|person_id|vault|"
    r"installed_extensions?|extension_package_ids?)\b"
)
FORBIDDEN_FIELDS = re.compile(
    r"\b(?:calendar_ids|selected_handles|consumer_policy|policy_authority|"
    r"selected_resources|granted_resources)\b"
)


def _items(lines: list[tuple[int, str]]) -> list[tuple[int, str, str]]:
    items: list[tuple[int, str, str]] = []
    for index, (number, line) in enumerate(lines):
        found = re.search(r"\b(?:enum|struct)\s+(ConnectionObserve\w+)", line)
        if not found:
            continue
        depth = 0
        body = []
        opened = False
        for _, current in lines[index:]:
            if "{" in current:
                opened = True
            if opened:
                body.append(current)
                depth += current.count("{") - current.count("}")
                if depth == 0:
                    break
        items.append((number, found.group(1), "\n".join(body)))
    return items


def check_tree(root: Path) -> list[Violation]:
    violations: list[Violation] = []
    for base in (root / "crates", root / "server", root / "apps/client/lib"):
        if not base.exists():
            continue
        for path in sorted(base.rglob("*")):
            if path.suffix not in (".rs", ".dart", ".go") or "/tests/" in path.as_posix():
                continue
            relative = path.relative_to(root).as_posix()
            lines = _production_lines(path)
            for number, line in lines:
                if LEGACY.search(line):
                    violations.append(Violation("legacy-observe-authority", relative, number))
                if relative == "crates/app/src/first_party_observe.rs" and FORBIDDEN_POLICY.search(line):
                    violations.append(Violation("mutable-first-party-policy", relative, number))
            if relative in (
                "crates/app/src/connection_observe.rs",
                "crates/bindings/protocol/src/dto/connection_observe.rs",
            ):
                for number, name, body in _items(lines):
                    if name.endswith("ReviewedMember") or name.endswith("ReviewedMemberDto"):
                        continue
                    if FORBIDDEN_FIELDS.search(body) or re.search(r"\b(?:pub\s+)?resource\s*:", body):
                        violations.append(Violation("standing-observe-field", relative, number))
            if relative == "apps/client/lib/features/connections/application/connection_observe_gateway.dart":
                for number, line in lines:
                    if re.search(r"\b(?:calendarIds|selectedHandles)\b|\b(?:String\??\s+resource|resource\s*:)", line):
                        violations.append(Violation("gateway-leaf-selector", relative, number))
            if relative == "crates/modules/context/src/application/source_candidates.rs":
                text = "\n".join(line for _, line in lines)
                match = re.search(r'"calendar\.timeline"\s*=>\s*\{(.*?)\n\s*ATTENTION_VIEW_ID', text, re.S)
                branch = match.group(1) if match else ""
                if ("for connection in request.source_connections" not in branch
                        or "connection.is_serving()" not in branch
                        or "connection_view_resource(" not in branch
                        or branch.count("add(") != 1
                        or re.search(r"for\s+\w+\s+in\s+connection\.resources\s*\(", branch)):
                    number = text[:match.start()].count("\n") + 1 if match else 1
                    violations.append(Violation("calendar-candidate-leaf", relative, number))
    return sorted(violations, key=lambda value: (value.path, value.line, value.rule))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", nargs="?", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    violations = check_tree(args.root.resolve())
    for violation in violations:
        print(violation)
    if violations:
        return 1
    print("connection observe source semantics: pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
