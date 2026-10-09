#!/usr/bin/env python3
"""Check the Go import graph with go list.

The owner-to-adapter scope is deliberately limited to the Authority and Views
packages changed by the first P5 View slice. Existing Integrations, Inference,
Trust, and other P5 adapter debt remains outside this gate. Authority may import
Views-owned contract types under views/contracts; it may not import the Views
application package at internal/views.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path


MODULE = "floe/server"
AUTHORITY = f"{MODULE}/internal/authority"
VIEWS = f"{MODULE}/internal/views"
OWNER_PACKAGES = {AUTHORITY, VIEWS}
COMPOSITION_ROOTS = {f"{MODULE}/internal/node", f"{MODULE}/internal/transport/http"}
CONCRETE_ADAPTER_PREFIXES = (
    f"{MODULE}/internal/connectors",
    f"{MODULE}/internal/storage",
    f"{MODULE}/internal/credentials",
    f"{MODULE}/internal/inference/providers",
    f"{MODULE}/internal/inference/codex",
    f"{MODULE}/internal/modelcatalog",
)
HTTP = f"{MODULE}/internal/transport/http"


def under(path: str, prefix: str) -> bool:
    return path == prefix or path.startswith(prefix + "/")


def go_list_graph(root: Path) -> dict[str, set[str]]:
    result = subprocess.run(
        ["go", "list", "-deps", "-json", "./..."],
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "go list failed")
    decoder = json.JSONDecoder()
    offset = 0
    graph: dict[str, set[str]] = {}
    while offset < len(result.stdout):
        while offset < len(result.stdout) and result.stdout[offset].isspace():
            offset += 1
        if offset == len(result.stdout):
            break
        record, offset = decoder.raw_decode(result.stdout, offset)
        if "Error" in record:
            raise RuntimeError(str(record["Error"]))
        package = record.get("ImportPath")
        if package:
            graph[package] = set(record.get("Imports") or ())
    return graph


def violations(graph: dict[str, set[str]]) -> list[tuple[str, str, str]]:
    found: list[tuple[str, str, str]] = []
    for importer, imports in graph.items():
        if importer in OWNER_PACKAGES:
            for target in imports:
                if any(under(target, prefix) for prefix in CONCRETE_ADAPTER_PREFIXES):
                    found.append(("owner-imports-concrete-adapter", importer, target))
        if importer == AUTHORITY or importer.startswith(AUTHORITY + "/"):
            for target in imports:
                if under(target, VIEWS) and target != f"{VIEWS}/contracts":
                    found.append(("authority-imports-views-application", importer, target))
        if importer.startswith(f"{MODULE}/internal/") and importer not in COMPOSITION_ROOTS:
            for target in imports:
                if under(target, HTTP):
                    found.append(("core-imports-http", importer, target))
    return found


def write_fixture(root: Path, edge: tuple[str, str] | None) -> None:
    (root / "go.mod").write_text(f"module {MODULE}\n\ngo 1.25.0\n", encoding="utf-8")
    files = {
        "internal/contracts/source/source.go": "package source\ntype ID string\n",
        "internal/views/contracts/contracts.go": (
            "package contracts\nimport source \"floe/server/internal/contracts/source\"\n"
            "type ID = source.ID\nvar _ source.ID\n"
        ),
        "internal/views/views.go": (
            "package views\nimport contract \"floe/server/internal/views/contracts\"\n"
            "type Service struct{}\nvar _ contract.ID\n"
        ),
        "internal/authority/authority.go": (
            "package authority\n"
            "import (source \"floe/server/internal/contracts/source\"; "
            "contract \"floe/server/internal/views/contracts\")\n"
            "var _ source.ID\nvar _ contract.ID\n"
        ),
        "internal/trust/trust.go": "package trust\n",
        "internal/transport/http/http.go": "package httptransport\ntype Handler struct{}\n",
        "internal/connectors/fixture/adapter.go": "package fixture\n",
    }
    # Keep the composition fixture valid without needing exported behavior.
    files["internal/authority/authority.go"] += "type Owner struct{}\n"
    files["internal/node/node.go"] = (
        "package node\nimport (\"floe/server/internal/authority\"; "
        "httptransport \"floe/server/internal/transport/http\")\n"
        "var _ authority.Owner\nvar _ httptransport.Handler\n"
    )
    if edge:
        importer, target = edge
        relative = importer.removeprefix(MODULE + "/") + "/fixture.go"
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            f"package {path.parent.name}\nimport _ \"{target}\"\n",
            encoding="utf-8",
        )
    for relative, contents in files.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents, encoding="utf-8")


def run_fixtures() -> None:
    with tempfile.TemporaryDirectory(prefix="floe-go-import-graph-") as temp:
        base = Path(temp)
        with tempfile.TemporaryDirectory(dir=base) as positive:
            positive_root = Path(positive)
            write_fixture(positive_root, None)
            if violations(go_list_graph(positive_root)):
                raise RuntimeError("positive import-graph fixture was rejected")

        forbidden = (
            (VIEWS, f"{MODULE}/internal/connectors/fixture", "owner-imports-concrete-adapter"),
            (AUTHORITY, VIEWS, "authority-imports-views-application"),
            (f"{MODULE}/internal/trust", HTTP, "core-imports-http"),
        )
        for index, (importer, target, expected_rule) in enumerate(forbidden):
            with tempfile.TemporaryDirectory(dir=base, prefix=f"negative-{index}-") as fixture:
                fixture_root = Path(fixture)
                write_fixture(fixture_root, (importer, target))
                found = violations(go_list_graph(fixture_root))
                if not any(rule == expected_rule for rule, _, _ in found):
                    raise RuntimeError(f"forbidden fixture did not trigger {expected_rule}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixtures-only", action="store_true", help="run positive and forbidden graph fixtures only")
    args = parser.parse_args()
    try:
        run_fixtures()
        if not args.fixtures_only:
            graph = go_list_graph(Path(__file__).resolve().parents[1])
            found = violations(graph)
            if found:
                for rule, importer, target in found:
                    print(f"{rule}: {importer} imports {target}", file=sys.stderr)
                return 1
    except Exception as error:  # noqa: BLE001 - report a concise gate failure
        print(f"Go import-graph gate failed: {error}", file=sys.stderr)
        return 1
    scope = "fixture set" if args.fixtures_only else "server graph"
    print(f"Go import-graph gate passed ({scope}; positive fixture and 3 forbidden fixtures).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
