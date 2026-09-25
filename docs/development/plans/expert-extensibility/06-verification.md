# 06: final verification and documentation convergence

Prerequisite: 05 complete. This document specifies future implementation gates. Writing the plan does not mean any code gate below has run. Overall checkpoint status belongs only in [README.md](README.md).

## Verification by changed surface

Follow the [repository verification skill](../../../../.agents/skills/code-change-verification/SKILL.md). Run nearest owner tests first, then full required gates. Record command, source HEAD, environment/prerequisite, observed result and relevant counts only when actually observed.

### Rust, contracts, persistence and dependency graph

From repository root:

```sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/test_check_boundaries.py
git diff --check
```

Confirm the architecture test invocation against its current script before use. Run added checker regression tests too. Format changed Rust files without unrelated workspace churn. Baseline failures are compared with 00 evidence; they are not silently called introduced or silently ignored.

### App, protocol, FFI and Flutter

```sh
cargo build -p floe-ffi
cd apps/client
flutter analyze
flutter test
flutter build macos
```

Use current component README commands for generation and bundled native artifacts. Verify generated DTO/localization/fixture output matches source. Flutter, bindings and bundled dylib must come from the same tested snapshot. A Rust-only pass is not proof of a changed Dart parser, settings command or native bundle.

Run the real serializer -> tracked fixture -> Dart parser tests introduced in 01 and ported in 02/04. Independently hand-written copies do not prove wire compatibility. Verify actual UI routes for generic Expert settings and recovery, not just isolated widgets.

### Native/macOS boundary

Read the nearest current README and invoke the relevant native fixture/validation scripts for changed EventKit/Contacts/Health/attention/Keychain paths. Include bundle ABI/sign/load and product conversation smoke when affected. Use existing fixtures for owner identity and source generation; do not replace them with a fake success solely to run on another OS.

macOS acceptance is required here. Missing native fixture, signing/toolchain or OS capability is `NOT_RUN`/`UNVERIFIED` with the exact reason, not a passing test or silent early return. iOS simulator/device is explicitly deferred to its implementation task; Android is out of scope. Do not install platforms, pair devices or change signing/account state without authorization.

### Go and source protocol

When Go behavior or Rust-Go source protocol/fixtures change:

```sh
cd server
go test -race ./...
go vet ./...
```

Format changed Go code. Even if the Go implementation does not change, exercise relevant end-to-end source admission fixtures when removing old Rust routes. Live-provider calls require configured, authorized test accounts; record unavailable prerequisites rather than guessing success. Do not change external data as a read verification shortcut.

### Documentation

From a full checkout after implementation:

```sh
python3 tools/docs/check_docs.py
git diff --check
```

Additionally inspect actual relative links/anchors and plan cross-references; a checker is not proof of semantic correctness. If a documentation-only environment has only retrieved files, validate and report that narrower set explicitly, never label it a full-repository check.

## Required end-to-end matrix

| Scenario | Acceptance evidence |
|---|---|
| Product Gmail bundle has mail and logistics grants on one connection | Both Views read independently; no false duplicate or wrong-View blocker. |
| Other Views/native grants coexist | Only selected compatible targets participate. |
| Manager direct read and Expert read on the same source | Each exact actual consumer is checked; product-issued grants and recovery agree. |
| Schedule one-call, summary-only and multiple-read result | Current backend output survives Task persistence, wire conversion and generic client display. |
| New statically supplied Expert reuses existing capability | No common production edit for package addition; normal registration, settings, delegation and result path. |
| A selected, B unselected | Add/enable/pause/revoke B without changing A's read scope or producing unrelated blockers. |
| A unavailable, B available | No automatic fallback; truthful typed limitation and setup/review path. |
| Selection/manifest/consumer set changes during review | Old review cannot widen or retarget authority; exact fresh review required. |
| Selection/grant changes during acquisition/model handoff/release | Owner fences block later use; already transmitted bytes are not claimed recalled. |
| Unrelated assignment update | Does not invalidate the current Task via global revision. |
| Budget Continue and crash replay | Same validated batch/selection identity; current source authority rechecked before use/release. |
| Linked resume after setup/review | Fresh admission, exact originating requirement satisfied, no false resolution loop. |
| Restart or response loss during binding/grant mutation | Exact-command rejoin; no duplicate mutation or automatic profile reset. |
| Task state/result settlement fails midway | Atomic rollback or owner-defined durable recovery; no half-visible generic result. |
| Action proposal from multiple-source reasoning | Exact contributor/target, reviewed effect and durable pre-dispatch intent; no blind retry after uncertain write. |
| Forged IDs, artifacts or authority fields | Person/assignment isolation, untrusted model requirement rejection, bounded safe output, no secret exposure. |

Each scenario needs an executable test or a clearly recorded manual/product verification with actual evidence. State the real boundaries versus test doubles. Do not label every owner independently tested as an end-to-end pass.

## Safety assertions that must survive deletion

Maintain exact-recipient consent and lineage; source/consumer/category/purpose identity; grant/source/policy revision checks; producer and native subject identity; per-contributor provenance and coverage; unknown/unavailable versus empty distinction; CAS; durable intent; cancellation direction; Task/Expert settlement; irreversible external-effect uncertainty and lookup-only reconciliation.

Deleting a stale test may remove an obsolete API assumption. It may not remove these assertions without relocating equivalent executable coverage. Declining test counts are not inherently regressions; missing semantic coverage is.

## Final source and artifact audit

Re-run 05's residual searches on the actual final HEAD, including production, inline tests, fixtures, generated output and documentation. Examine public exports, Cargo edges, wire variants, stored fields and old endpoint success mocks, not just filenames. Verify:

- one generic registration/delegation path;
- no source permission restored to Registry;
- one immutable selection per admitted invocation and no fallback source discovery;
- no Schedule-shaped common result/settlement/client parser;
- no obsolete source transport API needed solely by a fake test;
- no migration-only nullable state, dual decoder or standing authorization flag;
- existing protocol version numbers are not bumped just to retain obsolete local compatibility;
- authority epochs and legitimate runtime/executor generations still advance normally.

Use an explicitly selected isolated development profile if stored/wire meaning changed. Never auto-delete data on an open error, replace keys opportunistically or discard uncertain effect records. A fixed version number does not imply compatibility with an old binary/profile.

## Documentation convergence

Update current `docs/architecture/modules.md`, `runtime.md`, `authority-recovery.md` and invariants only where actual implemented ownership/path changes. Adjust `tools/architecture/module-dependencies.json` with actual intended allowed edges, not a second manually copied graph.

Amend/add/supersede an ADR only when the durable rationale changes. Inspect ADR 0018 for package/delegation intent and ADR 0030 for durable interaction rationale; do not turn either into checkpoint status. Document why configuration binding is distinct from current read/processing authority and why new installs cannot expand old reviews. Update the relevant current product/design docs for the shipped Expert settings surface; avoid progress prose in normative UI/architecture docs.

Update the README checkpoint evidence to actual commits and commands. No implementation verification is attributed to the original plan-writing commit. After user acceptance, remove the temporary plan and its index pointer as described by its lifecycle; Git history preserves it.

## Completion report

Report baseline and final HEAD; changed owners/contracts/canonical paths; migrated callers; code and tests deleted; safety assertions relocated with executable names; conformance experiment evidence; exact verification results and unavailable surfaces; residual exceptions; updated architecture/product/ADR docs; and any remaining blocker.

Mark 06 complete only when required macOS/product and owner gates are satisfied, or when the user explicitly accepts a specific documented exception. Never silently replace a required broad gate with a narrow unit test. Do not claim full validation from static inspection or plan creation.
