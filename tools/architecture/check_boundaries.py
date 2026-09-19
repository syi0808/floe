#!/usr/bin/env python3
"""Check the approved Rust internal production dependency DAG.

The checker is deliberately read-only.  Normal, build, and target-specific
production dependency tables participate in the graph; dev-only wiring is
reported separately and never participates in the production gate.
"""
from __future__ import annotations

import argparse
import fnmatch
import json
import sys
import tomllib
from pathlib import Path


def graph_errors(
    graph: dict[str, set[str]], forbidden: list[tuple[str, str]]
) -> list[str]:
    """Return cycle and transitive forbidden-edge errors for ``graph``."""
    errors: list[str] = []
    marks: dict[str, int] = {}
    stack: list[str] = []

    def visit(node: str) -> None:
        if marks.get(node) == 1:
            errors.append("cycle: " + " -> ".join(stack[stack.index(node) :] + [node]))
            return
        if marks.get(node) == 2:
            return
        marks[node] = 1
        stack.append(node)
        for dependency in graph.get(node, ()):
            visit(dependency)
        stack.pop()
        marks[node] = 2

    for node in graph:
        visit(node)

    for source, destination in forbidden:
        pending: list[tuple[str, list[str]]] = [(source, [source])]
        seen: set[str] = set()
        while pending:
            node, path = pending.pop()
            if node in seen:
                continue
            seen.add(node)
            if node == destination and node != source:
                errors.append("forbidden path: " + " -> ".join(path))
                break
            for dependency in graph.get(node, ()):
                pending.append((dependency, path + [dependency]))
    return errors


def _dependency_tables(doc: dict, key: str) -> list[dict]:
    tables = [doc.get(key, {})]
    for target in doc.get("target", {}).values():
        tables.append(target.get(key, {}))
    return tables


def _resolve_dependency(alias: str, spec: object, workspace: dict) -> str:
    if isinstance(spec, dict) and spec.get("workspace"):
        base = workspace.get(alias, {})
        if isinstance(base, dict):
            spec = {**base, **spec}
    return spec.get("package", alias) if isinstance(spec, dict) else alias


def dependency_names(doc: dict, workspace: dict, key: str) -> list[str]:
    """Resolve package names from all root and target tables named ``key``."""
    names: list[str] = []
    for table in _dependency_tables(doc, key):
        for alias, spec in table.items():
            names.append(_resolve_dependency(alias, spec, workspace))
    return names


def production_deps(doc: dict, workspace: dict) -> list[str]:
    """Resolve normal, build, and target-specific production dependencies."""
    tables = [doc.get("dependencies", {}), doc.get("build-dependencies", {})]
    for target in doc.get("target", {}).values():
        tables.extend(
            [target.get("dependencies", {}), target.get("build-dependencies", {})]
        )
    return [
        _resolve_dependency(alias, spec, workspace)
        for table in tables
        for alias, spec in table.items()
    ]


def dev_deps(doc: dict, workspace: dict) -> list[str]:
    """Resolve dev-only wiring without treating it as production architecture."""
    return dependency_names(doc, workspace, "dev-dependencies")


def _policy_graph(policy: dict) -> tuple[dict[str, set[str]], list[str]]:
    entries = policy["target"]
    by_id = {entry["id"]: entry for entry in entries}
    graph: dict[str, set[str]] = {}
    errors: list[str] = []
    for entry in entries:
        package = entry["package"]
        if package in graph:
            errors.append("duplicate target package: " + package)
        dependencies: set[str] = set()
        for dependency_id in entry["allowed_internal_dependencies"]:
            dependency = by_id.get(dependency_id)
            if dependency is None:
                errors.append(f"{package}: unknown policy dependency {dependency_id}")
            else:
                dependencies.add(dependency["package"])
        graph[package] = dependencies
    return graph, errors


def _forbidden_edges(policy: dict) -> list[tuple[str, str]]:
    entries = policy["target"]
    generic = [
        entry["package"]
        for entry in entries
        if entry["group"] in ("runtime", "modules")
    ]
    forbidden = [(package, "floe-experts-builtin") for package in generic]
    forbidden += [
        ("floe-inference", "floe-connections"),
        ("floe-connections", "floe-inference"),
    ]
    forbidden += [
        (entry["package"], dependency)
        for entry in entries
        if entry["group"] == "modules"
        for dependency in ("floe-vault", "floe-provider-adapters", "floe-ffi", "floe-app")
    ]
    return forbidden


