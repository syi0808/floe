# 05: obsolete-path deletion and extensibility conformance

Prerequisite: checkpoint 04 is complete. This checkpoint is the final deletion/conformance audit before checkpoint 06. It is not permission to preserve obsolete paths until the end.

## Plan authority and baseline

This document is the authoritative execution plan for checkpoint 05. Do not create a parallel 05 status file or migration ledger.

Plan baseline rechecked on 2026-09-27:

- main: 363130d9ebf2951d9746290221f5ce905acfcdeb
- checkpoint 04: Complete
- checkpoint 05: Not started
- checkpoint 06: Not started
- proposal-card golden exception: unchanged; do not enable, delete or regenerate it
- Android parity: out of scope
- iOS: not required for this checkpoint unless an iOS-owned file is independently changed

Read before implementation:

1. AGENTS.md
2. .agents/skills/architecture-change/SKILL.md
3. .agents/skills/code-change-verification/SKILL.md
4. docs/development/plans/expert-extensibility/README.md
5. this document
6. docs/architecture/README.md
7. docs/architecture/invariants.md
8. docs/architecture/runtime.md
9. docs/architecture/authority-recovery.md

Line numbers below are anchors at the plan baseline. Re-resolve the symbol if main moves; do not edit by stale offset.

## Current-main findings that define the work

### F1. Canonical remote source acquisition already exists

The production path is already:

~~~
App RemoteViewReader / BoundRemoteViewReader
  -> Context read_remote_view / read_selected_remote_view
  -> Access preview + exact grant/source/policy admission
  -> AuthorizedSourceClient
  -> ServerSourceClient::read_admitted_view
  -> /admit -> /read -> /release
~~~

Current anchors:

- crates/app/src/vault_host/remote_views.rs:20-166
- crates/modules/context/src/application/remote_sources.rs:1-700
- crates/adapters/providers/src/sources/server.rs:171-309
- crates/adapters/providers/src/sources/server.rs:1232-1347
- server/internal/transport/http/source.go:10-76

The server accepts only calendar.timeline, mail.communication, work.context and life.logistics at this source route, and requires the admission protocol for the non-Calendar Views. Do not add unsuffixed success routes.

### F2. The old ServerSourceClient convenience APIs have no non-test production caller

At the baseline the following methods still exist in crates/adapters/providers/src/sources/server.rs:

- read_communication_view:312
- read_work_context_view:339
- read_calendar_context_view:460
- read_confirmed_interaction_view:501
- read_logistics_view:516
- read_people_view:532
- read_attention_view:546
- read_wellbeing_view:560
- read_personal_view:574
- read_view:584

The apparent calls in crates/app/src/vault_host/conversation_turn.rs are inside the #[cfg(test)] module that begins at line 557. They are test-only legacy transport fixtures:

- FixtureRemoteReader:697-960
- commitments_delegation_reads_fresh_view_and_returns_typed_artifact:2710
- commitments_artifact_preserves_mail_calendar_task_and_memory_provenance:2895
- portfolio_delegations_read_fresh_views_and_return_typed_artifacts:3138

Provider-adapter tests at server.rs:699-1035 also exercise the obsolete unsuffixed read helpers.

Therefore 05-A is a deletion/migration task. Do not retain a production wrapper because an old test uses it.

### F3. Several closed-world or historical semantics are now caller-zero

Current search results:

- crates/experts/builtin/src/catalog.rs:53-57 BuiltinExpertKind::from_package_id has no production caller; only its local round-trip test uses it.
- crates/experts/builtin/src/catalog.rs:185-223 BuiltinSourceRequirement and BuiltinContextSource::requirement have no repository caller outside their definition/export.
- crates/contracts/agent/src/expert.rs:25-48 ExpertBudget and MAX_EXPERT_VIEW_BYTES have no repository runtime caller beyond their definition/re-exports; max_insights is additionally a historical shared-result name after ExpertInsight removal.
- PackageKind::Tool has no production Registry/package caller; the current Rust match is a negative ExpertManifest test, while apps/client/lib/features/experts/presentation/agent_capability_label.dart still has a Tool-shaped Registry presentation branch.
- apps/client/test/features/experts/agent_expert_result_test.dart is already absent; only historical plans still name it.
- PackageImplementation, FindFocusWindow, StatefulFocusProposal and ExpertFocusProposal are historical-plan matches, not current production symbols.

