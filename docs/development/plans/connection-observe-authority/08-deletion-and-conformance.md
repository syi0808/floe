# 08: Legacy purge and conformance closure

Prerequisite: 07 complete.

Status: Complete (2026-09-30). Checkpoint 09 not started.

Planning base: main at 55ab664edb1c3022cc73937ccb778f97fab6f227 on 2026-09-29.

Checkpoint 07 completed the outer product convergence. At this baseline, the architecture migration itself is no longer the work: native Calendar, hosted Views, Contacts, Attention and Wellbeing already enter through one App ConnectionObserveOperation; source configuration is Connections-owned; Feasibility is separate contextual Access; the old standing Calendar/Personal product DTOs and Flutter gateways are gone.

Checkpoint 08 is therefore a final semantic garbage-collection and regression-fencing checkpoint. It must remove the remaining misleading representations, collapse duplicate policy-composition helpers, narrow caller-zero public/dependency surface, delete migration-history tests that no longer protect a current invariant, add a focused source-semantic conformance checker, and add one cross-owner regression that proves the original failure mode cannot return.

Line numbers below are planning-base anchors on 55ab664e. Re-resolve symbols on the actual execution HEAD before editing.

## 1. Reconciled baseline: do not redo completed checkpoints

The following deletion items from the earlier version of this plan are already complete and must not be reimplemented:

- crates/modules/access/src/application/remote_calendar.rs: absent;
- crates/adapters/vault/src/vault/remote_calendar_grants.rs: absent;
- crates/adapters/vault/src/vault/calendar_grant_policy.rs: absent;
- crates/modules/access/src/application/calendar_lease.rs: absent;
- apps/client/lib/features/connections/application/native_calendar_access_gateway.dart: absent;
- apps/client/lib/features/connections/domain/native_calendar_access.dart: absent;
- production CalendarAccessChange / CalendarAccessOverview / CalendarAccessState / CalendarAccessConfiguration: absent;
- production PersonalAccessChange / ContactsAccessChange / PersonalAccessOverview: absent;
- remote product ConnectionObserve resource argument: absent;
- selected_resources / granted_resources standing projection: absent from current production Observe path;
- ConsumerPolicyAuthority and remote Calendar special grant/read stack: already removed by 04/05;
- _observeResource: already absent from production.

The similarly named Context file crates/modules/context/src/application/calendar_lease.rs is current Context provenance code, not the removed Access legacy module. Do not delete it on name similarity.

The current native implementation files remain legitimate:
- crates/app/src/vault_host/calendar_access.rs contains native Calendar subject/grant implementation behind common ConnectionObserve;
- crates/app/src/vault_host/personal_access.rs contains Contacts/Attention/Wellbeing implementation behind common ConnectionObserve;
- crates/app/src/vault_host/remote_observe.rs contains hosted provider review/admission internals behind common ConnectionObserve.

08 does not replace those owner implementations with another abstraction.

## 2. Frozen 08 exit state

08 is complete only when all of the following are true.

1. There is one canonical first-party Observe policy-composition function per supported connector/View semantics; no target/binding-shaped wrapper computes a different consumer set.
2. Policy composition depends only on shipped product capability declarations plus explicit Manager direct-read policy, never current Registry assignment/binding state, Person-specific Vault state, selected source resources or connection ID.
3. Native Calendar, remote and personal standing Observe continue to use the existing ConnectionObserveOperation product boundary from 07.
4. Expert candidates remain connection/View-level; no Calendar leaf loop creates candidates or grant scope.
5. Source edits still advance SourceAuthority without mutating GrantAuthority or Expert binding.
6. Third-party/installed extension state cannot change default first-party Observe consumers or policy digest.
7. Old migration-only helper names, stale comments, test module names and duplicate negative fixtures are removed where they now misdescribe current architecture.
8. Current strict wire rejection tests remain only where they protect a live contract invariant; migration-history enumeration is not preserved as an API compatibility suite.
9. Old Vault schema detection may remain only as fail-closed rejection. No old table is decoded, read as authority or migrated.
10. Public exports and Cargo dependencies are no wider than current legitimate cross-crate callers require.
11. A source-level architecture checker mechanically rejects the principal Observe regressions.
12. One integrated App-level regression proves 11 Calendar resources, one logical grant/binding, third-party denial, stable grant/binding across source edit, and stale old evidence.
13. 09 remains Not started.

## 3. Explicit legitimate survivors

Do not over-delete these current concepts.

### Provider/source leaf values

calendar_ids may remain in:
- Connections Calendar source configuration;
- native/provider acquisition;
- hosted signed source provenance;
- server connector scope;
- test fixtures for those current paths.

