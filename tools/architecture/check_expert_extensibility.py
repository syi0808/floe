#!/usr/bin/env python3
"""Reject source-level regressions in the generic Expert execution path."""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class Violation:
    rule: str
    path: str
    line: int

    def __str__(self) -> str:
        return f"{self.rule}: {self.path}:{self.line}"


DELETED_READS = re.compile(
    r"\b(?:read_communication_view|read_work_context_view|"
    r"read_calendar_context_view|read_confirmed_interaction_view|"
    r"read_logistics_view|read_people_view|read_attention_view|"
    r"read_wellbeing_view|read_personal_view)\s*\("
)
REMOVED_MANAGER_TOOLS = re.compile(
    r"\b(?:ContextToolService|manager_tool_descriptors|MANAGER_TOOL_DEFINITION_REVISION|"
    r"PublishingToolPort|ToolOutcomePort|blocked_tool_result|PEOPLE_IDENTITY_READ|"
    r"SCHEDULE_FEASIBILITY_READ|ATTENTION_COARSE_READ|WELLBEING_DERIVED_READ|"
    r"MAIL_COMMUNICATION_READ|WORK_CONTEXT_READ|LIFE_LOGISTICS_READ)\b|"
    r"[\"\'](?:people\.identity|schedule\.feasibility|attention\.coarse|wellbeing\.derived|"
    r"mail\.communication|work\.context|life\.logistics)\.read[\"\']"
)
REMOVED_RESULT = re.compile(
    r"\b(?:ExpertBudget|MAX_EXPERT_VIEW_BYTES|ExpertInput|ExpertInsight|"
    r"ExpertFocusProposal|StatefulFocusProposal|FindFocusWindow|"
    r"PackageImplementation)\b"
)
REMOVED_WIRE = re.compile(
    r"\b(?:ScheduleEndpoint|AgentCalendarExpert|CalendarExpert)\b|experts\.calendar\.install"
)
LEGACY_FORWARDER = re.compile(
    r"\b(?:legacy_(?:source_)?read(?:_[a-z_]+)?|"
    r"read_legacy_(?:source_)?[a-z_]+|compat_read_[a-z_]+|"
    r"read_[a-z_]+_compat|source_read_legacy)\s*\("
)
FLUTTER_PACKAGE_BRANCH = re.compile(
    r"floe\.builtin\.|\b(?:switch\s*\(\s*(?:packageId|package_id)|"
    r"(?:packageId|package_id)\s*==\s*['\"]|"
    r"case\s+['\"]floe\.)"
)
APP_PACKAGE_BRANCH = re.compile(
    r"['\"]floe\.builtin\.[^'\"]+['\"]\s*=>|"
    r"\b(?:package_id|packageId|agent_id|agentId)\s*==\s*['\"]floe\.builtin\."
)


def _production_lines(path: Path) -> list[tuple[int, str]]:
    lines = path.read_text(encoding="utf-8").splitlines()
    if path.suffix != ".rs":
        return [(number, line) for number, line in enumerate(lines, 1)]
    result: list[tuple[int, str]] = []
    skip_item = False
    brace_depth = 0
    for number, line in enumerate(lines, 1):
        stripped = line.strip()
        if stripped == "#[cfg(test)]":
            skip_item = True
            brace_depth = 0
            continue
        if skip_item:
            brace_depth += line.count("{") - line.count("}")
            if brace_depth > 0:
                continue
            if stripped.endswith(";") or stripped.endswith("}"):
                skip_item = False
            continue
        result.append((number, line.split("//", 1)[0]))
    return result


def _production_manifest_dependency(path: Path) -> bool:
    document = tomllib.loads(path.read_text(encoding="utf-8"))
    tables = [document.get("dependencies", {}), document.get("build-dependencies", {})]
    for target in document.get("target", {}).values():
        tables.extend((target.get("dependencies", {}), target.get("build-dependencies", {})))
    return any(
        name == "floe-experts-builtin"
        or isinstance(spec, dict) and spec.get("package") == "floe-experts-builtin"
        for table in tables
        for name, spec in table.items()
    )