Delete caller-zero code; do not recreate baseline-absent paths.

### F4. One package-specific source identity still leaks into production App host code

crates/app/src/vault_host/conversation_turn/expert_host.rs:817-854 builds Calendar SourceAccessRequirement and currently obtains its logical source ID through:

~~~
floe_experts_builtin::BuiltinContextSource::Calendar.source_id()
~~~

at line 835.

This is not execution dispatch, but it makes the common App acquisition host depend on a builtin package declaration for a Context-owned source identity. Checkpoint 05 must remove this dependency without changing grant semantics.

The canonical source/access identity must come from Context/Context-contract ownership. Do not replace the enum reference with an App hard-coded package string.

### F5. Existing non-builtin evidence is substantial but does not close 05-C by itself

Existing permanent evidence includes:

- crates/modules/experts/tests/delegation.rs:332 nonbuiltin_registration_installs_publishes_and_completes_without_source_binding
- crates/app/src/vault_host/tests/registered_runner.rs:833 registered_runner_nonbuiltin_uses_product_endpoint_and_durable_task
- registered_runner.rs:1089 registered_runner_required_unconfigured_source_returns_typed_outcome
- registered_runner.rs:1836 registered_runner_extension_does_not_change_first_party_observe_policy
- crates/app/src/vault_host/tests/conversation_flows.rs:1054 production_builtin_expert_binding_refresh_links_fresh_selected_task
- apps/client/test/features/experts/agent_registry_dialog_test.dart:115 unknown Expert renders manifest metadata without a UI branch

These tests prove important pieces, but E1 still needs the add-one-bundled-Expert experiment and E2 still needs one non-builtin package to cross the combined configuration/authority/linked-resume path.

### F6. Dependency checking exists; source-semantic enforcement does not

tools/architecture/check_boundaries.py currently checks Cargo/dependency topology and explicitly reports that source-level semantic checks are excluded.

Do not overload dependency-graph semantics with regexes hidden inside graph code. Add a companion source-semantic checker under tools/architecture with its own regression tests, and invoke both tools in 05/06 verification.

## Final state required by checkpoint 05

At completion:

1. ServerSourceClient has one canonical authorized remote read path; no unsuffixed typed convenience read remains.
2. Test fixtures do not resurrect old provider routes or manufacture fake authority when the assertion claims production authority.
3. Common App/Context owners do not depend on builtin package enums for source identity or dispatch.
4. Caller-zero historical fields/helpers are removed.
5. Generic Registry/Vault tests are named and structured around generic semantics rather than a synthetic Schedule topology.
6. A bundled Expert can be added by changing only the builtin package implementation for production registration; no common production owner changes.
7. A statically supplied non-builtin package can use the normal production registration/admission/configuration/delegation path without acquiring first-party trust from its ID.
8. Source/grant authority remains Access-owned; binding remains configuration only.
9. Unknown package artifacts remain inert generic artifacts unless an owning domain such as Actions explicitly recognizes its own media type.
10. Machine checks prevent the specific closed-world regressions removed in this checkpoint.
11. Checkpoint 06 remains Not started until 05 completion evidence is recorded.

## 05-0: baseline and disposition ledger

Before implementation edits:

1. Record local HEAD, fetched origin/main and working-tree status.
2. Preserve all user changes. No reset --hard, clean, automatic stash, profile reset or key deletion.
3. If origin/main is newer than the plan baseline, inspect every anchor below before applying edits.
4. Run the residual selectors in 05-D and classify every production/test match as:
   - DELETE: obsolete/caller-zero path;
   - MOVE/REWRITE: invariant survives but old fixture/topology does not;
   - RETAIN: legitimate package-local/domain-owner concept;
   - HISTORICAL: plan/ADR prose only.
5. Capture the initial count and paths for the eight ServerSourceClient convenience names, BuiltinContextSource in App, from_package_id and max_insights.
6. Do not treat a zero-test filter as evidence.