selected_handles may remain in:
- Contacts Connections source setup;
- native Contacts subject acquisition;
- provenance/query fixtures.

They must not appear as standing ConnectionObserve permission inputs.

### Logical reviewed resource

ConnectionObserveReviewedMember.resource is the canonical logical connection/View resource and is valid. The forbidden regression is an outer provider-leaf selector, not the logical reviewed member resource.

### Remote internal evidence

crates/app/src/remote_services.rs:157 RemoteConnectionObserveExpectation is crate-private provider-attested internal evidence with live routing/provider fields that the product-reviewed expectation intentionally omits. Keep it if it still represents that real adapter boundary. Delete or merge it only if the implementation proves it has become a pure duplicate; do not force product routing facts into ConnectionObserveExpectation to make one struct.

### Fail-closed old-schema sentinels

Current deliberate examples:
- crates/adapters/vault/src/vault.rs:315,334 reject_obsolete_policy_schemas;
- crates/adapters/vault/src/vault/calendar_grants.rs:765 obsolete_policy_tables_are_rejected_on_reopen;
- crates/adapters/vault/src/vault/feasibility_reviews.rs:556 obsolete_personal_schema_fails_closed_on_reopen.

These are not compatibility decoders. Preserve them unless the owning Vault design is deliberately changed with equivalent fail-closed behavior. A legacy table name in this bounded negative guard is an allowed residual.

### System permission terminology

Flutter CalendarSystemAccess or similar OS-permission types are source/system permission state, not the deleted Calendar standing grant product API. Do not rename them merely because they contain CalendarAccess.

## 4. 08-A — Collapse first-party Observe policy composition to one canonical path

The largest current production residual is crates/app/src/first_party_observe.rs.

Planning-base anchors:
- member_policy_digest: lines 98-116;
- personal_policy: 118-141;
- calendar_policy: 143-150;
- remote_policies: 152-172;
- remote_policies_for_target: 174-194;
- remote_member_policy_digest_for_target: 197-210;
- native_consumers_for_target: 212 onward;
- native_member_policy_digest_for_target: around 227 onward.

Current problem:

remote_policies builds a base policy with Manager consumers, while remote_policies_for_target adds trusted shipped consumers. The target wrapper accepts Vault, Person and connection identity even though policy is a shipped product rule, not target-owned authority. member_policy_digest can therefore describe a different prospective policy from the final remote review path. The *_for_target naming also preserves the exact Registry/leaf-target conceptual shape this refactor removed.

### Final shape

1. Make one canonical remote policy constructor that returns the full final consumer set:
   - Manager assistant only when manager_direct_remote_view(view_id) is true;
   - trusted shipped Expert consumers declared for that View;
   - sorted, unique;
   - same categories/operation/purpose/processing used by grant activation.

2. Use that canonical function for:
   - supported-connector detection;
   - ConnectionObserve overview;
   - review expectation construction;
   - policy digest;
   - interaction capture/refresh;
   - remote grant review/activation.

3. Delete remote_policies_for_target and remote_member_policy_digest_for_target.

4. Rewrite member_policy_digest so every supported standing connector resolves through the exact canonical final policy. There must be no digest helper that hashes a pre-shipped-consumer template while activation uses a wider consumer set.

5. Delete native_member_policy_digest_for_target if its only remaining work is Vault Person/device validation plus policy_digest(personal_policy). Source/Person/device validation belongs to the live owner path; product policy composition must remain identity-neutral.

6. native_consumers_for_target currently survives primarily to inject the Feasibility assistant consumer. Replace it with a Feasibility-specific helper or direct canonical Feasibility consumer construction at the Feasibility owner boundary. Do not retain a generic standing-native target helper for one contextual exception.

7. Remove imports that become unnecessary, especially PersonId/VaultKeyProvider dependencies inside first_party_observe if no policy function legitimately needs live Vault state.

### Callers to cut directly

Re-resolve and update:
- crates/app/src/vault_host/remote_observe.rs, especially product_overview around lines 95-150 and review/enable helpers around 245 onward;
- crates/app/src/vault_host/interaction_owners.rs around lines 520-660 and any later remote policy/digest refresh;
- crates/app/src/vault_host/review_snapshot.rs remote capture;
- crates/app/src/vault_host.rs WorkerAction::ConnectionObserve dispatch around 2417-2577;
- crates/app/src/vault_host.rs Feasibility command preparation immediately after that block;
- crates/app/src/vault_host/tests/interaction_resolution.rs test helpers;
- crates/app/src/vault_host/tests/registered_runner.rs policy tests.

Do not introduce another canonical_final_policies_for_target wrapper. Final policy composition must be target-independent.

### Tests

