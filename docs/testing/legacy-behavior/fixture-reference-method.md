> Historical fixture-audit method. It records a prior snapshot, not a current command gate.

# Fixture / resource consumer graph method and limits

Repository: `syi0808/floe`

Commit: `3f4b407f8079d611224cd7adbef121f9e7e75e8e`

## Method

- Checked out the exact detached commit and used its tracked file list as the source boundary.
- Inventory includes tracked paths under case-insensitive `fixtures` and `testdata` directory components, Apple `Tests/Resources` paths, and the tracked `ResponseLoss.swift` shim referenced by the calendar validation documentation. `scope_path_checks.csv` records that the exact `build_test_fixtures/ResponseLoss` path pattern and Apple `Tests/Resources` pattern have no tracked matches; the shim exists at `tools/validation/calendar/ResponseLoss.swift`. Each inventoried file has its existence state, byte count, and SHA-256 digest in `fixture_inventory.csv`.
- Scanned tracked UTF-8 text lines for fixture basenames, explicit path literals, SwiftPM resource declarations, and references to the shared fixture builder. Each graph row preserves the exact source file, line, source expression, mechanically resolved target path (when possible), source kind, and resolution context. `reference_graph.json` mirrors the graph CSV.
- Recorded unresolved and variable-derived paths separately in `unresolved_paths.csv` and `unresolved_paths.json`; path candidates are not guessed.
- Recorded test launcher command occurrences from tracked text in `test_launcher_references.csv`, and explicit manifest plus referenced Cargo/Flutter/Xcode test targets in `test_target_references.csv`. Code, documentation, scripts, and manifests are labeled mechanically by file type.
- `coverage.json` reports the tracked-file/text scan counts, inventory and graph edge counts, unresolved/dynamic rows, target-reference counts, and fixture files with/without statically resolved edges.

## Limits

This is a bounded static text/path audit, not a runtime trace. It does not establish semantic ownership or keep/delete disposition. Dynamic file names, generated build inputs, build-cache outputs, resource APIs without a resolvable bundle path, shell indirection, and paths assembled across control flow may not resolve to one tracked file; those detected cases remain in `unresolved_paths`. A file with no static edge is reported as such, not classified as unused. Test-launcher search is limited to explicit known launcher command spellings and manifest target declarations/references; it does not claim to enumerate every language-level test function.

No source or test files were modified. No formatter, compiler, build, test, installer, or executable architecture checker was run.