def check_tree(root: Path) -> list[Violation]:
    violations: list[Violation] = []
    crates = root / "crates"
    client = root / "apps/client/lib"
    server = root / "server"
    rust_files = sorted(crates.rglob("*.rs")) if crates.exists() else []
    dart_files = sorted(client.rglob("*.dart")) if client.exists() else []
    go_files = sorted(server.rglob("*.go")) if server.exists() else []
    for path in [*rust_files, *dart_files, *go_files]:
        relative = path.relative_to(root).as_posix()
        is_rust = path.suffix == ".rs"
        is_app = relative.startswith("crates/app/src/")
        is_common = relative.startswith(("crates/modules/", "crates/runtime/"))
        is_flutter = relative.startswith("apps/client/lib/features/experts/")
        is_test_file = "/tests/" in relative or relative.endswith("/tests.rs")
        if is_rust:
            for number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                line = raw_line.split("//", 1)[0]
                if DELETED_READS.search(line) or (
                    relative.endswith("/sources/server.rs")
                    and re.search(r"\bread_view\s*\(", line)
                ):
                    violations.append(Violation("deleted-source-read", relative, number))
                if LEGACY_FORWARDER.search(line):
                    violations.append(Violation("legacy-source-forwarder", relative, number))
        lines = _production_lines(path) if not is_test_file else []
        in_package_kind = False
        for number, line in lines:
            if is_rust and re.search(r"\benum\s+PackageKind\b", line):
                in_package_kind = True
            if in_package_kind and re.search(r"\bTool\b", line):
                violations.append(Violation("tool-shaped-registry", relative, number))
            if in_package_kind and "}" in line:
                in_package_kind = False
            if is_app and re.search(r"\bBuiltinContextSource\b", line):
                violations.append(Violation("builtin-source-in-app", relative, number))
            if is_app and re.search(r"\bBuiltinExpertKind\b", line):
                violations.append(Violation("builtin-dispatch-in-app", relative, number))
            if is_app and APP_PACKAGE_BRANCH.search(line):
                violations.append(Violation("builtin-dispatch-in-app", relative, number))
            if is_common and re.search(r"\bfloe_experts_builtin\b", line):
                violations.append(Violation("common-depends-on-builtin", relative, number))
            if is_rust and re.search(r"\bPackageKind::Tool\b", line):
                violations.append(Violation("tool-shaped-registry", relative, number))
            if is_rust and REMOVED_MANAGER_TOOLS.search(line):
                violations.append(Violation("manager-domain-tools", relative, number))
            if REMOVED_RESULT.search(line):
                violations.append(Violation("removed-shared-result", relative, number))
            if REMOVED_WIRE.search(line):
                violations.append(Violation("removed-calendar-wire", relative, number))
            if is_flutter and FLUTTER_PACKAGE_BRANCH.search(line):
                violations.append(Violation("flutter-package-branch", relative, number))
            if is_flutter and re.search(r"kind\s*==\s*\.tool\b|Connected information", line):
                violations.append(Violation("tool-shaped-registry", relative, number))
    for path in sorted(crates.rglob("Cargo.toml")) if crates.exists() else []:
        relative = path.relative_to(root).as_posix()
        if not relative.startswith(("crates/modules/", "crates/runtime/")):
            continue
        if _production_manifest_dependency(path):
            for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                if "floe-experts-builtin" in line:
                    violations.append(Violation("common-depends-on-builtin", relative, number))
                    break
    return sorted(violations, key=lambda violation: (violation.path, violation.line, violation.rule))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", nargs="?", type=Path, default=Path(__file__).resolve().parents[2])
    arguments = parser.parse_args()
    violations = check_tree(arguments.root.resolve())
    for violation in violations:
        print(violation)
    if violations:
        return 1
    print("expert extensibility source semantics: pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
