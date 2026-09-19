#!/usr/bin/env python3
"""Validate Floe's active documentation hierarchy without external dependencies."""

from __future__ import annotations

import re
import sys
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[2]

RETIRED_PATHS = (
    ROOT / "PROGRESS.md",
    ROOT / "docs" / "planning",
    ROOT / "docs" / "history",
    ROOT / "docs" / "validation",
)

RETIRED_REFERENCES = (
    "docs/planning/",
    "../planning/",
    "../../planning/",
    "docs/validation/",
    "../validation/",
    "../../validation/",
    "docs/history/",
    "../history/",
    "../../history/",
    "PROGRESS.md",
)

RETIRED_CODE_PATHS = (
    "crates/floe-core",
    "crates/floe-domain",
    "crates/floe-infra",
)

LINK_RE = re.compile(r"!?\\[[^\\]]*\\]\\(([^)]+)\\)")


def markdown_files() -> list[Path]:
    ignored = {".git", "build", ".dart_tool", "target"}
    return [
        path
        for path in ROOT.rglob("*.md")
        if not any(part in ignored for part in path.parts)
    ]


def local_target(source: Path, raw: str) -> Path | None:
    target = raw.strip().strip("<>")
    if not target or target.startswith("#"):
        return None
    if re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target):
        return None
    target = unquote(target.split("#", 1)[0].split("?", 1)[0]).strip()
    if not target or target.startswith("/"):
        return None
    return (source.parent / target).resolve()


def check_links(files: list[Path]) -> list[str]:
    errors: list[str] = []
    for source in files:
        text = source.read_text(encoding="utf-8")
        for raw in LINK_RE.findall(text):
            target = local_target(source, raw)
            if target is None:
                continue
            try:
                target.relative_to(ROOT)
            except ValueError:
                errors.append(f"{source.relative_to(ROOT)}: link escapes repository: {raw}")
                continue
            if not target.exists():
                errors.append(f"{source.relative_to(ROOT)}: missing link target: {raw}")
    return errors


def check_adr_index() -> list[str]:
    decisions = ROOT / "docs" / "decisions"
    index = (decisions / "README.md").read_text(encoding="utf-8")
    errors: list[str] = []
    for adr in sorted(decisions.glob("[0-9][0-9][0-9][0-9]-*.md")):
        if f"({adr.name})" not in index:
            errors.append(f"docs/decisions/README.md: missing ADR index entry: {adr.name}")
    return errors


def check_retired_roots() -> list[str]:
    return [
        f"retired documentation path exists: {path.relative_to(ROOT)}"
        for path in RETIRED_PATHS
        if path.exists()
    ]


def check_retired_references(files: list[Path]) -> list[str]:
    errors: list[str] = []
    code_path_scopes = {
        Path("README.md"),
        Path("AGENTS.md"),
        Path("DESIGN.md"),
        Path("apps/client/README.md"),
        Path("server/README.md"),
        Path("docs/development/agent-debugging.md"),
        Path("docs/deployment/oauth-configuration.md"),
    }
    for source in files:
        rel = source.relative_to(ROOT)
        text = source.read_text(encoding="utf-8")
        for marker in RETIRED_REFERENCES:
            if marker in text:
                errors.append(f"{rel}: references retired documentation path: {marker}")
        if rel.parts[:2] in {("docs", "architecture"), ("docs", "product")} or rel in code_path_scopes:
            for marker in RETIRED_CODE_PATHS:
                if marker in text:
                    errors.append(f"{rel}: references retired code path: {marker}")
    return errors


def main() -> int:
    files = markdown_files()
    errors = []
    errors.extend(check_links(files))
    errors.extend(check_adr_index())
    errors.extend(check_retired_roots())
    errors.extend(check_retired_references(files))
    if errors:
        for error in errors:
            print(f"ERROR: {error}")
        print(f"documentation check failed: {len(errors)} error(s)")
        return 1
    print(f"documentation check passed: {len(files)} Markdown files")
    return 0


if __name__ == "__main__":
    sys.exit(main())
