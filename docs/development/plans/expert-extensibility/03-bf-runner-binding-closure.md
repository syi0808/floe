# 03-B/F closure: bind supplied runners to product endpoints

## Authority and scope

This is the active residual execution plan within [checkpoint 03](03-registry-and-runtime.md), not a new checkpoint or a second migration. [README.md](README.md) alone owns overall status. The original 03 plan remains the source for already implemented invariants; this document narrows the remaining work after review of `683b118b00eafb0d931c0a5f5701d16d978d96ba`.

The plan-authoring source baseline is exactly that commit. Recheck actual HEAD and relevant source deltas before editing. The findings below are source-inspection findings, not newly executed failing tests. The implementation agent must add permanent executable evidence through the product endpoint.

Execute only BF-0 through BF-5 below. Do not rerun the original 03-A through 03-G as another redesign. Preserve the existing generic Registry/install receipts, atomic Directory publication, internal Task admission identity, pinned endpoint, assignment-local settlement, source host and Actions/Conversation safety boundaries.

Current `AGENTS.md` takes precedence over older plan text about compatibility/schema changes. This residual changes in-memory registration wiring, tests, and removal of a stale Dart target. It does not require a new persistence format, protocol version, schema bump, new crate or dependency edge. Do not reset a profile or introduce old/new decoders.

**Operator-held golden:** leave the commented assertion in `apps/client/test/features/actions/agent_proposal_card_test.dart` and `apps/client/test/goldens/agent_proposal_card.png` unchanged. Do not re-enable, delete or update that golden, and do not count it as passed.

**Deferred work:** checkpoint 04 source binding remains Not started. The pre-existing `ConfirmedInteractions` unavailable-to-empty-Ready behavior is not fixed by this residual; retain it as an explicit 04 outcome-contract follow-up, not as new successful-source evidence. No Calendar/remote source-selection redesign, dynamic loader, downloaded code, installation marketplace or external-account change belongs here.

## Confirmed gap at the source baseline

| Anchor | Observed implementation | Residual obligation |
|---|---|---|
| `crates/experts/builtin/src/registration.rs::{registrations,runner,manifests}` | `registrations<Host>()` already returns `ExpertRegistration<ExpertRun<Host,...>>`; a separate manifest projection exists. | Carry the supplied implementation with its manifest through product composition instead of discarding it before publication. |
| `crates/modules/experts/src/dispatch.rs::ExpertRun` | The callable is generic over a host type and a borrowed invocation lifetime. | Preserve short-lived host/read lifetimes when making the callable storable; do not manufacture a `'static` host. |
| `crates/app/src/vault_host.rs::OpenVault::publish_expert_directory` | Reads shipped `manifests()`, joins installed package identity, constructs `RegisteredExpertEndpoint` with admission and manifest only. | Join against the supplied registration collection and bind the complete selected registration to the endpoint. |
| `conversation_turn/expert_dispatch.rs::RegisteredExpertEndpoint::{new,execute}` | Holds admission and manifest, but no runner. `execute` rebuilds `shipped_bundle_dispatch()` for membership. | Store and invoke the selected implementation directly; no execution-time shipped-catalog membership lookup. |
| `ConversationExperts::execute_builtin` in the same file | Builds the real source/model/blocker host, then calls `shipped_bundle_dispatch().run(agent_id, ...)`. | Keep this common execution work, but call the already bound runner. |
| `ConversationExperts::handle_message` | Retains an A2A fallback that retrieves a shipped manifest by package ID. | Migrate necessary callers to explicit supplied/bound registrations or the canonical Task path; no silent shipped fallback. |
| `crates/modules/experts/tests/delegation.rs::nonbuiltin_registration_installs_publishes_and_completes_without_source_binding` | Installs a manifest but supplies a bare test `AgentEndpoint` directly to Directory. | Retain this owner-level test; add an App test using the real `RegisteredExpertEndpoint` and host. It is not product-endpoint coverage by itself. |
| `apps/client/lib/features/experts/domain/agent_registry.dart::AgentRegistryTarget` | `calendarView` / `calendar_view` remains despite the Rust target exposing only installation and assignment. | Delete the stale client target without restoring a Calendar Registry command. |

This is not a claim that the current Directory pin is broken: `TaskCoordinator` already resolves the endpoint once. The gap is that the pinned product endpoint internally rediscovers the executable from the shipped catalog instead of retaining the supplied registration's executable.