def _load_manifests(repo: Path) -> tuple[dict, dict[str, dict], dict[str, str]]:
    root = tomllib.loads((repo / "Cargo.toml").read_text(encoding="utf-8"))
    manifests: dict[str, dict] = {}
    paths: dict[str, str] = {}
    crates = repo / "crates"
    if not crates.is_dir():
        return root, manifests, paths
    for path in crates.rglob("Cargo.toml"):
        document = tomllib.loads(path.read_text(encoding="utf-8"))
        if "package" not in document:
            continue
        name = document["package"]["name"]
        if name in manifests:
            raise ValueError("duplicate package: " + name)
        manifests[name] = document
        paths[name] = str(path.parent.relative_to(repo))
    return root, manifests, paths


def _workspace_member(path: str, root: dict) -> bool:
    workspace = root.get("workspace", {})
    members = workspace.get("members", [])
    excludes = workspace.get("exclude", [])
    normalized = path.removeprefix("./")
    return any(
        fnmatch.fnmatch(normalized, pattern.removeprefix("./"))
        and not any(
            fnmatch.fnmatch(normalized, excluded.removeprefix("./"))
            for excluded in excludes
        )
        for pattern in members
    )


def _check_repository(policy: dict, repo: Path) -> tuple[dict[str, set[str]], list[str], list[str], dict[str, list[str]]]:
    entries = policy["target"]
    expected = {entry["package"]: entry for entry in entries}
    allowed, errors = _policy_graph(policy)
    warnings: list[str] = []
    dev_wiring: dict[str, list[str]] = {}
    root, manifests, paths = _load_manifests(repo)
    workspace = root.get("workspace", {}).get("dependencies", {})

    for name in set(expected) - set(manifests):
        errors.append("missing target crate: " + name)
    for name in set(manifests) - set(expected):
        errors.append("unexpected/legacy crate: " + name)

    graph: dict[str, set[str]] = {}
    legacy = {
        name
        for name in manifests
        if name not in expected or paths[name] != expected[name]["path"]
    }
    for name, document in manifests.items():
        graph[name] = {
            dependency
            for dependency in production_deps(document, workspace)
            if dependency in manifests or dependency.startswith("floe-")
        }
        dev_names = sorted(set(dev_deps(document, workspace)))
        if dev_names:
            dev_wiring[name] = dev_names
        if name not in expected:
            continue
        relocated = paths[name] == expected[name]["path"]
        if not relocated:
            errors.append(f"{name}: wrong directory {paths[name]}")
        elif not _workspace_member(paths[name], root):
            errors.append(f"{name}: target crate is not a workspace member")
        for dependency in graph[name] - allowed[name]:
            errors.append(f"{name}: disallowed direct dependency {dependency}")
        if relocated:
            for dependency in graph[name] & legacy:
                errors.append(
                    f"{name}: target crate depends on unmigrated/legacy crate {dependency}"
                )
    errors += graph_errors(graph, _forbidden_edges(policy))
    return graph, errors, warnings, dev_wiring


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "repo",
        type=Path,
        nargs="?",
        default=Path(__file__).resolve().parents[2],
        help="checkout root (default: repository containing this tool)",
    )
    parser.add_argument(
        "--policy",
        type=Path,
        default=Path(__file__).resolve().with_name("module-dependencies.json"),
    )
    parser.add_argument("--policy-only", action="store_true")
    parser.add_argument("--json-out", type=Path)
    args = parser.parse_args()
    try:
        policy = json.loads(args.policy.read_text(encoding="utf-8"))
        graph, errors = _policy_graph(policy)
        warnings: list[str] = []
        dev_wiring: dict[str, list[str]] = {}
        if args.policy_only:
            errors += graph_errors(graph, _forbidden_edges(policy))
            report_mode = "policy-only"
        else:
            graph, errors, warnings, dev_wiring = _check_repository(
                policy, args.repo.resolve()
            )
            report_mode = "final"
        report = {
            "mode": report_mode,
            "nodes": len(graph),
            "edges": sum(map(len, graph.values())),
            "errors": errors,
            "warnings": warnings,
            "dev_wiring": dev_wiring,
            "scope": (
                "internal normal + build dependencies, including all declared target tables; "
                "dev-only wiring is reported separately; excludes source-level semantic checks"
            ),
        }
        if args.json_out:
            args.json_out.parent.mkdir(parents=True, exist_ok=True)
            args.json_out.write_text(
                json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
            )
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 1 if errors else 0
    except (OSError, KeyError, ValueError, tomllib.TOMLDecodeError) as exc:
        print("Cannot check architecture: " + str(exc), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