Rewrite first_party_observe tests so they prove:
- remote canonical policy contains exactly Manager + trusted shipped consumers;
- calendar remote/native policy produces the same digest for the same logical View policy;
- arbitrary Registry install/binding changes do not change policy or digest;
- extension package IDs never enter default consumers;
- member_policy_digest equals policy_digest of the exact activation policy for every supported connector/View.

Suggested commit:

~~~
app: collapse first-party observe policy composition
~~~

## 5. 08-B — Delete stale migration-history tests and rename misleading test surfaces

The goal is not to erase useful negative tests. It is to stop carrying the migration itself as a supported contract.

### App native Calendar test module

Current file:
- crates/app/src/vault_host/tests/native_calendar_access.rs
- module declaration crates/app/src/vault_host.rs:3135.

The file now tests common ConnectionObserve semantics, not a Calendar Access product API.

Rename with git mv to a current-semantic name such as:
- crates/app/src/vault_host/tests/calendar_connection_observe.rs

Update the module declaration accordingly.

Retain tests around:
- pause without source mutation;
- stale grant/source/subject/identity rejection;
- 11-resource review/enable;
- logical resource only;
- grant continuity across resource edit.

Do not preserve old naming solely because historical checkpoints referenced it.

### Protocol strictness tests

Current useful anti-regression tests:
- crates/bindings/protocol/src/dto/connection_observe.rs:179 onward;
- observe_wire_rejects_leaf_and_routing_authority_fields around 197;
- native_and_hosted_observe_share_one_exact_wire around 220;
- overview_rejects_old_selected_granted_projection around 303.

Keep tests that state a current invariant:
- ConnectionObserve accepts connector+connection and exact backend expectation only;
- leaf Calendar IDs / Contacts handles / remote routing fields are rejected;
- selected/granted dual projection is not a current DTO;
- enable without exact expectation is invalid.

Current migration-history suites:
- crates/bindings/protocol/tests/local_owner_wire.rs:293-330 enumerates prior access.personal/access.contacts/access.calendar kinds;
- crates/bindings/protocol/tests/remote_wire.rs:35 onward enumerates obsolete remote Observe and old remote_calendar_grant_* operation names.

Converge these:
1. retain one generic unknown-kind strict-decoder test per owner envelope;
2. retain explicit forbidden-field tests on the current ConnectionObserve DTO where the semantic regression matters;
3. delete long lists whose only purpose is remembering every prior operation spelling;
4. do not add aliases or legacy enums to make those fixtures compile.

### Flutter tests

07 already deleted apps/client/test/features/connections/native_calendar_access_test.dart.

Audit remaining connection tests for obsolete mock API names and duplicate parser rejection fixtures:
- connector_screen_test.dart;
- server_connector_panel_test.dart;
- connection_observe_gateway_test.dart;
- personal source/connection tests.

Keep behavioral tests for current UI and source/Observe separation. Delete fixtures that only emulate removed gateway classes or old field sets.

### Vault negative schemas

Keep the fail-closed old-schema tests called out in section 3. They prove absence of a silent old-authority fallback rather than backward compatibility.

Suggested commit:

~~~
tests: remove observe migration-history fixtures
~~~

## 6. 08-C — Public API and dependency narrowing

This is an audit with deletion gates, not a requirement to invent dependency changes.

### Public Rust exports

Inspect:
- crates/app/src/lib.rs, especially ConnectionObserve exports around the current connection_observe block and source command exports;
- crates/modules/access/src/lib.rs;
- crates/modules/context/src/lib.rs;
- crates/bindings/protocol/src/lib.rs and dto/mod.rs;
- crates/bindings/ffi/src/lib.rs.

For every pub symbol touched by 00-07:
1. use rg to enumerate cross-crate callers;
2. if all callers are in the defining crate, narrow to pub(crate) or private;
3. if the symbol exists only for tests, move/re-export under cfg(test) rather than keeping production public;
4. delete old alias/re-export groups rather than leaving a public compatibility name.

Do not narrow legitimate protocol/FFI boundary types solely to reduce count. ConnectionObserve DTOs and App boundary values used by floe-ffi are real public boundaries.

### Cargo dependencies

Inspect actual manifests and source references:
- crates/app/Cargo.toml;
- crates/modules/access/Cargo.toml;
- crates/modules/context/Cargo.toml;
- crates/modules/day/Cargo.toml;
- crates/modules/experts/Cargo.toml;
- any adapter/binding manifest modified by cleanup.

Use:
~~~
cargo metadata --no-deps --format-version 1
rg -n "floe_[a-z0-9_]+::" <crate source>
python3 tools/architecture/check_boundaries.py
~~~

Remove a dependency only when no production caller remains. Update tools/architecture/module-dependencies.json only when an actually removed direct dependency changes the allowed policy. Do not edit allowed edges merely to make the policy look smaller when the manifest still uses them.