No production change belongs in 05-0. Commit only if the baseline itself requires a factual plan correction.

## 05-A: delete obsolete ServerSourceClient read surface

### 05-A1. Remove the dead production API, not the canonical transport

Primary file:

- crates/adapters/providers/src/sources/server.rs

Delete the obsolete surface anchored at the baseline by:

- CalendarContextRequest: roughly 70-78
- valid_connection_id: roughly 80-91 if no caller remains
- typed direct read methods at 312-609
- read_personal_view
- read_view
- imports/constants used only by those methods

Retain:

- ServerSourceClient connection preparation and pairing identity
- authorization_client
- read_admitted_view
- read_authorized_view
- observe_calendar_connections if still used by the Calendar product/connection owner
- observe_source_connections
- authenticated_get for metadata discovery
- AuthorizedSourceClient
- RemoteGrantTransport implementation
- RemoteViewTransport implementation

After deletion, ServerSourceClient must not POST directly to an unsuffixed /v1/views/<view> endpoint.

Do not add a forwarding compatibility facade.

### 05-A2. Rewrite provider tests by semantic ownership

Current obsolete-path tests in server.rs:

- cancelled_queued_source_read_never_opens_a_provider_connection:700
- communication_view_read_is_authenticated_bounded_and_validated:749
- portfolio_view_reads_use_fixed_routes_and_strict_validation:836
- source_reads_never_touch_model_or_catalog_discovery:962

Disposition:

1. Preserve the cancellation/queued-call assertion by moving it onto the canonical authorized read or authorization transport path. Cancellation before provider handoff must still produce zero outbound connection attempts.
2. Replace the communication direct-route test with canonical admit/read/release transport coverage only if equivalent Rust adapter coverage does not already exist. The test must verify the actual signed/admitted path, not a new fake unsuffixed endpoint.
3. Delete fixed-unsuffixed-route assertions. The server source protocol, not a typed convenience wrapper, owns route shape.
4. Retain the “source reads do not trigger model/catalog discovery” meaning only at the canonical path. Do not keep the old method solely for this assertion.
5. Keep catalog metadata tests at server.rs:1037+; they are legitimate source discovery metadata and are not Expert read compatibility.

Also inspect server/internal/application/calendar_admission_test.go and existing Context remote-source tests before adding duplicate coverage. Preserve signature, producer identity, admission, release, malformed-input and response-loss assertions.

### 05-A3. Remove the App legacy transport fixture

crates/app/src/vault_host/conversation_turn.rs test module:

- FixtureRemoteReader:697-960
- legacy_source_client:1656 and its test callers
- three package-integration tests at 2710, 2895 and 3138

For each test:

- If the test asserts Expert/package behavior or provenance after a Context read, replace FixtureRemoteReader with an owner-level SelectedSourceReader/CalendarContextReaderApi fake that returns bounded typed payload plus explicit ContextDependency. It must not claim to prove provider admission.
- If the test asserts source authority or signed provider behavior, drive the real RemoteViewReader -> Context -> AuthorizedSourceClient path with the existing signed transport fixtures instead.
- Do not manufacture a fresh GrantAuthority/SourceAuthority in an App fake and call that “production authority”.
- Keep the resulting assertion at the narrowest semantic owner.

The expired-dependency-before-I/O test may retain a prepared ServerSourceClient only as an external transport endpoint; it must not depend on a deleted convenience method.

### 05-A4. Preserve server denial of legacy unsuffixed routes

The Go server already treats unsuffixed View reads as invalid/admission-required. Matches in server/internal/application/console_test.go and server/internal/application/person_test.go that deliberately POST legacy unsuffixed routes and assert rejection are RETAIN negative regressions, not compatibility clients.

Do not delete those tests merely to obtain a zero string-search count. The final distinction is:

- no Rust success caller or fixture uses an unsuffixed View route;
- canonical server success uses admit/read/release;
- Go may mention an unsuffixed route only to prove it is rejected.

### 05-A5. Deletion gate

At the end of 05-A:

