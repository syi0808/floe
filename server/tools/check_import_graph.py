#!/usr/bin/env python3
"""Check Go owner, adapter, composition, and HTTP import boundaries."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tempfile
from pathlib import Path


MODULE = "floe/server"
INTERNAL = f"{MODULE}/internal"
AUTHORITY = f"{INTERNAL}/authority"
VIEWS = f"{INTERNAL}/views"
VIEWS_CONTRACTS = f"{VIEWS}/contracts"
HTTP = f"{INTERNAL}/transport/http"
COMPOSITION_ROOTS = (f"{INTERNAL}/node",)
OWNER_ROOTS = tuple(
    f"{INTERNAL}/{name}"
    for name in (
        "authority",
        "contracts",
        "inference",
        "integrations",
        "operation",
        "pairing",
        "trust",
        "views",
    )
)
CONCRETE_ADAPTER_PREFIXES = (
    f"{INTERNAL}/adapters",
    # The operator-facing model catalog remains its existing adapter surface;
    # metadata/capability ownership is a separate P5 slice.
    f"{INTERNAL}/modelcatalog",
)


def under(path: str, prefix: str) -> bool:
    return path == prefix or path.startswith(prefix + "/")


def is_owner(path: str) -> bool:
    return any(under(path, prefix) for prefix in OWNER_ROOTS)


def is_composition_root(path: str) -> bool:
    return any(under(path, prefix) for prefix in COMPOSITION_ROOTS)


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


def graph_cycle(graph: dict[str, set[str]]) -> list[str] | None:
    state: dict[str, int] = {}
    stack: list[str] = []

    def visit(package: str) -> list[str] | None:
        state[package] = 1
        stack.append(package)
        for dependency in graph.get(package, ()):
            if dependency not in graph:
                continue
            if state.get(dependency, 0) == 0:
                cycle = visit(dependency)
                if cycle:
                    return cycle
            elif state.get(dependency) == 1:
                start = stack.index(dependency)
                return stack[start:] + [dependency]
        stack.pop()
        state[package] = 2
        return None

    for package in graph:
        if state.get(package, 0) == 0:
            cycle = visit(package)
            if cycle:
                return cycle
    return None


def violations(graph: dict[str, set[str]]) -> list[tuple[str, str, str]]:
    found: list[tuple[str, str, str]] = []
    for importer, imports in graph.items():
        if is_owner(importer):
            for target in imports:
                if any(under(target, prefix) for prefix in CONCRETE_ADAPTER_PREFIXES):
                    found.append(("owner-imports-concrete-adapter", importer, target))
        if under(importer, AUTHORITY):
            for target in imports:
                if under(target, VIEWS) and not under(target, VIEWS_CONTRACTS):
                    found.append(("authority-imports-views-application", importer, target))
        if under(importer, INTERNAL) and not is_composition_root(importer):
            for target in imports:
                if under(target, HTTP):
                    found.append(("core-imports-http", importer, target))
    cycle = graph_cycle(graph)
    if cycle:
        found.append(("internal-import-cycle", cycle[0], " -> ".join(cycle)))
    return found


def matching_paren(source: str, opening: int) -> int | None:
    depth = 0
    quote: str | None = None
    escaped = False
    for index in range(opening, len(source)):
        character = source[index]
        if quote:
            if escaped:
                escaped = False
            elif character == "\\":
                escaped = True
            elif character == quote:
                quote = None
            continue
        if character in ('"', "'", '`'):
            quote = character
        elif character == "(":
            depth += 1
        elif character == ")":
            depth -= 1
            if depth == 0:
                return index
    return None


def source_contract_violations(root: Path) -> list[tuple[str, str, str]]:
    """Guard owner result boundaries in addition to the Go import graph."""
    found: list[tuple[str, str, str]] = []
    function_start = re.compile(
        r"(?m)^[ \t]*func[ \t]+(?:\([^()\n]*\)[ \t]*)?([A-Za-z_]\w*)[ \t]*\("
    )
    dynamic_schema_maps = {
        "internal/integrations/definitions.go:ValidatedConnectorScope",
        "internal/integrations/definitions.go:CloneConnectorScope",
    }
    for path in sorted((root / "internal").rglob("*.go")):
        if path.name.endswith("_test.go"):
            continue
        relative = path.relative_to(root).as_posix()
        source = path.read_text(encoding="utf-8")
        if re.search(r"\boperation\.(?:Result|Accept)\b", source):
            found.append(("generic-operation-result", relative, "references operation.Result or operation.Accept"))
        if relative.startswith("internal/operation/"):
            if re.search(r"(?m)^\s*type\s+Result\s+struct\b", source):
                found.append(("generic-operation-result", relative, "declares operation.Result"))
            if re.search(r"(?m)^\s*Value\s+any\b", source):
                found.append(("generic-operation-result", relative, "declares an any success payload"))
            if re.search(r"(?m)^\s*func\s+Accept\s*\(", source):
                found.append(("generic-operation-result", relative, "declares operation.Accept"))

        owner_response_map = relative.startswith(
            ("internal/trust/", "internal/pairing/", "internal/integrations/", "internal/inference/")
        )
        if not owner_response_map:
            continue
        for match in function_start.finditer(source):
            name = match.group(1)
            opening = match.end() - 1
            closing = matching_paren(source, opening)
            if closing is None:
                continue
            body = source.find("{", closing + 1)
            if body < 0:
                continue
            result_type = source[closing + 1 : body]
            if re.search(r"\bmap\s*\[\s*string\s*\]\s*(?:any|interface\s*\{\s*\})", result_type):
                if f"{relative}:{name}" not in dynamic_schema_maps:
                    found.append(("owner-response-map", relative, f"{name} returns a dynamic map"))
    return found


def fixture_files() -> dict[str, str]:
    return {
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
            "var _ source.ID\nvar _ contract.ID\ntype Owner struct{}\n"
        ),
        "internal/contracts/contracts.go": "package contracts\ntype Value struct{}\n",
        "internal/trust/trust.go": "package trust\ntype Principal struct{}\n",
        "internal/integrations/integrations.go": (
            "package integrations\nimport \"floe/server/internal/trust\"\n"
            "type Descriptor struct{}\nvar _ trust.Principal\n"
        ),
        "internal/integrations/definitions.go": (
            "package integrations\n"
            "func ValidatedConnectorScope(scope map[string]any) (map[string]any, error) { return scope, nil }\n"
            "func CloneConnectorScope(scope map[string]any) map[string]any { return scope }\n"
        ),
        "internal/inference/inference.go": (
            "package inference\nimport \"floe/server/internal/trust\"\n"
            "type Owner struct{}\nvar _ trust.Principal\n"
        ),
        "internal/operation/operation.go": "package operation\ntype ID string\n",
        "internal/pairing/pairing.go": (
            "package pairing\nimport \"floe/server/internal/trust\"\nvar _ trust.Principal\n"
        ),
        "internal/transport/http/http.go": "package httptransport\ntype Handler struct{}\n",
        "internal/adapters/integrations/fixture/adapter.go": (
            "package fixture\nimport \"floe/server/internal/integrations\"\n"
            "type Runtime struct{}\nvar _ integrations.Descriptor\n"
        ),
        "internal/adapters/integrations/forbidden/adapter.go": "package forbidden\n",
        "internal/adapters/oauth/fixture/adapter.go": "package fixture\n",
        "internal/adapters/models/providers/provider.go": "package providers\n",
        "internal/adapters/models/codex/codex.go": "package codex\n",
        "internal/adapters/storage/privatefiles/files.go": "package privatefiles\n",
        "internal/adapters/storage/repository.go": (
            "package storage\nimport (\"floe/server/internal/trust\"; "
            "\"floe/server/internal/integrations\"; \"floe/server/internal/inference\")\n"
            "var _ trust.Principal\nvar _ integrations.Descriptor\nvar _ inference.Owner\n"
        ),
        "internal/adapters/credentials/repository.go": (
            "package credentials\nimport \"floe/server/internal/integrations\"\n"
            "var _ integrations.Descriptor\n"
        ),
        "internal/node/node.go": (
            "package node\nimport (\"floe/server/internal/authority\"; "
            "httptransport \"floe/server/internal/transport/http\"; "
            "\"floe/server/internal/adapters/integrations/fixture\")\n"
            "var _ authority.Owner\nvar _ httptransport.Handler\nvar _ fixture.Runtime\n"
        ),
    }


def write_fixture(root: Path, edges: tuple[tuple[str, str], ...] = ()) -> None:
    (root / "go.mod").write_text(f"module {MODULE}\n\ngo 1.25.0\n", encoding="utf-8")
    for relative, contents in fixture_files().items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents, encoding="utf-8")
    for index, (importer, target) in enumerate(edges):
        relative = importer.removeprefix(MODULE + "/") + f"/fixture_{index}.go"
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        package_name = path.parent.name
        for existing in path.parent.glob("*.go"):
            first_line = existing.read_text(encoding="utf-8").splitlines()[:1]
            if first_line and first_line[0].startswith("package "):
                package_name = first_line[0].removeprefix("package ").strip()
                break
        path.write_text(
            f'package {package_name}\nimport _ "{target}"\n',
            encoding="utf-8",
        )


def run_cycle_fixture(root: Path) -> None:
    cycle = (
        (f"{INTERNAL}/trust", f"{INTERNAL}/adapters/credentials"),
        (f"{INTERNAL}/adapters/credentials", f"{INTERNAL}/trust"),
    )
    write_fixture(root, cycle)
    result = subprocess.run(
        ["go", "list", "-deps", "-json", "./..."],
        cwd=root,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    diagnostics = result.stdout + result.stderr
    if result.returncode == 0 or "import cycle" not in diagnostics:
        raise RuntimeError("adapter-to-owner cycle fixture was not rejected by go list")


def run_fixtures() -> int:
    with tempfile.TemporaryDirectory(prefix="floe-go-import-graph-") as temp:
        base = Path(temp)
        with tempfile.TemporaryDirectory(dir=base, prefix="positive-") as positive:
            positive_root = Path(positive)
            write_fixture(positive_root)
            positive_graph = go_list_graph(positive_root)
            if violations(positive_graph):
                raise RuntimeError("positive owner-contract and adapter-to-owner fixture was rejected")
            if source_contract_violations(positive_root):
                raise RuntimeError("positive typed-result and dynamic-scope source fixture was rejected")

        forbidden = [
            (f"{INTERNAL}/authority", f"{INTERNAL}/adapters/storage", "owner-imports-concrete-adapter"),
            (f"{INTERNAL}/contracts", f"{INTERNAL}/adapters/models/providers", "owner-imports-concrete-adapter"),
            (f"{INTERNAL}/integrations", f"{INTERNAL}/adapters/integrations/forbidden", "owner-imports-concrete-adapter"),
            (f"{INTERNAL}/inference", f"{INTERNAL}/adapters/models/providers", "owner-imports-concrete-adapter"),
            (f"{INTERNAL}/operation", f"{INTERNAL}/adapters/oauth/fixture", "owner-imports-concrete-adapter"),
            (f"{INTERNAL}/pairing", f"{INTERNAL}/adapters/credentials", "owner-imports-concrete-adapter"),
            (f"{INTERNAL}/trust", f"{INTERNAL}/adapters/storage/privatefiles", "owner-imports-concrete-adapter"),
            (f"{INTERNAL}/views", f"{INTERNAL}/adapters/integrations/forbidden", "owner-imports-concrete-adapter"),
            (VIEWS_CONTRACTS, f"{INTERNAL}/adapters/storage/privatefiles", "owner-imports-concrete-adapter"),
            (f"{INTERNAL}/inference/nested/contracts", f"{INTERNAL}/adapters/models/codex", "owner-imports-concrete-adapter"),
            (AUTHORITY, VIEWS, "authority-imports-views-application"),
            (f"{INTERNAL}/trust", HTTP, "core-imports-http"),
        ]
        for index, (importer, target, expected_rule) in enumerate(forbidden):
            with tempfile.TemporaryDirectory(dir=base, prefix=f"negative-{index}-") as fixture:
                fixture_root = Path(fixture)
                write_fixture(fixture_root, ((importer, target),))
                found = violations(go_list_graph(fixture_root))
                if not any(rule == expected_rule for rule, _, _ in found):
                    raise RuntimeError(f"forbidden fixture did not trigger {expected_rule}: {importer} -> {target}")
        with tempfile.TemporaryDirectory(dir=base, prefix="cycle-") as cycle:
            run_cycle_fixture(Path(cycle))
        source_forbidden = [
            (
                "internal/operation/result.go",
                "package operation\ntype Result struct { Value any }\nfunc Accept(value any) Result { return Result{Value: value} }\n",
                "generic-operation-result",
            ),
            (
                "internal/pairing/pairing.go",
                "package pairing\nimport operation \"floe/server/internal/operation\"\nfunc (s *Operations) Execute() (operation.Result, error) { return operation.Result{}, nil }\n",
                "generic-operation-result",
            ),
            (
                "internal/integrations/results.go",
                "package integrations\nfunc catalogResponse() map[string]any { return nil }\n",
                "owner-response-map",
            ),
        ]
        for index, (relative, contents, expected_rule) in enumerate(source_forbidden):
            with tempfile.TemporaryDirectory(dir=base, prefix=f"source-negative-{index}-") as fixture:
                fixture_root = Path(fixture)
                write_fixture(fixture_root)
                source_path = fixture_root / relative
                source_path.parent.mkdir(parents=True, exist_ok=True)
                source_path.write_text(contents, encoding="utf-8")
                found = source_contract_violations(fixture_root)
                if not any(rule == expected_rule for rule, _, _ in found):
                    raise RuntimeError(f"forbidden source fixture did not trigger {expected_rule}: {relative}")
    return len(forbidden) + len(source_forbidden) + 1


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixtures-only", action="store_true", help="run positive and forbidden graph fixtures only")
    args = parser.parse_args()
    try:
        negative_fixtures = run_fixtures()
        if not args.fixtures_only:
            server_root = Path(__file__).resolve().parents[1]
            graph = go_list_graph(server_root)
            found = violations(graph)
            if found:
                for rule, importer, target in found:
                    print(f"{rule}: {importer} imports {target}", file=sys.stderr)
                return 1
            source_found = source_contract_violations(server_root)
            if source_found:
                for rule, path, detail in source_found:
                    print(f"{rule}: {path}: {detail}", file=sys.stderr)
                return 1
    except Exception as error:  # noqa: BLE001 - report a concise gate failure
        print(f"Go import-graph gate failed: {error}", file=sys.stderr)
        return 1
    scope = "fixture set" if args.fixtures_only else "server graph"
    owners = ", ".join(prefix.rsplit("/", 1)[-1] for prefix in OWNER_ROOTS)
    print(
        f"Go import-graph gate passed ({scope}; owner roots: {owners} and nested packages; "
        f"positive adapter-to-owner and typed-result fixtures and {negative_fixtures} negative fixtures)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