Known non-targets at this baseline:
- App still legitimately composes Experts, Day, Access, Context and Connections elsewhere;
- Context still legitimately uses Day calendar values and Access authority;
- Access dev/test wiring to Context may remain if tests genuinely need it.

Record zero dependency removals as a valid result if the audit proves every current edge is legitimate.

### Misleading helper/export names

If ATTENTION_ASSISTANT_CONSUMER or another old Personal-specific export is proven to be a general Manager consumer constant with cross-crate callers, rename only if the new name removes a real semantic lie and all callers can cut over directly. Do not broaden 08 into cosmetic naming churn.

Suggested commit:

~~~
refactor: narrow observe public and dependency surface
~~~

## 7. 08-D — Add a focused ConnectionObserve conformance checker

Add:

- tools/architecture/check_connection_observe_conformance.py
- tools/architecture/test_check_connection_observe_conformance.py

Follow the existing Violation/check_tree/CLI style used by:
- tools/architecture/check_expert_extensibility.py;
- tools/architecture/test_check_expert_extensibility.py.

The checker must inspect source semantics, not line numbers.

### Required production rules

At minimum reject:

1. legacy Observe authority symbols in production:
   - ConsumerPolicyAuthority;
   - selected_shipped_consumers;
   - native_calendar_policy_for_target;
   - remote_policies_for_target;
   - remote_member_policy_digest_for_target;
   - native_member_policy_digest_for_target after 08-A removes them;
   - RemoteCalendarQuery;
   - SignedCalendarPreview;
   - RemoteCalendarSourceReference;
   - remote Calendar grant mapping/product symbols already removed.

2. forbidden standing product fields:
   - calendar_ids or selected_handles inside App ConnectionObserveOperation / protocol ConnectionObserve mutation/review input;
   - outer resource selector on ConnectionObserve;
   - selected_resources/granted_resources on current Observe overview;
   - consumer_policy/policy_authority fields.

3. first-party policy dependence on mutable Expert state:
   - production portion of crates/app/src/first_party_observe.rs must not reference AgentRegistry, assignment, binding-selected resources or arbitrary installed extension IDs;
   - floe_experts_builtin::manifests remains allowed because shipped manifests are the product policy source.

4. Calendar candidate leaf regression:
   - the calendar.timeline branch of crates/modules/context/src/application/source_candidates.rs must create one candidate per serving SourceConnection using connection_view_resource;
   - it must not iterate connection.resources to emit one candidate per Calendar leaf.

5. product gateway regression:
   - apps/client/lib/features/connections/application/connection_observe_gateway.dart must not expose calendarIds, selectedHandles or an outer resource parameter.

### Checker exclusions

- ignore docs and the temporary execution-plan bundle;
- distinguish production from Rust cfg(test), using the same production-line approach as check_expert_extensibility;
- allow negative protocol tests to contain forbidden old field strings;
- allow provider/source configuration and Context acquisition to contain calendar_ids / selected_handles outside the standing product contract;
- allow logical reviewed member resource.

### Checker regression fixtures

The test file must contain one failing fixture for every rule family and at least one allowed fixture proving:
- source config may carry calendar_ids/selected_handles;
- ConnectionObserve reviewed member may carry logical resource;
- shipped manifest lookup is allowed;
- test-only negative literals do not fail production scan.

### Make the checker discoverable

Update the structural verification references in the same checkpoint:
- README.md around lines 39-46;
- docs/architecture/README.md source-of-truth rules around lines 52-58;
- .agents/skills/code-change-verification/SKILL.md architecture gate/completion sections;
- AGENTS.md architecture-convergence section if needed so future architecture work knows the checker exists.

Do not duplicate its rule list into all docs. Name the checker and its purpose; the script owns exact machine rules.

Suggested commit:

~~~
architecture: enforce connection observe conformance
~~~

## 8. 08-E — One cross-owner conformance regression

Add one integrated regression as close to App runtime as practical.

Preferred home:
- a new focused test in crates/app/src/vault_host/tests/connection_observe_conformance.rs, or
- registered_runner.rs only if reusing its large fixture clearly keeps the test smaller.

Do not make an already oversized unrelated test file substantially harder to navigate solely to avoid one new module.

### Required scenario

Build one native EventKit SourceConnection with eleven resources and one stable connection ID.

Then prove in one scenario:

