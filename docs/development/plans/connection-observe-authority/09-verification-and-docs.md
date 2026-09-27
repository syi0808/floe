# 09: Full verification and documentation/ADR convergence

Prerequisite: 08 complete.

This checkpoint proves the final system, aligns durable documentation with implementation and leaves no migration plan status outside this bundle.

## Final architecture acceptance

The implementation must satisfy all of these without exceptions:

1. Connections owns current standing resource scope.
2. Access owns standing Observe grant.
3. Expert binding selects connection/View, not leaf resources.
4. Context resolves current resources and records exact provenance.
5. resource changes advance SourceAuthority only.
6. grant policy changes advance GrantAuthority only.
7. ConsumerPolicyAuthority does not exist.
8. native and remote Calendar use logical calendar.timeline permission.
9. remote Calendar uses generic remote View authorization protocol.
10. Use with Floe is one connection-level product control.
11. third-party Experts receive no default first-party grant.
12. Feasibility/exact-recipient/Act contextual authority remains separate.

## 09-A: clean baseline and residual recheck

Record:

~~~
git status --short --branch
git rev-parse HEAD
git log -1 --oneline
cargo metadata --no-deps --format-version 1
~~~

Repeat every residual selector from 08. No old design production match may be waved through because tests pass.

Inspect actual Cargo manifests against tools/architecture/module-dependencies.json.

## 09-B: focused safety matrix

Run named tests covering:

- 11-resource native Calendar grant/read;
- connection resource update without grant/binding update;
- old dependency invalidated by SourceAuthority;
- grant pause/revoke;
- native subject/generation drift;
- remote producer/source drift;
- generic remote Calendar admit/read/release;
- first-party built-in policy;
- extension denial;
- linked interaction drift/resume;
- exact-recipient processing reauthorization;
- Calendar Actions source/proposal fence.

Record exact command names and results in this document during execution.

## 09-C: full repository verification

Required unless a platform prerequisite is genuinely unavailable:

~~~
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/check_expert_extensibility.py
# any source-semantic checker introduced in 08

CARGO_INCREMENTAL=0 RUST_TEST_THREADS=1 cargo test --workspace --no-fail-fast
cargo check --workspace

(
  cd server
  go test ./...
  go test -race ./...
  go vet ./...
)

cargo build -p floe-ffi

(
  cd apps/client
  flutter analyze
  flutter test
  flutter build macos
)

git diff --check
~~~

Run macOS native Calendar/provider ABI/sign/load fixture gates from the code-change-verification skill. iOS is reported separately according to current project policy; do not claim it passed if not executed.

## 09-D: architecture documentation

Update current documents to the implemented state:

- docs/architecture/modules.md
- docs/architecture/runtime.md
- docs/architecture/authority-recovery.md
- docs/architecture/invariants.md only if a repository-wide invariant truly changed
- docs/product/integrations-and-privacy.md
- relevant server/connector README if protocol shape changed
- DESIGN/product copy docs only where UI semantics changed.

Remove stale statements including:

- first-party consumers derived from exact Expert binding resource;
- per-source consumer-policy authority;
- resource edit while Use with Floe active always requires fresh grant expansion;
- Calendar leaf grant resources;
- separate remote Calendar authority path.

Architecture docs describe current final state, not checkpoint history.

## 09-E: ADR convergence

The durable rationale changes accepted ADR 0027/0028 semantics. Preserve history.

Add a new ADR (next available number, expected 0031 at plan baseline) that amends the relevant portions of:

- ADR 0027 connection authority/observation;
- ADR 0028 connection-scoped permission presentation.

The ADR should record:

- Connection owns current resource scope;
- standing Observe grant binds stable connection/View, not a copied leaf-resource set;
- source scope change advances SourceAuthority and does not by itself mutate the grant;
- Expert binding is connection/View configuration and never permission;
- consumer policy is represented by GrantAuthority + review digest, not ConsumerPolicyAuthority;
- Use with Floe remains Access permission even though resource membership follows the connection;
- provider/source drift fails closed at source reauthorization;
- third-party Experts remain excluded from default first-party consumers.

Update docs/decisions/README.md.

Do not rewrite old ADR text to pretend it always said this.

## 09-F: final code/deletion metrics

Record for the final report:

- files deleted;
- major types/fields/routes removed;
- package dependencies removed;
- tests deleted vs replacement invariant tests;
- remaining explicit exceptions (expected: Feasibility/contextual recipient/Act only, not compatibility).

Counts are evidence, not success criteria. Simpler semantics matter more than net line count.

## Completion report

Report:

1. start/final HEAD and checkpoint commit SHAs;
2. final owner/path summary;
3. obsolete code and storage/wire deleted;
4. exact full verification results;
5. residual search result;
6. architecture and ADR updates;
7. any unavailable platform checks.

Then mark 09 Complete in README.

## Plan retirement

After explicit user acceptance, remove docs/development/plans/connection-observe-authority and the active-plan pointer from docs/README.md in one documentation cleanup commit. Git history remains the archive.