~~~
rg -n 'read_communication_view|read_work_context_view|read_calendar_context_view|read_confirmed_interaction_view|read_logistics_view|read_people_view|read_attention_view|read_wellbeing_view|read_personal_view' crates
~~~

Expected production/test source count: zero, except this plan/historical documentation if docs are included separately.

Also search for direct unsuffixed POST construction:

~~~
rg -n 'post\(.*v1/views|"/v1/views/(mail\.communication|work\.context|life\.logistics|relationships\.confirmed_interactions|people\.identity|attention\.coarse|wellbeing\.derived)"' crates
~~~

Any remaining direct route must be individually justified as canonical protocol code; do not allow a provider convenience read.

Suggested coherent commit: 05-A provider canonicalization/deletion plus migrated tests.

## 05-B: remove remaining closed-world common-owner residue

### 05-B1. Move Calendar source-access identity to Context ownership

Current leak:

- crates/app/src/vault_host/conversation_turn/expert_host.rs:817-854
- exact builtin reference at 835

Implement one Context-owned canonical source-access identity mapping for capabilities that can produce SourceAccessRequirement. Prefer a small function/constant at the existing Context/Context-contract source-access boundary; do not create another registry.

Then:

1. Context selected/declaration read resolves the canonical logical source-access ID.
2. LocalExpertSourceDriver receives that owner-defined ID together with the exact selected refs, or receives an equivalent Context-owned request value.
3. AppLocalExpertSource forwards it to Calendar acquisition.
4. PersonalViewSource::calendar_views and CalendarContextReaderApi carry it only as opaque Context identity.
5. calendar_access_requirement uses that Context-owned ID.
6. BuiltinContextSource remains package-local metadata only if still needed to construct builtin manifests; App production must not import it.

Required regression:

- a statically supplied non-builtin Calendar requirement may use an arbitrary requirement key while the SourceAccess blocker still carries the canonical Context source identity and exact non-builtin consumer;
- no builtin package ID or enum is consulted to construct the blocker.

Do not change grant purpose, consumer identity, resource set, reviewed grant, source authority or inline-resolution rules.

### 05-B2. Delete caller-zero historical API and synthetic Registry Tool package surface

Delete after re-search confirms the baseline:

- crates/experts/builtin/src/catalog.rs:53-57 BuiltinExpertKind::from_package_id;
- its self-only round-trip test around 248-253;
- BuiltinSourceRequirement at catalog.rs:185-194;
- BuiltinContextSource::requirement at catalog.rs:213-223;
- the BuiltinSourceRequirement re-export in crates/experts/builtin/src/lib.rs;
- crates/contracts/agent/src/expert.rs ExpertBudget as a whole, not merely max_insights, if all fields remain caller-zero;
- MAX_EXPERT_VIEW_BYTES and the ExpertBudget/MAX re-exports from floe-agent-contract and floe-experts when no independent caller remains.

Also remove the obsolete Registry package Tool variant when latest-main search still shows no production caller:

- delete PackageKind::Tool but retain PackageRef.kind / PackageKind::Expert if current persistence and wire identity intentionally serialize kind: expert;
- update the ExpertManifest negative test so obsolete tool is rejected at the serialization/contract boundary rather than kept as a supported enum value;
- remove the Tool-shaped branch from apps/client/lib/features/experts/presentation/agent_capability_label.dart and associated Tool-only Expert Registry fixtures;
- re-run protocol/FFI/Dart compatibility if generated contract output is affected.

This does not delete Manager ToolCall, ToolDescriptor or model tool execution. Those are Conversation/Model capabilities, not synthetic Registry Tool packages.

Do not replace max_insights with a renamed shared insight budget. Package result/output limits remain with their actual runtime/package owners.

Do not delete BuiltinExpertKind merely because it is closed-world inside the shipped bundle. A compile-time builtin catalogue is allowed. The prohibition is common-owner dispatch/trust based on that enum.

Do not delete BuiltinContextSource if registration.rs still uses its capability/source metadata to build package-owned manifests.

### 05-B3. Genericize stale App Registry/Action test topology

Current old-shaped fixture:

- crates/app/src/vault_host/tests/schedule_host.rs:1-132
- TestScheduleHost mutates the shipped Schedule manifest into floe.schedule
- callers: proposals.rs, vault_registry.rs and expert_actions.rs