1. source candidate discovery returns exactly one calendar.timeline connection/View candidate;
2. save that candidate as an Expert binding;
3. establish one active standing grant whose GrantScope contains exactly calendar.timeline:<connection>;
4. first-party policy contains shipped Calendar Experts only and excludes an arbitrary extension package;
5. an arbitrary installed extension/binding cannot enter default grant consumers;
6. acquire/read current Calendar Context using all eleven physical resources;
7. dependency permission resources contain the one logical View while source_resources contain all eleven leaves;
8. change the same SourceConnection resource set, e.g. 11 -> 12, through Connections;
9. SourceAuthority advances;
10. GrantId remains identical;
11. GrantAuthority remains identical because permission policy is unchanged;
12. Expert candidate ID remains identical;
13. saved Expert binding revision remains identical;
14. old dependency reauthorization fails because SourceAuthority/source_resources changed;
15. a new read uses all twelve current resources.

This test should compose existing real owner APIs rather than duplicate their implementation.

Reuse proven fixtures/helpers from:
- crates/app/src/vault_host/tests/native_calendar_access.rs (to be renamed), especially common_observe_reviews_eleven_calendars_without_leaf_permission_input around current line 294;
- crates/app/src/vault_host/tests/registered_runner.rs contacts source-edit/binding test;
- crates/modules/context/tests/native_calendar_read.rs current_resource_growth_reads_all_calendars_and_stales_old_dependency;
- crates/modules/context/src/application/source_candidates.rs calendar.timeline candidate path around 163 onward.

If crossing Context with the full OpenVault fixture becomes disproportionately complex, keep the App integration test responsible for Connections + Access + Experts and call the existing Context regression as a required paired acceptance test. Do not invent a forwarding integration abstraction solely for the test.

### Third-party denial

The test must prove denial from product policy, not merely that an extension is disabled. Use an installed/supplied extension with Calendar capability and show its package ID is absent from the canonical first-party consumers/digest.

Suggested commit:

~~~
test: lock connection observe cross-owner invariants
~~~

## 9. 08-F — Residual audit with current classifications

Run concept searches after implementation.

### Production-deletion searches

~~~
rg -n "ConsumerPolicyAuthority|consumer_policy|policy_authority|policy_incarnation|policy_epoch" crates server apps/client/lib
rg -n "RemoteCalendarQuery|SignedCalendarPreview|RemoteCalendarSourceReference|calendar_source_preview" crates server apps/client/lib
rg -n "selected_shipped_consumers|native_calendar_policy_for_target|remote_policies_for_target|remote_member_policy_digest_for_target|native_member_policy_digest_for_target" crates
rg -n "CalendarAccessChange|CalendarAccessOverview|PersonalAccessChange|ContactsAccessChange|PersonalAccessOverview" crates apps/client/lib
rg -n "granted_resources|grantedResources|_observeResource" crates apps/client/lib
rg -n "connectionObserve.*resource|ConnectionObserve.*calendar_ids|ConnectionObserve.*selected_handles" crates apps/client/lib
rg -n "remote_calendar_grant|calendar_grant_policy" crates server apps/client/lib
~~~

Production matches must be zero except the explicitly classified Vault stale-schema rejection strings where the search term is a historical table name.

### Semantic searches that require classification

~~~
rg -n "calendar_ids" crates server apps/client/lib
rg -n "selected_handles" crates server apps/client/lib
rg -n "source_resources" crates server apps/client/lib
rg -n "RemoteConnectionObserveExpectation" crates/app
rg -n "calendar_lease" crates
rg -n "calendar_access" crates/app apps/client/lib
~~~

Classify every match:
- source configuration;
- provider acquisition;
- Context provenance;
- logical reviewed member;
- fail-closed stale-schema rejection;
- legitimate internal native/remote implementation;
- blocker.

There is no "legacy but harmless" production bucket.

### Historical docs

Completed checkpoint documents 00-07 may contain old symbol names as historical execution context. Do not churn those documents in 08 solely to make global rg zero. The active 08 residual report must distinguish docs/history from current production. 09 owns final plan lifecycle and ADR/document convergence.

## 10. Test deletion accounting

Record:
- test files renamed;
- tests deleted because they enumerated migration history;
- strict current-contract tests retained;
- new checker tests;
- new cross-owner regression.

A lower test count is not a failure if deleted tests only protected removed behavior. Do not weaken current safety assertions to reduce residual matches.

## 11. Verification during implementation

Run the narrowest checks after each slice.

### Policy composition

~~~
cargo test -p floe-app first_party_observe
cargo test -p floe-app interaction_resolution -- --test-threads=1
cargo test -p floe-app registered_runner -- --test-threads=1
~~~

If a filter matches zero, run the nearest full App target and record it.

### Protocol/test cleanup

~~~
cargo test -p floe-protocol
cargo test -p floe-ffi
~~~

### Architecture checker