## Final contract and ownership

### One supplied registration survives the full path

Use the existing Experts-owned `ExpertRegistration<R>` and manifest/admission values. App composes concrete services; the package/bundle supplies the executable. The target path is:

```text
composition-supplied immutable registrations
  -> exact installed PackageRef + manifest/definition join
  -> RegisteredExpertEndpoint { exact admission, selected registration }
  -> Directory::publish(owner, complete candidate set)
  -> TaskCoordinator resolves and stores exact admission once
  -> pinned endpoint constructs the existing invocation-scoped host
  -> selected registration.runner(host, request)
  -> trusted blocker/artifact/coverage handling
  -> ExpertReport -> TaskSnapshot -> existing settlement/product projection
```

The endpoint must retain the manifest and executable as one coherent registration. The request cannot select a different runner. Exact Person, package, definition, Task and invocation checks stay in place before package execution. A missing supplied implementation never falls back to the shipped bundle.

The bound implementation may be a callable or a compiled factory that creates an invocation runner. A factory is valid only when it already represents one concrete selected implementation and cannot look up an ID, a mutable registry, or a shipped catalog when invoked.

### Rust lifetime placement

The current runner type is `ExpertRun<Host, Request, Output>`, while `DelegatedMessageExperts` borrows per-invocation services and recorders. A runner stored in a long-lived endpoint must support each invocation's fresh host lifetime; storing `ExpertRun<DelegatedMessageExperts<'static,...>>` is not a solution.

Prefer the smallest safe stored callable/factory representation compatible with the existing registration API. If a higher-ranked callable is sufficient, use it. If host-associated types prevent that representation, change the existing bundle runner boundary to one object-safe/erased callable with an explicitly borrowed host; keep the same host operations and ownership. The exact Rust spelling must compile against the real host, not only a dummy trait.

Do not add `unsafe`, `transmute`, leaked hosts, global run-ID maps, function-address digests, or a second forwarding host solely to hide a lifetime error. Preserve `AuthorizedRead`/held source-read lifetime and release behavior. Any necessary model `?Sized`/trait-object changes are mechanical caller migration, not new model routing. Prove the storage-and-call seam with the real product host before migrating the full set of callers. Do not relocate App-specific host types into shared Agent/Experts contracts.

### Runtime registrations are not first-party trust

The supplied runtime set says which statically controlled implementations can execute; it is not an Observe grant list. The product-shipped trusted subset remains the only input to first-party automatic consumer derivation. Installing or supplying `example.test.expert` must not add it to first-party Observe consumers.

Default product composition creates the shipped registration set at the composition boundary. Install manifests and runtime publication derive from those descriptors, not two independently authored lists. A pure manifest projection is acceptable for policy/install code; it must use the same declarations and never substitute for the executable at endpoint binding.

Tests may supply an additional explicit registration collection through the same composition constructor/helper as production. There must be no `cfg(test)` branch that changes dispatch decisions, no magic extension ID, and no public/FFI registration command added for testing. Use current Registry APIs to explicitly install the test manifest; ordinary runtime publication does not auto-install packages.

## BF-0: baseline and dependency map

Read:

1. `AGENTS.md`, `docs/README.md` and the two repo-local architecture-change/code-change-verification skills;
2. this document and the plan README; consult original 03-B/03-D/03-F only for preserved invariants;
3. `docs/architecture/{README.md,invariants.md,runtime.md,authority-recovery.md}`;
4. the source anchors above, `stateful_settlement.rs`, bundle `host.rs`, `Directory`, `TaskCoordinator`, App first-party policy and the existing App endpoint/Registry fixtures.

Record local HEAD, origin HEAD, worktree and the source delta from `683b118b00eafb0d931c0a5f5701d16d978d96ba`. Fetch is allowed; fast-forward only when safe. Never reset, clean or stash away user work. Do not run baseline tests concurrently with production edits.

```sh
git status --short --branch
git rev-parse HEAD
git fetch origin main
git rev-parse origin/main
git diff --name-status 683b118b00eafb0d931c0a5f5701d16d978d96ba...HEAD
df -h .
cargo test -p floe-experts --test delegation
cargo test -p floe-experts-builtin
cargo test -p floe-app first_party_observe --lib -- --test-threads=1
cargo test -p floe-app vault_registry --lib -- --test-threads=1
python3 tools/architecture/check_boundaries.py
(cd apps/client && flutter test test/features/experts/agent_registry_test.dart)
```

Record actual results; zero-test filtered output is not coverage. Preserve known parallel timeout evidence separately from serial passes. Keep ordinary incremental compilation and existing target/profile/flags during iteration. Do not create another target directory or automatically clean caches. Any required disk cleanup is limited to identified generated artifacts with user work and uncertain operation records preserved.

## BF-1: bind and execute the supplied runner

### BF-1a. Composition and validated registration set

In bundle `registration.rs` and App composition, create a bounded immutable supplied registration set that owns both manifests and runnable implementations. Reuse the generic descriptor; add an App-private collection/handle only if needed to validate and retain registrations across the Vault lifetime.

Validate the entire supplied set before changing Directory: manifests, duplicate exact package identities, duplicate/conflicting public definitions, and the currently supported one-callable-assignment rule. Reuse current limits and failure semantics. Missing installed packages may remain non-callable, but an exact PackageRef with different installed/supplied manifest contents must fail closed; never silently substitute a shipped entry.

In `OpenVault`, accept/retain the supplied set and make `publish_expert_directory` use that set. The default creation path supplies shipped registrations; the same path accepts the statically controlled test set. Do not add a separate test-only publisher. Installation/ExistingOnly semantics, disabled-assignment preservation, and first-party trust remain unchanged.

### BF-1b. Bound product endpoint

Change `RegisteredExpertEndpoint::new` to take exact admission plus the selected registration/implementation handle. Store it immutably. Do not offer an alternate constructor that accepts only a manifest and looks up a runner later.

At publication, check the installed manifest and exact admission against the supplied registration before constructing the endpoint. Keep one atomic `Directory::publish` call for the full owner set; preserve unrelated owner entries and the existing collision semantics. Reuse stable handles where possible rather than rebuilding endpoints merely to trigger publication revision churn. This is not a redesign of Directory revision policy.

At execution, validate the invocation against the bound admission/manifest, then construct the same production services and invocation-scoped host as today. Invoke the retained implementation directly. Delete both execution-time `shipped_bundle_dispatch()` call sites and the ID-membership gate they provide. The absence of that gate does not authorize arbitrary requests: admission/registration identity is now the gate.

### BF-1c. Common execution seam and remaining callers

Rename/generalize `execute_builtin` as appropriate and pass the bound registration explicitly. Preserve:

- `ExpertModelHost` / shared `InferenceExecutor`, `experts.delegated` model consumer, and exact package source consumer;
- exact request/session/device/Task lineage, cancellation and output/budget limits;
- Context requirement-key validation and per-read authority;
- captured dependencies, held source leases and artifact/report coverage binding;
- host-owned source/model blocker publication and linked-resume references;
- raw requirement / raw Actions artifact rejection;
- exact admitted assignment passed into stateful settlement;
- the existing `ExpertReport -> TaskSnapshot` path without A2A-to-report reconstruction.

Audit `ConversationExperts::handle_message`, `task_runners` and A2A test helpers. Necessary adapters must delegate through the same canonical Task path or receive the same explicit bound registration. Delete a caller-zero fallback instead of adding an `Option<runner>` with a shipped default. A2A display projection may remain; a second implicit execution path may not.

Remove `shipped_bundle_dispatch` once caller-zero. Delete `ExpertDispatchTable` only if the real caller audit proves it has no independent remaining use; do not remove an otherwise legitimate generic owner API merely to produce a zero-name count.

## BF-2: executable product-path acceptance

Add an App test module near the current endpoint/Registry tests, for example `crates/app/src/vault_host/tests/registered_runner.rs` (proposed path), and wire it into the existing test module tree. Reuse `DirectEndpointFixture`, fresh encrypted Vault/key fixtures and existing local provider stubs where applicable.

**Required route:** supplied registration -> real Registry installation -> the real App publication/endpoint builder -> real `RegisteredExpertEndpoint` -> existing `DelegatedMessageExperts`/trusted execution seam -> `TaskCoordinator` with `VaultTaskRepository` -> durable Task result. A custom bare `AgentEndpoint`, `ExpertTaskRunner` override, `FixtureEndpoint`, or manually fabricated terminal Task is not a substitute.

A test package may deterministically return a bounded text/artifact without a model call. Any prerequisite Inference availability is supplied through established provider fixtures, not a fake Ready branch in the endpoint. No real external account or network-side mutation is required.

Use a common test prefix such as `registered_runner_` for focused execution. Names below describe required assertions; record actual executable names.

| Case | Required proof |
|---|---|
| Non-builtin zero-source runner | A statically supplied `example.test.expert` runner executes through the real App endpoint, exactly once; its unique result marker and independent artifact reach the durable Task and safe projection. It is not present in the shipped catalog. |
| Required but unconfigured source | The card remains callable. A read through the real host requirement API returns its actual typed limitation; no fake authority is inserted. Report the exact outcome rather than assuming all sources return the same blocker. |
| Undeclared requirement | The supplied runner attempts an absent key and the host rejects it before source payload I/O. Run this against the supplied manifest, not a shipped package lookup. |
| Manifest/admission mismatch | Wrong package version, definition revision, Person or installed manifest is rejected; no runner or payload I/O executes. |
| Missing/duplicate implementation | Missing supplied registration is non-callable, duplicate/conflicting supplied definitions fail before publication, and neither case falls back to a shipped implementation. The previous Directory set remains intact on failure. |
| Actual product runner pin | Runner A enters after admission and waits on an explicit test barrier. Publish a valid replacement registration B using a changed package/definition identity and current Registry APIs. A returns its A marker; a newly admitted Task invokes B and returns B. Use real product endpoints for both. |
| Replay and disable | Replaying A's completed Task preserves stored admission/output and does not execute A or B again. Disabled assignments block new admissions without erasing an admitted Task's result. |
| No implicit first-party grant | Supplying/installing the non-builtin package leaves first-party policy consumers/fingerprints unchanged. Manager `assistant` and the shipped consumer sets retain current semantics. |
| Trusted artifact/blocker path | Forged requirement/Actions media are rejected or routed through existing trusted validation exactly as before; source/model blockers still use durable interaction references. Retain existing real-host regression coverage rather than replacing it with runner-only tests. |

For the A/B pin test, change implementation semantics only under an appropriately changed package/definition identity. Do not bless mutable implementations under an unchanged immutable manifest identity. Use `Notify`, a channel or equivalent deterministic synchronization, not short sleeps or enlarged production deadlines. The pin under test is the actual product endpoint's executable, not only Directory's endpoint Arc.

Retain `modules/experts/tests/delegation.rs` as lower-owner coverage. Label it accurately in the report; the new App route supplies the missing product evidence.

## BF-3: remove the stale client Registry target

In `apps/client/lib/features/experts/domain/agent_registry.dart`, delete `AgentRegistryTarget.calendarView` and its `calendar_view` special serialization. Keep only installation/assignment. Update reachable callers and focused tests/support. Search beyond enum uses for raw `'calendar_view'` Registry target strings.

The Rust Registry configuration contract already has the two valid targets. Do not add a backend command, substitute an Access permission mutation, retain an alias, or create a compatibility decoder. Verify both supported client commands still serialize correctly and backend protocol rejection of the obsolete target remains. Limit this cleanup to the stale target; do not redesign Registry settings or introduce 04 binding UI.

## BF-4: verification and deletion gate

### Targeted gate

Run after the corresponding changes, recording actual counts/results:

```sh
cargo test -p floe-experts --test delegation
cargo test -p floe-experts-builtin
cargo test -p floe-app registered_runner_ --lib -- --test-threads=1
cargo test -p floe-app first_party_observe --lib -- --test-threads=1
cargo test -p floe-app vault_registry --lib -- --test-threads=1
cargo test -p floe-app schedule_settlement_binds_evidence_to_the_exact_grant_policy_dependency --lib -- --test-threads=1
cargo test -p floe-protocol
python3 tools/architecture/check_boundaries.py
git diff --check
(
  cd apps/client
  flutter test test/features/experts/agent_registry_test.dart
  flutter test test/features/conversation/agent_delegation_fixture_test.dart
  flutter test test/features/actions/agent_proposal_card_test.dart
)
```

Change test filters only to the real names/targets on execution HEAD. Never report a zero-test filter as passed coverage. Do not regenerate the existing Rust-to-Dart fixture unless the actual product representation changes; runner wiring alone should not change it.

### Broad gate and known timeout qualification

Use the explicitly qualified serial workspace mode established at the 03 baseline:

```sh
cargo check --workspace --lib
CARGO_INCREMENTAL=0 RUST_TEST_THREADS=1 cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
cargo build -p floe-ffi
(
  cd apps/client
  flutter analyze
  flutter test
  flutter build macos
)
git diff --check
```

This proves the full suite in serial mode, not default-parallel reliability. If parallel runs are attempted, report their failures separately; do not hide them, increase production deadlines, or add ignores. Retain names/reasons for existing ignored tests. Broad-gate environment flags apply only to that command, not global project settings.

macOS is the product gate. iOS is not executed unless separately authorized; no Android parity work. Go/server is out of scope; if unexpectedly touched, justify the expansion and run the Go verification skill before closure. Preserve the proposal-card golden exception even while executing surrounding non-golden tests.

### Residual audit

```sh
rg -n 'shipped_bundle_dispatch|execute_builtin|task_runners|RegisteredExpertEndpoint' crates/app/src
rg -n 'floe_experts_builtin::(manifests|registrations)|from_package_id|BuiltinExpertKind' crates/app/src
rg -n 'ExpertDispatchTable|ExpertRun|ExpertRegistration' crates
rg -n 'AgentRegistryTarget\.calendarView|calendar_view|calendarView' apps/client/lib apps/client/test
rg -n 'builtin_setups|staged_registry|resolve_builtin|granted_tool_assignments' crates apps/client/lib
```

Interpret matches by meaning and scope. Bootstrap-only shipped declarations, first-party policy projection, bundle-local code, unrelated legitimate Calendar APIs and historical docs are not runtime fallback. There must be no shipped/ID lookup from `RegisteredExpertEndpoint::execute` or the common execution seam, no manifest-only endpoint constructor with fallback, no test-only dispatch branch, and no stale client Registry calendar target.

Compare golden code/PNG against the task start commit. Leave both unchanged. Do not claim a repository clone/build was performed unless it actually was.

## BF-5: documentation, commits and completion

Update `docs/architecture/runtime.md` to describe the actual supplied-registration join and bound runner execution after implementation. Touch authority/recovery documentation only if a current statement requires convergence; no source-binding claim is added. Preserve the known 04 `ConfirmedInteractions` outcome follow-up in the completion report without editing its production behavior here.

The README's 03 row is reopened only for this bounded residual. Preserve the original completion SHAs and serial-test qualification. Mark 03 Complete again only after BF-1/BF-2 product evidence, BF-3 cleanup, residual audit and BF-4 gates. Append the real closure implementation/test/verification SHA references; leave 04 Not started. Do not create a second STATUS file or erase the original 03 evidence.

Recommended commit boundaries:

1. BF-1 + BF-2: bound registration/product endpoint and permanent product-path tests, one coherent green change set;
2. BF-3: stale client target removal and its focused tests, separately if independent;
3. BF-4/BF-5: verification evidence and current architecture/status convergence.

Combine adjacent changes rather than introduce a compatibility path for commit splitting. Keep evidence in this plan or the existing authoritative report location, not a parallel migration ledger. Local implementation commits are permitted by the execution task; **do not push**.

## Required implementation report

Report:

1. start/final local and origin HEAD, worktree state and actual substep commit SHAs;
2. concrete stored runner/factory type, lifetime strategy and owning files;
3. final path from supplied registration through App publication, real endpoint/host, Task and settlement;
4. deleted runtime catalog lookups, fallback constructors and migrated A2A/test callers;
5. exact App product-path test names and proof no bare test endpoint/runner override bypassed the host;
6. A/B replacement markers, admission identities, runner call counts and completed replay behavior;
7. unchanged first-party consumer/fingerprint behavior for an additionally supplied package;
8. stale client Registry target removal and supported command tests;
9. targeted/broad commands with pass/fail/unavailable results, ignored tests and explicit serial-vs-parallel qualification;
10. residual findings, golden code/PNG comparison, unchanged 04 source behavior and any unexpected scope expansion;
11. current architecture and README changes; 03 closed only when the evidence supports it, 04 still Not started;
12. final local commit(s), no push, and any remaining blocker stated concretely.