Replace it with a generic Expert test fixture owned by the App test tree, for example TestExpertRegistryHost, built from public ExpertManifest/ExpertRegistration semantics without cloning Schedule.

Migrate:

- proposals.rs
- vault_registry.rs
- expert_actions.rs and expert_actions/inspection.rs as applicable

Preserve:

- exact admission/assignment identity
- private-state/settlement CAS
- encrypted-at-rest assertions
- Actions contributor/evidence/approval checks
- uncertain external-write recovery

Delete schedule_host.rs once all surviving assertions have a generic home.

### 05-B4. Rehome generic bundle-install tests

crates/app/src/vault_host/tests/vault_registry/builtin_setup.rs currently contains two genuinely generic tests:

- generic_bundle_install_is_source_independent_and_idempotent
- generic_assignment_disablement_survives_reopen_and_ensure

Move/rename them under a generic Registry fixture and use a generic manifest set unless the assertion explicitly concerns shipped-bundle bootstrap.

Do not delete these semantics:

- exact install-operation rejoin
- manifest digest mismatch conflict
- source-independent install
- disablement surviving reopen/ensure

The package-owner tests in crates/experts/builtin/tests/registry.rs may retain shipped-bundle semantics; they are not common-owner coupling merely because they enumerate the shipped package.

### 05-B5. Test/dead-fixture audit

Confirm baseline-absent artifacts remain absent:

- ScheduleEndpoint
- old Calendar Expert App directory
- experts/builtin/tests/calendar_setup.rs
- old Registry calendar_setup/calendar_access modules
- old Flutter Calendar Expert controller/dialog/domain/support tests
- apps/client/test/features/experts/agent_expert_result_test.dart and its old support parser

Do not recreate any of these to satisfy an old test.

Suggested coherent commits:

- 05-B1 Context-owned blocker/source identity
- 05-B2 caller-zero API deletion
- 05-B3/B4 generic test topology cleanup

Combine adjacent commits if a temporary compatibility wrapper would otherwise be required.

## 05-C: extensibility conformance experiments

### 05-C1. E1 — temporary ninth bundled Expert

Purpose: prove that adding one shipped package is a package change, not a common-runtime change.

Use a temporary conformance Expert with an existing low-authority capability, preferably floe.tasks, so the experiment tests registration/binding/delegation rather than inventing a provider feature.

Temporary production diff is allowed only under:

- crates/experts/builtin/**

The temporary package should define, inside the builtin package:

- one new builtin catalogue/registration entry
- package-owned metadata/prompt/result contract
- one existing source requirement
- one package runner
- one ordinary package-owned result artifact/media type

Do not edit production App, Experts, Vault, Context, Access, protocol, FFI, Flutter, provider or server code to recognize the package.

Test-only harness changes outside builtin are allowed only to drive existing product APIs.

Drive the temporary package through:

1. shipped registration discovery;
2. install and exact assignment;
3. product list/detail DTO;
4. candidate inspection and explicit source binding;
5. Directory publication;
6. Manager model selecting the new agent by manifest/card ID;
7. production TaskCoordinator and bound endpoint;
8. exact requirement read;
9. terminal ExpertReport/Task result;
10. generic product/client projection.

Client evidence must include the existing generic unknown-Expert rendering surface; no package ID switch may be added to Flutter.

Record:

- exact temporary diff
- production paths outside crates/experts/builtin: expected zero
- test-only harness paths
- commands and results

Do not ship the fake Expert. Remove the temporary package after recording the experiment, using an exact reverse patch or equivalent bounded edit; do not reset away unrelated work.

Permanent machine checks and E2 tests remain after the temporary E1 package is removed.

### 05-C2. E2 — permanent non-builtin production-chain conformance

Extend the existing example.test.expert / supplied-registration test family instead of creating a fake Directory.

Preferred permanent anchors:

- crates/app/src/vault_host/tests/registered_runner.rs
- crates/app/src/vault_host/tests/conversation_flows.rs
- crates/app/src/vault_host/tests/interaction_resolution.rs only where owner resolution must be exercised
- apps/client/test/features/experts/agent_registry_dialog_test.dart for generic UI behavior

The same statically controlled non-builtin package must prove:

1. normal registration and installation;
2. exact assignment identity;
3. explicit binding using opaque candidate IDs;
4. source read through the production host seam;
5. no access to another Expert's selected refs;
6. no automatic inclusion in selected_shipped_consumers / first-party Observe;
7. generic artifact/result persistence and replay;
8. one recoverable authority interaction followed by reviewed resolution and a fresh linked resume.

For the recoverable interaction, do not broaden first-party Observe policy merely to make the extension pass. A safe preferred shape is:

- configure an intrinsic/local capability such as floe.tasks through the normal binding path;
- trigger delegated model recipient consent for experts.delegated;
- resolve the trusted model-access interaction using the existing owner;
- assert the linked child is a fresh Run/Task and re-admits current binding.

If a source-access review is used instead, it must go through a legitimate product/Access policy that can explicitly review the exact extension consumer. Do not build a fake grant directly and do not add the extension to automatic first-party consumers.

Required negative assertions:

- a package ID prefix does not imply trust;
- Manager assistant grant does not authorize the extension;
- another Expert's binding does not authorize the extension;
- old review after rebind is superseded before mutation;
- replay of completed Task executes no runner/provider again.

### 05-C3. Result/action genericity

Use an ordinary non-Schedule artifact in the conformance package.

Assert:

- Task/A2A/Conversation/Flutter generic result projection does not parse package JSON;
- unknown package artifact is inert unless an owning subsystem recognizes its own media type;
- Actions continues to recognize only Actions-owned Calendar proposal artifacts with exact evidence;
- no generalized dispatch requires a Schedule result media type.

Do not alter the approved proposal-card golden assertion or PNG.

Suggested coherent commit: 05-C permanent conformance tests only. E1 temporary fake package must not remain in the final tree.

## 05-D: residual audit and machine enforcement

### 05-D1. Run source searches by meaning

Run from repository root:

~~~
rg -n 'BuiltinExpertKind|BuiltinContextSource|BuiltinExpertSetup|BuiltinSourceBinding|builtin_setups' crates apps/client/lib apps/client/test server docs
rg -n 'CalendarExpert|ScheduleEndpoint|experts\.calendar\.install|AgentCalendarExpert' crates apps/client server docs
rg -n 'PackageImplementation|FindFocusWindow|focus_minimum_minutes|builtin_expert' crates apps/client server docs
rg -n 'ExpertInput|ExpertInsight|ExpertFocusProposal|StatefulFocusProposal|experts\.builtin|calendar\.expert' crates apps/client server docs
rg -n 'read_communication_view|read_work_context_view|read_calendar_context_view|read_confirmed_interaction_view|read_logistics_view|read_people_view|read_attention_view|read_wellbeing_view|read_personal_view' crates
rg -n 'enabled_builtin_expert_cards|required_tools|granted_tool_assignments|modelCalls == 1|view_calls.*!= 1|ExpertBudget|MAX_EXPERT_VIEW_BYTES|max_insights|from_package_id|PackageKind::Tool' crates apps/client
rg -n 'kind == .tool.|Connected information' apps/client/lib/features/experts apps/client/test/features/experts
~~~

Also inspect:

- grants(128) callers for exact-target classification versus source discovery
- calendar_connection uses for product ownership versus Expert target widening
- first-party package/prefix trust checks
- fallback/compat/legacy branches
- old unsuffixed source URL fixtures
- package-ID switches in App/Flutter
- generated contracts and tracked fixtures

Every remaining match must be classified. Historical plan text is not a production residual.

### 05-D2. Add a dedicated source-semantic architecture checker

Add:

- tools/architecture/check_expert_extensibility.py
- tools/architecture/test_check_expert_extensibility.py

Keep tools/architecture/check_boundaries.py focused on dependency graph policy.

The new checker should fail on at least these regressions:

1. any definition/call of the deleted ServerSourceClient convenience methods in executable Rust;
2. BuiltinContextSource use in production App;
3. BuiltinExpertKind-based execution dispatch or package-ID switch in production App;
4. common modules/runtime depending on floe-experts-builtin;
5. PackageKind::Tool or Tool-shaped Expert Registry/client presentation;
6. removed shared result/Focus symbols in executable code;
7. package-specific branching in the generic Flutter Expert registry/settings gateway;
8. reappearance of ScheduleEndpoint/Calendar Expert wire commands;
9. forwarding-only legacy source-read compatibility names.

Narrow owner-scoped allowances are acceptable for:

- crates/experts/builtin package-local implementation;
- App composition of shipped registrations;
- trusted shipped-manifest identification for first-party policy/default bootstrap;
- Actions-owned Calendar proposal handling;
- historical docs excluded from executable-source checks.

Do not use a blanket current-file allowlist. The checker should name the rule and offending path/line.

Regression tests must create minimal temporary source trees that prove each forbidden example fails and legitimate package-local/composition examples pass.

### 05-D3. Architecture completion audit

Before marking 05 complete, confirm:

- one generic registration/delegation path;
- no source permission state in Expert Registry;
- no source fallback/substitution at Expert runtime;
- no package-specific generic result parser;
- no obsolete source transport route needed only by tests;
- no migration-only nullable/dual-decoder state;
- no new dependency reversal;
- current architecture docs still describe the actual runtime.

Update docs/architecture only if implementation ownership/path changed. Do not add progress prose to an ADR.

## Verification sequence

### Per-step targeted tests

After 05-A:

~~~
cargo test -p floe-provider-adapters sources::server -- --nocapture
cargo test -p floe-context remote_sources -- --nocapture
cargo test -p floe-app commitments_delegation_reads_fresh_view_and_returns_typed_artifact --lib -- --test-threads=1
cargo test -p floe-app commitments_artifact_preserves_mail_calendar_task_and_memory_provenance --lib -- --test-threads=1
cargo test -p floe-app portfolio_delegations_read_fresh_views_and_return_typed_artifacts --lib -- --test-threads=1
~~~

Use exact current test names after migration. A zero-test filter is failure of the verification procedure.

After 05-B:

~~~
cargo test -p floe-agent-contract
cargo test -p floe-experts
cargo test -p floe-experts-builtin
cargo test -p floe-app vault_registry --lib -- --test-threads=1
cargo test -p floe-app expert_actions --lib -- --test-threads=1
cargo test -p floe-app proposals --lib -- --test-threads=1
~~~

After 05-C:

~~~
cargo test -p floe-app registered_runner_ --lib -- --test-threads=1
cargo test -p floe-app <new-nonbuiltin-linked-resume-test> --lib -- --test-threads=1
cargo test -p floe-experts nonbuiltin_registration_installs_publishes_and_completes_without_source_binding -- --nocapture
~~~

Flutter genericity:

~~~
cd apps/client
flutter test test/features/experts/agent_registry_dialog_test.dart
flutter test test/features/conversation
flutter test test/features/actions
~~~

Run the exact new E1/E2 tests by name and record their counts/results.

### Architecture tooling

~~~
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/test_check_boundaries.py
python3 tools/architecture/check_expert_extensibility.py
python3 tools/architecture/test_check_expert_extensibility.py
git diff --check
~~~

### Go/source protocol

Go production behavior is not expected to change.

Because 05-A deletes old Rust routes/fixtures, run the existing source admission tests that cover the canonical Go path. If any Go source/protocol file changes, run from server:

~~~
go test -race ./...
go vet ./...
~~~

Do not add a ConfirmedInteractions server route.

### Broad checkpoint-05 qualification

After all 05 implementation commits:

~~~
cargo check --workspace --lib
CARGO_INCREMENTAL=0 RUST_TEST_THREADS=1 cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/test_check_boundaries.py
python3 tools/architecture/check_expert_extensibility.py
python3 tools/architecture/test_check_expert_extensibility.py
cargo build -p floe-ffi

cd apps/client
flutter analyze
flutter test
flutter build macos
cd ../..

python3 tools/docs/check_docs.py
git diff --check
~~~

Report the Rust workspace run as serial qualification. Do not claim default-parallel reliability unless separately run.

If no FFI/client production contract changed, the FFI/Flutter broad checks are still useful checkpoint evidence; do not attribute them to the plan-writing commit.

Keep ignored/live-provider/EventKit tests explicitly reported as ignored/not-run rather than passed.

## Required safety assertions after deletion

The final tree must still have executable coverage for:

- exact consumer and recipient consent;
- grant/source/policy revision and producer identity;
- native subject identity;
- exact selected source/resource and no fallback;
- requirement-key selected-read separation;
- per-contributor provenance/coverage;
- unknown/unavailable versus empty distinction;
- Registry/binding/Task CAS;
- binding fence at provider handoff;
- successful terminal transaction fence;
- linked-resume fresh admission;
- durable pre-dispatch intent;
- uncertain external-write reconciliation;
- cancellation direction;
- token/secret non-leakage.

A deleted topology assertion needs no replacement. A deleted safety assertion does.

## Commit strategy

Use these boundaries only while every intermediate tree is coherent:

1. 05-A — delete legacy ServerSourceClient direct reads and migrate old test fixtures.
2. 05-B1 — remove common App dependency on builtin source identity.
3. 05-B2 — delete caller-zero historical API/fields.
4. 05-B3 — genericize Registry/Action test topology and remove schedule_host.
5. 05-C — permanent non-builtin conformance/linked-resume tests; E1 temporary experiment evidence recorded, fake package removed.
6. 05-D — source-semantic architecture checker, residual audit, docs/status evidence.

Combine adjacent commits rather than introducing a compatibility facade.

Do not start checkpoint 06 implementation inside these commits.

## Checkpoint-05 completion gate

05 is Complete only when all of the following are true:

1. 05-A through 05-D are complete.
2. All deleted ServerSourceClient convenience names have zero executable matches.
3. No production App source identity comes from BuiltinContextSource.
4. Caller-zero ExpertBudget/MAX_EXPERT_VIEW_BYTES, from_package_id and BuiltinSourceRequirement/requirement are gone if still caller-zero at execution time.
5. PackageKind::Tool and Tool-shaped Expert Registry/client presentation are gone if still production-caller-zero at execution time.
6. schedule_host old topology is removed and surviving assertions live under generic fixtures.
7. E1 demonstrates zero common-production diff for adding one bundled Expert, and the temporary fake package is absent from final HEAD.
8. E2 passes through real registration/admission/configuration/delegation/interaction/resume owners and does not receive first-party trust.
9. Source-semantic architecture checker and its negative fixtures pass.
10. Targeted and broad gates pass with exact results recorded.
11. Proposal-card golden exception remains byte-identical and untouched.
12. Current architecture docs match code.
13. README checkpoint status is changed from 05 Not started to 05 Complete only after all evidence is green.
14. Checkpoint 06 remains Not started.

## Required agent completion report

Report:

1. starting local/origin HEAD and worktree state;
2. actual 05-A/05-B/05-C/05-D commit SHAs;
3. final ServerSourceClient public read surface;
4. deleted legacy methods/helpers/tests and replacement safety coverage;
5. final Calendar blocker/source identity owner and path;
6. caller-zero cleanup results including ExpertBudget/MAX_EXPERT_VIEW_BYTES and from_package_id;
7. PackageKind::Tool / Flutter Tool-presentation cleanup and protocol compatibility result;
8. generic test-fixture migration and deleted schedule_host evidence;
9. E1 temporary package diff, commands/results and proof of zero common-production changes;
10. proof the E1 fake package is absent from final HEAD;
11. E2 non-builtin registration/binding/read/interaction/linked-resume evidence;
11. proof non-builtin package does not enter first-party Observe policy;
12. generic result/action artifact evidence;
13. source-semantic checker rules and negative fixture results;
14. residual-search classifications and any retained matches with owner/reason;
15. exact targeted/broad/Flutter/Go commands and results;
16. ignored/not-run platform/provider tests;
17. serial-versus-parallel qualification;
18. proposal-card golden byte comparison;
19. architecture/docs/status changes;
20. final local HEAD and clean/dirty worktree;
21. whether anything was pushed;
22. checkpoint 06 still Not started.