~~~
python3 tools/architecture/test_check_connection_observe_conformance.py
python3 tools/architecture/check_connection_observe_conformance.py
python3 tools/architecture/test_check_boundaries.py
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/test_check_expert_extensibility.py
python3 tools/architecture/check_expert_extensibility.py
~~~

### Cross-owner regressions

At minimum run named/focused tests covering:
- common_observe_reviews_eleven_calendars_without_leaf_permission_input or its renamed replacement;
- contacts_source_edit_keeps_saved_expert_binding_and_logical_grant;
- current_resource_growth_reads_all_calendars_and_stales_old_dependency;
- the new 08 cross-owner conformance test.

### Rust/public surface

~~~
cargo check --workspace --lib
git diff --check
~~~

Run cargo test for every crate whose public exports or manifest changed.

### Flutter

If Flutter tests/fixtures or architecture-tool references change:

~~~
cd apps/client
flutter analyze
flutter test test/features/connections
~~~

Full Flutter test/build remains mandatory in 09, but run full flutter test in 08 if production Flutter code changes rather than test-only cleanup.

### Server

08 does not require server behavior changes at this baseline. If residual audit deletes or modifies server code, run from server:

~~~
go test ./...
go test -race ./...
go vet ./...
~~~

Otherwise record server production residual search as clean/classified and leave full server verification to 09.

### Broad Rust close gate

After all 08 production/public/checker changes:

~~~
CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast -- --test-threads=1
~~~

The broad gate is required because the checkpoint may narrow public exports/dependencies and adds a cross-owner regression. 09 will repeat final repository verification after documentation/ADR convergence.

## 12. Documentation scope in 08

Do not rewrite the architecture story again if code semantics remain the 07 final state.

Update only:
- architecture-tool references required to make the new checker discoverable;
- current architecture text if 08-A actually changes the described policy-composition path;
- this execution plan and parent README checkpoint status.

Do not add the final ADR here unless implementation discovers a new durable decision. 09 owns the planned ADR convergence and final documentation sweep.

## 13. Suggested implementation slices

### 08-A — canonical policy cleanup

- collapse remote/base/target policy composition;
- delete *_for_target policy wrappers;
- narrow Feasibility consumer helper;
- update direct callers/tests.

Commit:

~~~
app: remove target-shaped observe policy helpers
~~~

### 08-B — test/fixture purge

- rename native Calendar common Observe test module;
- delete migration-history operation enumerations;
- retain semantic strictness and Vault fail-closed guards.

Commit:

~~~
tests: purge obsolete observe migration fixtures
~~~

### 08-C — public/dependency audit

- narrow caller-zero pub exports;
- remove proven unused dependencies;
- update dependency policy only for actual graph changes.

Commit:

~~~
refactor: narrow observe dependency surface
~~~

Skip this commit if the audit produces no code change; record zero removals rather than creating churn.

### 08-D — conformance checker

- add checker + checker tests;
- wire it into structural verification references.

Commit:

~~~
architecture: enforce connection observe invariants
~~~

### 08-E — integrated regression

- add cross-owner Calendar resource/grant/binding/provenance regression.

Commit:

~~~
test: lock connection observe conformance
~~~

### 08-F — closure

- residual searches;
- execution evidence;
- README 08 Complete / 09 Not started;
- required verification.

Commit:

~~~
docs: complete connection observe checkpoint 08
~~~

Combine slices where the final code becomes smaller. Do not add temporary adapters or aliases to preserve intermediate compilation.

## 14. Close procedure

Before marking 08 Complete:

1. fetch origin/main and record start/final revisions;
2. prove the 07 common ConnectionObserve product path remains the only standing product path;
3. prove first-party policy has one canonical final composition per connector/View;
4. prove no target/Registry/binding/source-resource input affects first-party consumer policy;
5. prove all *_for_target Observe policy wrappers owned by the old design are deleted;
6. prove old Calendar/Personal product symbols remain absent from production;
7. prove remote Calendar special grant/read symbols remain absent;
8. prove Calendar candidate discovery is connection/View-level;
9. prove ConnectionObserve wire has no provider leaf selector;
10. prove negative migration-history tests were removed/consolidated while current strictness remains;
11. prove Vault old-schema matches are fail-closed rejection only;
12. inspect public exports and Cargo dependencies and record each removal or why none were removable;
13. run the new conformance checker and its negative fixtures;
14. run existing boundary and Expert extensibility checkers and their tests;
15. run the integrated 11->12 Calendar regression;
16. run targeted Rust/protocol/FFI checks;
17. run Flutter checks if affected;
18. run server checks if affected;
19. run broad serialized Rust workspace tests;
20. run all residual searches and classify every surviving production match;
21. update only required tooling/current docs;
22. append execution evidence to this file;
23. update parent README:
    - 00 Complete
    - 01 Complete
    - 02 Complete
    - 03 Complete
    - 04 Complete
    - 05 Complete
    - 06 Complete
    - 07 Complete
    - 08 Complete
    - 09 Not started
24. commit closure;
25. stop. Do not begin Checkpoint 09.

## 15. Required execution evidence

Record:

1. date;
2. start local HEAD and first/final fetched origin/main;
3. implementation and closure SHAs;
4. final first-party policy helper surface;
5. deleted target-shaped policy helpers;
6. proof canonical policy/digest equals activation policy for native/remote/personal;
7. proof Registry/extension state cannot affect first-party consumers;
8. old production symbol/file absence;
9. renamed/deleted test files;
10. migration-history tests removed;
11. current strict rejection tests retained;
12. Vault fail-closed old-schema exceptions;
13. public exports narrowed;
14. Cargo dependencies removed, or explicit proof none were removable;
15. module-dependencies policy changes, if any;
16. new conformance checker rules;
17. checker positive/negative fixture results;
18. one-candidate Calendar source discovery proof;
19. 11-resource logical grant/binding proof;
20. 11->12 source edit SourceAuthority proof;
21. unchanged GrantId/GrantAuthority proof;
22. unchanged candidate ID/binding revision proof;
23. old dependency stale/new read current-resources proof;
24. third-party extension denial proof;
25. full residual search classification;
26. architecture/tooling docs changed;
27. targeted App results;
28. Context regression results;
29. protocol/FFI results;
30. architecture checker results;
31. Flutter results or not affected;
32. server results or not affected;
33. broad serialized Rust result;
34. final clean worktree;
35. confirmation Checkpoint 09 was not started.

## 16. Required agent report

Report only:

1. start HEAD / first and final fetched origin-main / final HEAD;
2. checkpoint commit SHAs by slice;
3. final canonical first-party policy helper shape;
4. deleted *_for_target and legacy policy helpers;
5. proof final policy digest matches exact grant activation policy;
6. proof Registry/third-party state does not affect default consumers;
7. production legacy file/symbol deletion audit;
8. test files renamed/deleted and why;
9. current negative safety/strictness tests retained;
10. Vault old-schema fail-closed exceptions;
11. public exports narrowed;
12. dependencies removed or zero-removal audit result;
13. dependency-policy changes, if any;
14. new conformance checker rules;
15. checker tests/results;
16. integrated Calendar candidate/grant/binding scenario;
17. 11->12 SourceAuthority result;
18. GrantId/GrantAuthority continuity result;
19. candidate ID/binding revision continuity result;
20. old dependency stale/new read result;
21. third-party denial result;
22. residual search/classification;
23. tooling/current docs changed;
24. targeted App verification;
25. Context verification;
26. protocol/FFI verification;
27. Flutter verification or not affected;
28. server verification or not affected;
29. broad serialized Rust verification;
30. clean worktree and confirmation Checkpoint 09 was not started.

## 17. Execution evidence (2026-09-30)

1. Start local HEAD and first fetched `origin/main`: `4e50769565e28eb58dc5ad87c0b56dfd5daa13fc`; final fetched `origin/main` remained the same. The start worktree was clean `main`.
2. Implementation commits: `ffbb317a` policy convergence and Calendar test-module rename; `0e6cef8f` migration-fixture purge; `49c2248d` checker/tool references; `581e21ab` and `3e4f79be` public narrowing; `85202cca` Calendar conformance regressions; `44b47d63` exact digest coverage; `e4156f51`, `0334d10a`, and `647e6a88` checker strengthening. Closure commit is recorded in the follow-up evidence line below.
3. Final App policy surface is `trusted_shipped_consumers`, `personal_policy`, `calendar_policy`, `remote_policies`, `member_policy_digest`, and `policy_digest`. Remote policy construction now includes the complete sorted/deduplicated Manager-plus-shipped consumer set before any caller receives it; native Calendar and personal policies are likewise identity-neutral.
4. Deleted `remote_policies_for_target`, `remote_member_policy_digest_for_target`, `native_member_policy_digest_for_target`, and `native_consumers_for_target`. Feasibility constructs its contextual assistant consumer at its owner dispatch rather than using a standing-native helper.
5. `member_policy_digest` hashes the same canonical policy object used by native Calendar, remote and personal review/activation. The supported-remote and Apple-personal digest test and the Calendar activation regression pass; remote review and enable both call `remote_policies`, and enable compares the reviewed digest to `policy_digest(policy)` before mutation.
6. `first_party_observe` production has no Vault, Person, connection, Registry, assignment, binding or selected-resource input. An installed Calendar-capable `example.calendar.extension` remains absent from the canonical grant consumers; both the new Calendar scenario and existing Registry policy tests prove this.
7. Production searches found no removed Calendar/Personal product DTOs, remote Calendar grant/read symbols, target policy wrappers, `ConsumerPolicyAuthority`, selected/granted standing projections or provider-leaf outer Observe selector. The current `calendar_access.rs`, `personal_access.rs` and `remote_observe.rs` remain legitimate internal implementations behind common `ConnectionObserve`.
8. Renamed `native_calendar_access.rs` to `calendar_connection_observe.rs`. The old local owner and remote wire migration-history operation enumerations were replaced by one generic unknown-kind rejection per envelope, not compatibility aliases. The 11-Calendar test was extended and renamed as the 08 conformance scenario.
9. Retained current protocol strictness tests for leaf/routing field rejection, exact native/hosted wire, enable expectation and selected/granted projection rejection. Retained Vault reopen rejection of obsolete Calendar/Personal schemas; no old table is decoded or migrated.
10. Vault residual `calendar_grant_policy_schema`/`calendar_grant_policies`/`calendar_grant_mappings` strings occur only in fail-closed stale-schema detection and negative reopen tests.
11. Narrowed caller-zero `validate_personal_source_selection` from public Context export to `pub(crate)`, and `RemoteViewGrantReview`/`review_remote_view_grant` to Access-internal symbols. Cross-crate references and exposed return signatures justify the remaining touched App/Access/Context/Protocol/FFI exports, including actual FFI ConnectionObserve DTOs and source commands.
12. Removed no Cargo dependency: `cargo metadata` and production `floe_*` source-reference audit found every normal dependency in App, Access, Context, Day and Experts still used. The 105-edge dependency policy therefore did not change.
13. Added `check_connection_observe_conformance.py`: production legacy authority symbols and fields; mutable first-party Registry/binding dependence; connection/View Calendar candidate shape; and Flutter gateway leaf selectors. It excludes Rust `cfg(test)` and test files while allowing source configuration/acquisition leaf values and logical reviewed-member resource.
14. Checker fixtures exercise each forbidden rule family and allowed source configuration, logical reviewed resource, shipped manifest lookup, and test-only negative literals. The checker and its 3-test fixture suite pass; boundary and Expert extensibility checkers and their fixture suites pass.
15. The App 11→12 scenario establishes one EventKit SourceConnection, discovers one connection/View candidate, saves one Expert binding, reviews and activates one logical Calendar grant, installs a Calendar-capable arbitrary extension, and verifies that extension is absent from default consumers.
16. A Connections source edit from 11 to 12 resources advances `SourceAuthority` once. The same scenario proves unchanged `GrantId`, `GrantAuthority`, candidate ID and binding revision, with active Observe retained; the old review expectation fails closed.
17. The paired Context regression now reads eleven physical resources under one logical dependency, rejects the old dependency after the edit, and reads all twelve current resources with `source_resources` recording those leaves. This pairing avoids a test-only cross-owner forwarding abstraction.
18. Residual production classifications: `calendar_ids` and `selected_handles` occur in Connections configuration, native/provider acquisition, hosted signed provenance/server scope, Context acquisition, Action source evidence and tests; not standing Observe permission input. `source_resources` is current source/provenance or informational overview data, not a grant-scope selector. `RemoteConnectionObserveExpectation` is crate-private provider evidence with routing facts; `calendar_lease` is current Context provenance; `calendar_access` names current App internals or OS permission state. No blocker or legacy production bucket remains.
19. Updated the checker references in root README, current architecture README/invariants, and verification skill. Updated current modules architecture only for the newly canonical policy-composition path. No ADR or Checkpoint 09 work was started.
20. Targeted App: first-party policy, 53 interaction-resolution tests, 18 registered-runner tests, native Calendar conformance and Contacts continuity passed. Context 11→12 focused regression passed. Protocol and FFI suites passed. `cargo check --workspace --lib` passed.
21. Flutter `flutter analyze` and `flutter test test/features/connections` passed (68 tests). No production Flutter or server code changed; server full tests are reserved for 09. Structural checker suites and `git diff --check` passed.
22. `cargo test -p floe-context` and `cargo test -p floe-access` (41 tests) passed after public export narrowing. The final same-snapshot `CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast -- --test-threads=1` passed across 94 Rust test-result sections, with no failures. Final `cargo check --workspace --lib`, checker suites, boundary policy, and `git diff --check` passed. Checkpoint 09 remains Not started.
23. Closure commit: `4d1ec37fb7844dd7c08d3c378ad8fbd8aee5de36`. Final `origin/main` fetch remained `4e50769565e28eb58dc5ad87c0b56dfd5daa13fc`; the final committed worktree is clean. No push, deployment, external-account change or Checkpoint 09 implementation occurred.
