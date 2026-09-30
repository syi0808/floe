# 09: Full verification, durable documentation and ADR convergence

Prerequisite: 08 complete.

Status: Incomplete — native bundle fixture passes the three Day scenarios; the required Action evidence-fence assertion fails against the current user-proposal contract. Golden tests remain excluded from the 09 completion gate.

Planning base: main at `a4f292cb77bf3dc6a3593c035fc80a5245c79645` on 2026-09-30.

Checkpoint 08 closed the last implementation/deletion/conformance work. Checkpoint 09 is not another architecture migration. Its job is to prove the frozen final architecture from one source snapshot, remove stale current-runbook statements, record the durable rationale in ADR 0031, and leave this temporary execution plan ready for retirement after explicit user acceptance.

Line numbers below are planning-base anchors on `a4f292cb`. Re-resolve every symbol/path on the actual execution HEAD before editing.

## 1. Frozen checkpoint boundary

### 09 owns

- final repository-wide residual audit;
- focused safety/invariant verification;
- architecture/conformance checker verification;
- broad Rust, Go server, FFI, Flutter and Apple/native gates;
- environment-gated loopback/product smoke where the repo already provides disposable validation;
- current architecture/product/runbook documentation convergence;
- a new durable ADR 0031 that amends the stale Observe semantics in ADR 0027, ADR 0028 and ADR 0030;
- final deletion/verification evidence and parent checkpoint status.

### 09 does not own

- a new source/grant/binding architecture;
- a new permission mode or RestrictedSubset feature;
- Android parity work;
- production migration compatibility;
- plan-bundle retirement before explicit user acceptance;
- live external Calendar/contact/health/provider/account mutation without explicit approval;
- rewriting old ADRs to pretend they always described the final design.

### Production-change rule

The expected production-code change count in 09 is zero.

If verification exposes a concrete implementation defect that violates the already-frozen architecture:
1. fix it directly in the existing owner/path;
2. add/update the smallest invariant regression;
3. rerun the affected focused and full gates;
4. record the defect and fix SHA in this plan.

If fixing the issue would require a new owner, second authority, new compatibility path, new permission model, or reopening a completed checkpoint's architecture decision, do not silently redesign in 09. Leave 09 incomplete and report the blocker.

## 2. Final architecture acceptance — no exceptions

The completed repository must prove all of the following simultaneously.

1. Connections owns standing source identity/lifecycle/current physical resources/native subject/current `SourceAuthority`.
2. Access owns standing Observe permission and `GrantAuthority`.
3. Expert binding is connection/View configuration, never standing permission or physical leaf scope.
4. Context reloads current source resources at acquisition and records exact `source_resources` plus `SourceAuthority`.
5. A source/resource/subject change advances `SourceAuthority` but does not inherently change GrantId, `GrantAuthority` or Expert binding.
6. A permission/state/scope policy change advances `GrantAuthority`; no `ConsumerPolicyAuthority` exists.
7. First-party policy is derived only from shipped capability declarations plus explicit Manager direct-read policy.
8. Installed/third-party Expert Registry state cannot change default first-party consumers or policy digest.
9. Native and hosted Calendar grant exactly one logical `calendar.timeline:<connection>` View per connection.
10. Provider leaves remain source configuration/acquisition/provenance, not standing permission inputs.
11. Native Calendar, hosted Views, Contacts, Attention and Wellbeing use the one connection-level `ConnectionObserve` product contract.
12. Feasibility remains contextual and query-bound.
13. Exact-recipient model consent remains contextual Access authority.
14. Observe does not imply Act.
15. Durable interaction/recovery binds immutable reviewed evidence and fails closed on material drift.
16. Remote pairing authenticates the producer/client relationship but does not itself grant connector data access.
17. Existing action durable-intent and uncertain-write recovery invariants remain unchanged.
18. The three architecture checkers pass:
    - `check_boundaries.py`;
    - `check_expert_extensibility.py`;
    - `check_connection_observe_conformance.py`.

## 3. 09-A — Baseline, worktree and current graph

Before any edit:

~~~sh
git fetch origin
git status --short --branch
git rev-parse HEAD
git rev-parse origin/main
git log -1 --oneline
cargo metadata --no-deps --format-version 1
~~~

Requirements:
- local main must be clean;
- local HEAD and fetched `origin/main` must be recorded;
- if `origin/main` moved, rebase/fast-forward according to normal repo practice, then re-resolve this plan;
- do not preserve a local stale planning base by adding compatibility changes.

Also run the current structural tools before documentation edits:

~~~sh
python3 tools/architecture/test_check_boundaries.py
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/test_check_expert_extensibility.py
python3 tools/architecture/check_expert_extensibility.py
python3 tools/architecture/test_check_connection_observe_conformance.py
python3 tools/architecture/check_connection_observe_conformance.py
git diff --check
~~~

This establishes that 09 starts from a structurally valid implementation rather than using documentation changes to mask code drift.

Suggested evidence-only commit: none. Do not create a baseline commit.

## 4. 09-B — Focused invariant matrix

Run named regressions before broad suites so a final-state failure is attributable.

If any filter matches zero tests, run the nearest full crate/test target and record the zero-match fact.

### 4.1 Connection/View identity and source-edit continuity

~~~sh
cargo test -p floe-app calendar_connection_observe_conformance_eleven_to_twelve -- --test-threads=1
cargo test -p floe-context --test native_calendar_read current_resource_growth_reads_all_calendars_and_stales_old_dependency
cargo test -p floe-app common_observe_pauses_without_changing_calendar_source -- --test-threads=1
~~~

These must jointly prove:
- eleven physical Calendar resources -> one candidate;
- one logical grant resource;
- one saved connection/View Expert binding;
- 11 -> 12 changes `SourceAuthority`;
- GrantId / `GrantAuthority` unchanged;
- candidate ID / binding revision unchanged;
- active Observe remains active;
- old reviewed expectation/dependency fails closed;
- next Context read uses all twelve current physical resources.

### 4.2 Native source/subject/generation drift

~~~sh
cargo test -p floe-app common_observe_rejects_stale_grant_source_subject_and_identity -- --test-threads=1
cargo test -p floe-context --test native_calendar_read native_view_rejects_connection_change_after_observation
cargo test -p floe-context --test native_calendar_read native_view_rejects_device_generation_drift
cargo test -p floe-context --test native_calendar_read native_admission_rejects_missing_or_changed_authority
~~~

No stale subject, connection, device generation or source authority may be substituted with latest state.

### 4.3 First-party policy and extension denial

~~~sh
cargo test -p floe-app every_member_digest_matches_the_activation_policy
cargo test -p floe-app policy_never_default_grants_extensions_or_wildcards
cargo test -p floe-app shipped_permission_digest_does_not_depend_on_registry_state -- --test-threads=1
cargo test -p floe-app calendar_consumers_match_trusted_shipped_capability_declarations
~~~

Required semantics:
- policy digest hashes the exact activation policy;
- no Registry assignment/binding/source-resource input affects policy;
- arbitrary installed Calendar-capable extension remains absent;
- Manager assistant appears only for actual Manager direct-read Views/connectors.

### 4.4 Hosted/remote review and drift

~~~sh
cargo test -p floe-app product_review_uses_common_connection_expectation_without_routing_fields
cargo test -p floe-app review_then_enable_binds_gmail_bundle_atomically
cargo test -p floe-app calendar_resource_edit_keeps_logical_grant_without_automatic_review
cargo test -p floe-app stale_descriptor_refuses_without_mutation
cargo test -p floe-app disable_still_pauses_and_disconnects_without_expectations
~~~

Then interaction-level hosted verification:

~~~sh
cargo test -p floe-app gmail_authority_rotation_after_review_supersedes_without_mutation -- --test-threads=1
cargo test -p floe-app gmail_bad_signature_never_mutates_nor_resolves -- --test-threads=1
cargo test -p floe-app remote_calendar_allow_resolves_through_hosted_connection -- --test-threads=1
~~~

Hosted provider identity/routing remains internal signed source evidence. It must not reappear in the product-reviewed permission contract.

### 4.5 Durable interaction/recovery

~~~sh
cargo test -p floe-app fresh_approve_with_drift_supersedes_without_mutation -- --test-threads=1
cargo test -p floe-app native_commit_then_crash_reopens_and_resolves_without_second_advance -- --test-threads=1
cargo test -p floe-app gmail_commit_then_crash_reopens_and_resolves_without_second_mutation -- --test-threads=1
cargo test -p floe-app observer_cancellation_never_revokes_a_recorded_decision -- --test-threads=1
~~~

The review lifecycle must remain Conversation-owned and crash/retry-safe; no latest-state widening or duplicate owner mutation.

### 4.6 Exact-recipient consent

~~~sh
cargo test -p floe-access consent_identity_binds_grant_and_exact_source_resources_and_authority
cargo test -p floe-access changed_review_fields_change_consent_identity
cargo test -p floe-app consent_approve_grants_exact_review_and_resolves -- --test-threads=1
cargo test -p floe-app consent_wrong_device_never_grants -- --test-threads=1
~~~

A source leaf or `SourceAuthority` change must invalidate an old recipient approval even when the standing Observe grant is unchanged.

### 4.7 Actions source fence

~~~sh
cargo test -p floe-app source_resource_change_blocks_old_calendar_receipt_without_mirror_revision_change -- --test-threads=1
cargo test -p floe-app native_executor_uses_rust_ledger_and_lookup_only_after_response_loss -- --test-threads=1
~~~

Observe simplification must not weaken action destination/source fences or uncertain-write recovery.

## 5. 09-C — Final residual audit

Run the production searches from 08 again against the final code snapshot.

~~~sh
rg -n "ConsumerPolicyAuthority|consumer_policy|policy_authority|policy_incarnation|policy_epoch" crates server apps/client/lib
rg -n "RemoteCalendarQuery|SignedCalendarPreview|RemoteCalendarSourceReference|calendar_source_preview" crates server apps/client/lib
rg -n "selected_shipped_consumers|native_calendar_policy_for_target|remote_policies_for_target|remote_member_policy_digest_for_target|native_member_policy_digest_for_target" crates
rg -n "CalendarAccessChange|CalendarAccessOverview|PersonalAccessChange|ContactsAccessChange|PersonalAccessOverview" crates apps/client/lib
rg -n "granted_resources|grantedResources|_observeResource" crates apps/client/lib
rg -n "connectionObserve.*resource|ConnectionObserve.*calendar_ids|ConnectionObserve.*selected_handles" crates apps/client/lib
rg -n "remote_calendar_grant|calendar_grant_policy" crates server apps/client/lib
~~~

Expected production result:
- zero legacy authority/runtime/product matches;
- old Vault table names may survive only inside explicit fail-closed stale-schema detection/tests.

Classify current leaf/provenance concepts:

~~~sh
rg -n "calendar_ids" crates server apps/client/lib
rg -n "selected_handles" crates server apps/client/lib
rg -n "source_resources" crates server apps/client/lib
rg -n "RemoteConnectionObserveExpectation" crates/app
rg -n "calendar_lease" crates
rg -n "calendar_access" crates/app apps/client/lib
~~~

Allowed classes:
- Connections source configuration;
- provider/native/server acquisition;
- Context provenance;
- Actions destination/source evidence;
- logical reviewed member;
- crate-private hosted signed evidence;
- current Context lease/provenance;
- OS/system permission state;
- fail-closed old-schema rejection.

There is no allowed “legacy but harmless” production category.

Also run:

~~~sh
python3 tools/architecture/check_connection_observe_conformance.py
~~~

A passing checker is required in addition to manual classification.

## 6. 09-D — Current documentation audit

Current source and architecture are already expected to express the final model. Update only stale or contradictory current documents; do not create progress prose.

### Inspect first

- `docs/architecture/modules.md`;
- `docs/architecture/runtime.md`;
- `docs/architecture/authority-recovery.md`;
- `docs/architecture/invariants.md`;
- `docs/product/integrations-and-privacy.md`;
- `apps/client/README.md`;
- `server/README.md` if any hosted Calendar/remote authority statement is stale;
- relevant design/runbook docs only if they describe current source/permission behavior.

### Architecture docs

At the planning base, the following current statements are already correct and should normally remain:
- `modules.md:34`: App policy is shipped-capability/Manager-derived and independent of Registry/binding/connection/resources;
- `modules.md:42`: standing grants bind stable source/logical View and SourceAuthority is Connections-owned;
- `modules.md:46`: Registry binding is not source permission;
- `runtime.md:103`: source edits stale review/evidence but do not re-review grant or rebind Expert;
- `authority-recovery.md:21`: source edits are separate Connections commands;
- `authority-recovery.md:27`: `policy_digest` is compare-only, not an authority epoch;
- `integrations-and-privacy.md:19`: Calendar resource edits do not change standing grant or binding.

Do not rewrite these merely to create a 09 diff. Change them only if actual code verification finds an inconsistency or if a cross-link to ADR 0031 materially improves durable navigation.

### Known stale current runbook

`apps/client/README.md` is stale at the planning base:
- line ~186: “Switching calendars replaces the previous mirror” is too narrow/misleading for the current connection resource-set model;
- line ~189: “native gateway still supports one selected calendar” is false;
- line ~199: “exactly one reviewed EventKit calendar” is false for current multi-resource source semantics.

Rewrite that Connected Calendar/runbook section to current truth:
- Connections owns one EventKit SourceConnection with a bounded current resource set;
- multiple selected/current Calendars are supported;
- one Use with Floe grant covers one logical `calendar.timeline:<connection>` View;
- Context reads the current source resource set;
- resource edits advance source authority and stale old evidence without inherently changing the standing grant or Expert binding;
- Calendar Actions remain exact-destination fenced;
- fixture/native validation does not read a personal Calendar unless an explicitly authorized live procedure is run.

Do not copy checkpoint history into the client guide.

### Historical execution plans

00-08 may contain removed names as historical execution evidence. Do not clean them in 09 solely for global search aesthetics; the whole plan directory is retired after explicit acceptance.

## 7. 09-E — ADR 0031: Connection-owned source scope and logical standing Observe

The next available ADR number is confirmed as 0031.

Create:

`docs/decisions/0031-connection-owned-source-scope-and-logical-observe.md`

Recommended header:

~~~md
# ADR 0031: Connection-owned source scope and logical standing Observe

- **Status:** accepted
- **Date:** 2026-09-30
- **Amends:** ADR 0027 connection authority/observation, ADR 0028 connection-scoped permission presentation, ADR 0030 Observe reviewed-target semantics
- **Extends:** ADR 0029 pre-stable architecture convergence
~~~

Use “Amends”, not “Supersedes”: the identity, pairing, revocation, recovery and durable-interaction foundations remain valid.

### Required context

Explain the original failure class without turning the ADR into an implementation report:
- Calendar leaf resources leaked into connection scope, Expert selection, standing permission and acquisition;
- per-leaf consumer composition could collapse or overgrant;
- copied leaf permission scope made ordinary source-resource edits look like permission changes;
- a separate consumer-policy authority duplicated the actual permission/source epochs.

### Required decision

Record these durable decisions:

1. Connections owns the current source resource set and trusted native subject.
2. `SourceAuthority` is the source/resource/subject epoch.
3. Access standing Observe binds stable Person/Connection/Connector/execution-owner source identity plus one or more logical connection/View resources.
4. `GrantAuthority` is the only standing permission/state/scope epoch.
5. No `ConsumerPolicyAuthority`; product policy drift is detected by deterministic compare-only `policy_digest`.
6. Provider leaves are source configuration/acquisition/provenance, not standing permission identity.
7. Expert binding selects connection/View configuration, not provider leaves and not permission.
8. Source resource change does not by itself mutate GrantId, `GrantAuthority` or Expert binding.
9. Old dependency/review evidence fails closed through `SourceAuthority`, exact `source_resources`, subject/producer and grant checks.
10. First-party consumers derive from trusted shipped capability declarations plus explicit Manager direct-read policy; installed third-party/Registry state cannot widen them.
11. Use with Floe is one connection-level Access permission control; source editors remain Connections operations.
12. Feasibility, exact-recipient processing consent and Act authority remain separate contextual authorities.

### Explicit amendments to older ADR wording

ADR 0027 line ~25:
- “processing/consumer/action policy epochs” is historical.
- 0031 clarifies there is no standing consumer-policy epoch; standing permission uses `GrantAuthority`, source truth uses `SourceAuthority`, and `policy_digest` is compare-only.

ADR 0028 lines ~85, ~98, ~120-121:
- the connection UI may display the exact current source resource set;
- that physical set is not copied into standing `GrantScope.resources`;
- an active resource edit updates Connections and advances source authority;
- it does not automatically expand/re-review the standing logical grant solely because leaves changed;
- Off/On or an actual permission-policy change invokes fresh grant review.

ADR 0030 lines ~42-44:
- durable Observe targets no longer carry a separate “policy authority”;
- durable review binds source/connection revision as applicable, subject/producer evidence, canonical logical member resources, compare-only policy digest and exact expected grant state/absence;
- exact physical source resources belong to acquisition/provenance and exact-recipient review where applicable, not copied standing permission scope.

### Consequences

Record:
- simpler authority model: SourceAuthority + GrantAuthority;
- source edits stale evidence without forcing grant/binding churn;
- provider drift remains fail closed;
- no implicit third-party authorization;
- product can support many source leaves behind one logical permission while retaining exact provenance;
- RestrictedSubset, if ever needed, is a deliberate future product/authority mode, not accidental Expert leaf selection.

### Alternatives rejected

At minimum:
- union/intersection of per-leaf Expert consumers;
- copying current leaf resources into every standing grant;
- auto-advancing/reviewing grants on every source edit;
- auto-rebinding Experts after source edits;
- keeping a separate consumer-policy authority;
- treating Expert binding as permission.

### ADR index

Update `docs/decisions/README.md`:
- add ADR 0031 under Device context, connections and authority;
- make 0027/0028 entries point readers to 0031 for amended standing Observe/resource semantics;
- add 0030 cross-reference only if needed for durable review amendment.

Do not modify the body text of 0027/0028/0030 to make history disappear.

Suggested commit:

~~~text
docs: record connection-owned observe authority decision
~~~

## 8. 09-F — Full repository verification

All final gates must run against one final source snapshot after ADR/runbook edits and after any defect fix.

### 8.1 Architecture/checker tests

~~~sh
python3 tools/architecture/test_check_boundaries.py
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/test_check_expert_extensibility.py
python3 tools/architecture/check_expert_extensibility.py
python3 tools/architecture/test_check_connection_observe_conformance.py
python3 tools/architecture/check_connection_observe_conformance.py
~~~

### 8.2 Rust workspace

~~~sh
cargo check --workspace --lib
CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast -- --test-threads=1
cargo build -p floe-ffi
git diff --check
~~~

Use the serialized broad test command because prior App global-runner tests can race under default parallel execution. Do not hide a deterministic failure as a “known race”.

### 8.3 Go server

From `server/`:

~~~sh
go test ./...
go test -race ./...
go vet ./...
~~~

This is required in 09 even if 09 changes no Go production code because the final authority story includes hosted generic View authorization.

### 8.4 Real disposable loopback server boundary

On supported macOS+Go environments, from repo root:

~~~sh
cargo test -p floe-provider-adapters --test live_server_access
~~~

This test is documented as disposable: it starts a local server, pairs with test-owned keys and does not use the shared saved-connection Keychain slot.

If environment prerequisites are unavailable, record UNAVAILABLE with the concrete reason. Do not replace it with an external-account test.

### 8.5 Flutter / FFI same snapshot

From `apps/client/`, after the FFI build above:

~~~sh
flutter analyze
flutter test
flutter build macos
~~~

If `integration/local_server_pairing_test.dart` is supported by the environment, also run:

~~~sh
flutter test integration/local_server_pairing_test.dart
~~~

It uses a fresh temporary profile and memory-only credential persistence. Record UNAVAILABLE rather than mutating shared credentials if prerequisites conflict.

Do not require the Foundation-model product integration test for this authority refactor unless its documented environment is already available; if run, report it separately.

### 8.6 Apple native deterministic packages

Run:

~~~sh
swift test --package-path apps/client/apple/FloeAppleContacts
swift test --package-path apps/client/apple/FloeAppleHealth
swift test --package-path apps/client/apple/FeasibilityProvider
swift test --package-path apps/client/ios/ScreenTimeGate
bash tools/s3-validation/check-native.sh
~~~

`check-native.sh` is deterministic native payload/conflict/default-gate coverage and does not read a personal Calendar.

Do not run the ignored real EventKit response-loss test without explicit operator authorization for the dedicated disposable calendar/event.

### 8.7 Live source/device smoke

Live Calendar, Contacts, Health, Screen Time or external provider/account smoke is optional only when an explicitly approved disposable source/device/account exists.

Otherwise record:

`SKIPPED — no explicitly approved disposable live source/device/account`

Never edit TCC, signing identities, shared Keychain slots, provider accounts or unrelated source data to satisfy 09.

## 9. 09-G — Final docs consistency pass

After ADR 0031 and runbook edits:

~~~sh
rg -n "one selected calendar|exactly one reviewed EventKit calendar|selection change while Use with Floe is active reviews|Grant resource choices cannot exceed" docs apps/client/README.md
rg -n "ConsumerPolicyAuthority|consumer policy authority|policy authority" docs/architecture docs/product apps/client/README.md
rg -n "separate remote Calendar|remote Calendar authority|leaf grant" docs/architecture docs/product apps/client/README.md
~~~

Interpretation:
- old ADRs may retain historical statements because ADR 0031 explicitly amends them;
- current architecture/product/runbook documents must not assert the old semantics;
- temporary 00-08 plans may retain historical evidence until plan retirement.

Review `docs/README.md`:
- keep the active-plan pointer while 09 is in progress;
- do not remove it in the 09 closure commit;
- plan retirement remains a separate post-acceptance cleanup.

## 10. 09-H — Final metrics and evidence

Metrics are supporting evidence, not acceptance criteria.

Record:

1. final HEAD;
2. checkpoint 09 commits;
3. new ADR file;
4. current-doc/runbook files changed;
5. production code changes, expected zero unless verification found a defect;
6. production files deleted in 09, expected zero;
7. final architecture checker count/list;
8. final public/dependency changes in 09, expected zero unless a defect/cleanup is discovered;
9. test files/tests changed in 09;
10. full verification results.

For historical migration size, do not attribute every line in an arbitrary long-range main diff to this refactor. Prefer explicit deletion/type/route lists already recorded by checkpoints 00-08. If a mechanical range summary is included, label the exact range and state that it is repository diff context, not refactor-only attribution.

## 11. Suggested execution slices

### 09-A — baseline + focused verification

- fetch/clean baseline;
- structural checkers;
- named invariant matrix;
- residual audit.

No commit unless a real code/test defect is fixed.

### 09-B — current docs/runbook convergence

- audit architecture/product docs;
- update only stale current statements;
- fix `apps/client/README.md` multi-Calendar/current-source semantics.

Suggested commit:

~~~text
docs: converge connection observe current documentation
~~~

Skip architecture-doc edits if already current.

### 09-C — ADR 0031

- add ADR 0031;
- update ADR index.

Suggested commit:

~~~text
docs: record connection-owned observe authority decision
~~~

### 09-D — full final verification

- Rust;
- architecture tools;
- Go;
- FFI;
- Flutter/macOS;
- Apple native deterministic tests;
- disposable loopback server test where supported;
- residual recheck.

No evidence-only code changes.

### 09-E — closure

- append exact execution evidence to this file;
- set 09 Complete in parent README;
- record unavailable/skipped gates honestly;
- verify final clean worktree;
- stop before plan retirement.

Suggested commit:

~~~text
docs: complete connection observe checkpoint 09
~~~

## 12. Completion gate

Do not mark 09 Complete until:

1. start and final fetched `origin/main` are recorded;
2. all three architecture checkers and checker test suites pass;
3. focused invariants pass;
4. production residual audit has no unexplained legacy match;
5. broad serialized Rust workspace passes;
6. `cargo check --workspace --lib` passes;
7. FFI build passes;
8. Go test/race/vet pass;
9. Flutter analyze/full tests/macOS build pass;
10. Apple native deterministic packages and `check-native.sh` pass, or a concrete environment limitation is recorded for a genuinely unavailable target;
11. loopback live_server_access passes where supported or is explicitly UNAVAILABLE;
12. current architecture/product docs match source;
13. client runbook no longer claims one selected Calendar;
14. ADR 0031 exists and amends 0027/0028/0030;
15. ADR index points readers to 0031;
16. no unauthorized live account/device/source mutation was performed;
17. README shows 00-09 Complete;
18. final worktree is clean.

If any required deterministic gate fails, 09 stays incomplete.

## 13. Required execution evidence

Append to this file:

1. date;
2. start local HEAD;
3. first fetched `origin/main`;
4. final fetched `origin/main`;
5. final HEAD;
6. 09 commit SHAs by slice;
7. production-code change count and reason;
8. architecture checker test results;
9. architecture checker production results;
10. 11->12 App conformance result;
11. Context old-dependency/new-12-resource result;
12. native subject/generation drift results;
13. first-party policy/digest/extension-denial results;
14. hosted remote review/drift results;
15. durable interaction/crash recovery results;
16. exact-recipient consent results;
17. Actions source/uncertain-write results;
18. residual production search classification;
19. Cargo metadata/dependency-policy result;
20. `cargo check --workspace --lib`;
21. broad serialized Rust result;
22. protocol/FFI implicit workspace result and explicit FFI build;
23. Go test result;
24. Go race result;
25. Go vet result;
26. disposable `live_server_access` result or UNAVAILABLE reason;
27. Flutter analyze result;
28. full Flutter test result/count;
29. macOS build result;
30. local server pairing integration result or UNAVAILABLE reason;
31. Apple Contacts Swift result;
32. Apple Health Swift result;
33. Feasibility Swift result;
34. ScreenTime Swift result;
35. native `check-native.sh` result;
36. any live device/source smoke or SKIPPED reason;
37. current docs changed;
38. ADR 0031 path/title/amendments;
39. ADR index update;
40. final metrics;
41. final `git diff --check`;
42. final clean worktree;
43. confirmation plan retirement was not performed;
44. confirmation no work beyond Checkpoint 09 was started.

## 14. Required agent report

Report only:

1. start HEAD / first and final fetched origin-main / final HEAD;
2. Checkpoint 09 commit SHAs;
3. production code changes, if any, and why;
4. final owner/path summary;
5. three architecture checker results;
6. focused 11->12 Calendar result;
7. GrantId/GrantAuthority/candidate/binding continuity result;
8. old dependency/new current-source read result;
9. native subject/generation drift result;
10. first-party policy/digest/third-party denial result;
11. hosted remote drift/result;
12. durable interaction/recovery result;
13. exact-recipient consent result;
14. Actions source/recovery result;
15. residual audit/classification;
16. Cargo metadata/dependency-policy result;
17. broad Rust/check result;
18. FFI build result;
19. Go test/race/vet results;
20. disposable loopback server result or UNAVAILABLE reason;
21. Flutter analyze/full tests/macOS build results;
22. local server pairing integration result or UNAVAILABLE reason;
23. Apple Contacts/Health/Feasibility/ScreenTime results;
24. native `check-native.sh` result;
25. live source/device smoke or SKIPPED reason;
26. current docs/runbook files updated;
27. ADR 0031 summary and amended ADRs;
28. final metrics;
29. README 09 Complete confirmation;
30. clean worktree;
31. explicit confirmation plan bundle/docs active-plan pointer were not retired yet.

## 15. Plan retirement — after explicit user acceptance only

After Checkpoint 09 has completed and the user explicitly accepts the final result, perform a separate documentation cleanup commit:

- delete `docs/development/plans/connection-observe-authority/`;
- remove the Active execution plan pointer from `docs/README.md`;
- verify no default-agent/current documentation points to the retired plan as active;
- keep ADR 0031 and current architecture/product/runbook documents;
- use Git history as the archive.

Do not perform this retirement as part of the 09 implementation or closure commit.

## 16. Execution evidence — 2026-09-30

### Outcome and snapshot

**Incomplete.** The required full Flutter test gate failed deterministically. Execution
stopped at that gate; no macOS build, pairing integration or Apple deterministic gate
was advanced, and 09 is not marked Complete. No architecture was reopened and no
production/test assertion or golden was changed to force a pass.

- Start: clean local `main` at `fc761ded422ef979ff3b977000a7c89dfb4a672b`.
- First fetched `origin/main`: `fc761ded422ef979ff3b977000a7c89dfb4a672b`.
- Final fetched `origin/main`: `fc761ded422ef979ff3b977000a7c89dfb4a672b`; final fetch succeeded after failure diagnosis.
- Final verification snapshot: `102eec86faa76dea339f2216ae3998666a26eef4`.
- 09-B documentation commit: `5cd1f937021af010a41f9595767a1994ca821fcb`.
- 09-C ADR commit: `102eec86faa76dea339f2216ae3998666a26eef4`.
- Failed-gate evidence commit: `025feccaa70ac4e6a2aa5ae4c1dc276c53c3bc91`.
- Final HEAD / evidence finalization commit: the commit containing this update, titled
  `docs: finalize checkpoint 09 failure evidence`; resolve its SHA with
  `git log -1 --format=%H -- docs/development/plans/connection-observe-authority/09-verification-and-docs.md`.
  Its only additional changes are checkpoint status/evidence, not verification inputs.
- Execution order: fetched baseline/metadata/checkers → all focused regressions →
  production residual audit → current docs/ADR → full gates through Flutter failure →
  focused failure diagnosis and evidence. Closure/retirement acceptance was not reached.

Planning anchors were re-resolved on the actual execution HEAD using source searches
and named test execution. The planning-base-to-start diff changes only this plan,
not source. Principal current anchors include App 11→12 conformance at
`crates/app/src/vault_host/tests/calendar_connection_observe.rs:294`,
Context growth at `crates/modules/context/tests/native_calendar_read.rs:479`,
policy at `crates/app/src/first_party_observe.rs:251`, hosted resource continuity at
`crates/app/src/vault_host/remote_observe.rs:951`, native/hosted crash recovery at
`crates/app/src/vault_host/tests/interaction_resolution.rs:2006` / `:3648`,
recipient source identity at
`crates/modules/access/src/application/recipient_consent.rs:700`, and Actions source
fence / response-loss recovery at
`crates/app/src/vault_host/tests/proposals.rs:205` /
`crates/app/src/vault_host/tests/native_actions.rs:177`.

### Frozen owner/path and focused evidence

Connections retains current source resources/subject/`SourceAuthority`; Access retains
stable logical standing grants/`GrantAuthority`; Experts retains connection/View binding;
Context reloads current sources and records exact physical provenance. Product callers
use `ConnectionObserve`; hosted acquisition uses generic signed View
preview/admission/read/release. No owner, public contract, dependency, permission mode,
compatibility path or authority was introduced.

All 29 commands from section 4 passed, with one named test matched per filter. App's
separate integration target can report zero filtered matches, but the requested named
test in the crate matched and passed; there was no zero-match command.

| Exact command | Result |
|---|---|
| `cargo test -p floe-app calendar_connection_observe_conformance_eleven_to_twelve -- --test-threads=1` | PASS — 1 named test |
| `cargo test -p floe-context --test native_calendar_read current_resource_growth_reads_all_calendars_and_stales_old_dependency` | PASS — 1 named test |
| `cargo test -p floe-app common_observe_pauses_without_changing_calendar_source -- --test-threads=1` | PASS — 1 named test |
| `cargo test -p floe-app common_observe_rejects_stale_grant_source_subject_and_identity -- --test-threads=1` | PASS — 1 named test |
| `cargo test -p floe-context --test native_calendar_read native_view_rejects_connection_change_after_observation` | PASS — 1 named test |
| `cargo test -p floe-context --test native_calendar_read native_view_rejects_device_generation_drift` | PASS — 1 named test |
| `cargo test -p floe-context --test native_calendar_read native_admission_rejects_missing_or_changed_authority` | PASS — 1 named test |
| `cargo test -p floe-app every_member_digest_matches_the_activation_policy` | PASS — 1 named test |
| `cargo test -p floe-app policy_never_default_grants_extensions_or_wildcards` | PASS — 1 named test |
| `cargo test -p floe-app shipped_permission_digest_does_not_depend_on_registry_state -- --test-threads=1` | PASS — 1 named test |
| `cargo test -p floe-app calendar_consumers_match_trusted_shipped_capability_declarations` | PASS — 1 named test |
| `cargo test -p floe-app product_review_uses_common_connection_expectation_without_routing_fields` | PASS — 1 named test |
| `cargo test -p floe-app review_then_enable_binds_gmail_bundle_atomically` | PASS — 1 named test |
| `cargo test -p floe-app calendar_resource_edit_keeps_logical_grant_without_automatic_review` | PASS — 1 named test |
| `cargo test -p floe-app stale_descriptor_refuses_without_mutation` | PASS — 1 named test |
| `cargo test -p floe-app disable_still_pauses_and_disconnects_without_expectations` | PASS — 1 named test |
| `cargo test -p floe-app gmail_authority_rotation_after_review_supersedes_without_mutation -- --test-threads=1` | PASS — 1 named test |
| `cargo test -p floe-app gmail_bad_signature_never_mutates_nor_resolves -- --test-threads=1` | PASS — 1 named test |
| `cargo test -p floe-app remote_calendar_allow_resolves_through_hosted_connection -- --test-threads=1` | PASS — 1 named test |
| `cargo test -p floe-app fresh_approve_with_drift_supersedes_without_mutation -- --test-threads=1` | PASS — 1 named test |
| `cargo test -p floe-app native_commit_then_crash_reopens_and_resolves_without_second_advance -- --test-threads=1` | PASS — 1 named test |
| `cargo test -p floe-app gmail_commit_then_crash_reopens_and_resolves_without_second_mutation -- --test-threads=1` | PASS — 1 named test |
| `cargo test -p floe-app observer_cancellation_never_revokes_a_recorded_decision -- --test-threads=1` | PASS — 1 named test |
| `cargo test -p floe-access consent_identity_binds_grant_and_exact_source_resources_and_authority` | PASS — 1 named test |
| `cargo test -p floe-access changed_review_fields_change_consent_identity` | PASS — 1 named test |
| `cargo test -p floe-app consent_approve_grants_exact_review_and_resolves -- --test-threads=1` | PASS — 1 named test |
| `cargo test -p floe-app consent_wrong_device_never_grants -- --test-threads=1` | PASS — 1 named test |
| `cargo test -p floe-app source_resource_change_blocks_old_calendar_receipt_without_mirror_revision_change -- --test-threads=1` | PASS — 1 named test |
| `cargo test -p floe-app native_executor_uses_rust_ledger_and_lookup_only_after_response_loss -- --test-threads=1` | PASS — 1 named test |

Together these prove 11 physical resources → one candidate/logical permission/binding;
11→12 advances source authority, preserves GrantId/GrantAuthority/candidate ID/binding
revision and Active Observe, rejects old review/dependency, and reads all twelve next.
Native subject/identity/generation drift rejects. Shipped policy/digest is independent
of Registry and denies extensions/wildcards. Hosted signature/authority drift cannot
mutate or resolve stale review. Native/hosted commit-then-crash rejoins without a second
mutation; observer cancellation does not revoke recorded intent. Exact-recipient identity
binds both resource sets and authorities; wrong device rejects. Actions source fences
and Rust-ledger lookup-only lost-response recovery remain intact.

### Residual and documentation audit

All production searches in section 5 ran before edits and again after verification;
`check_connection_observe_conformance.py` also passed.

- Removed remote Calendar types/routes, old per-leaf consumer composition, old
  product access contracts and leaf-scoped ConnectionObserve inputs: no matches.
- `consumer_policy` / `policy_authority` strings in Context contracts,
  Conversation and protocol are negative deserialization tests, not runtime authority.
- `granted_resources` strings in protocol are rejection tests for obsolete projections.
- Old Calendar policy/mapping table names in
  `crates/adapters/vault/src/vault.rs:338` and
  `crates/adapters/vault/src/vault/calendar_grants.rs:767` are fail-closed stale-schema
  detection and its regression, not a decoder or fallback.
- `calendar_ids` / `selected_handles`: Connections source setup, native/provider/server
  acquisition, Context lease/observation provenance and Actions exact destination/source
  evidence, plus their tests. Dormant Android adapter fields were not extended or tested.
- `source_resources`: exact physical provenance, signed hosted evidence and read-only
  source presentation; not another permission selection list.
- `RemoteConnectionObserveExpectation`: crate-private hosted signed evidence;
  not a product wire/routing contract or independent permission owner.
- `calendar_lease`: current Context lease/provenance; `calendar_access`: canonical
  App composition/current Context and system-subject checks, not an obsolete product editor.
- No unexplained legacy production match or compatibility exception remains.

Current architecture modules/runtime/invariants, authority/recovery, product privacy,
client/server runbooks and relevant Calendar design text were inspected against source.
Correct ownership statements were left intact. Current doc changes:
`apps/client/README.md` (multi-Calendar source set, logical Observe, evidence and Action
fences); `docs/design/s1-calendar-ui.md` (remove false single-native-calendar implementation
claim); `docs/architecture/authority-recovery.md` (correct accepted ADR 0027 status and
link the durable amendment).

ADR `docs/decisions/0031-connection-owned-source-scope-and-logical-observe.md`,
**Connection-owned source scope and logical standing Observe**, amends 0027 standing
epochs, 0028 physical-source versus logical permission/resource edits and 0030 durable
Observe reviewed-target semantics; it extends 0029. The ADR index includes 0031 and
amendment navigation from 0027/0028/0030. Bodies of those historical ADRs are unchanged.

Section 9 searches ran after these edits: current architecture/product/client runbook
contains none of the old authority/remote-Calendar/leaf-grant claims. Remaining single-
Calendar phrases are historical ADR/temporary-plan quotations or the design intent
“not one selected calendar”, not an assertion of current single-Calendar implementation.

### Full gate evidence — one final verification snapshot

The six structural commands passed both at baseline and final snapshot. Final results:

| Exact command | Result |
|---|---|
| `cargo metadata --no-deps --format-version 1` | PASS at baseline and final snapshot; outputs compare identical; no manifest/dependency changes |
| `python3 tools/architecture/test_check_boundaries.py` | PASS — 10 tests |
| `python3 tools/architecture/check_boundaries.py` | PASS — dependency policy |
| `python3 tools/architecture/test_check_expert_extensibility.py` | PASS — 7 tests |
| `python3 tools/architecture/check_expert_extensibility.py` | PASS |
| `python3 tools/architecture/test_check_connection_observe_conformance.py` | PASS — 3 tests |
| `python3 tools/architecture/check_connection_observe_conformance.py` | PASS |
| `cargo check --workspace --lib` | PASS |
| `CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast -- --test-threads=1` | PASS; includes protocol/FFI; ignored live tests not enabled |
| `cargo build -p floe-ffi` | PASS |
| `git diff --check` | PASS at baseline, final verification snapshot and evidence preparation |
| `cd server && go test ./...` | PASS |
| `cd server && go test -race ./...` | PASS |
| `cd server && go vet ./...` | PASS |
| `cargo test -p floe-provider-adapters --test live_server_access` | PASS — 1 disposable loopback test; 1 approved-OAuth/model test ignored |
| `cd apps/client && flutter analyze` | PASS — no issues |
| `cd apps/client && flutter test` | FAIL — 366 passed, 5 failed |
| `cd apps/client && flutter build macos` | NOT RUN — stopped at required Flutter failure |
| `cd apps/client && flutter test integration/local_server_pairing_test.dart` | NOT RUN — stopped at required Flutter failure; not claimed unavailable |
| `swift test --package-path apps/client/apple/FloeAppleContacts` | NOT RUN — stopped at required Flutter failure |
| `swift test --package-path apps/client/apple/FloeAppleHealth` | NOT RUN — stopped at required Flutter failure |
| `swift test --package-path apps/client/apple/FeasibilityProvider` | NOT RUN — stopped at required Flutter failure |
| `swift test --package-path apps/client/ios/ScreenTimeGate` | NOT RUN — stopped at required Flutter failure |
| `bash tools/s3-validation/check-native.sh` | NOT RUN — stopped at required Flutter failure |
| Live Calendar/Contacts/Health/ScreenTime/provider/account smoke | SKIPPED — no explicitly approved disposable live source/device/account |

Rust's existing dead-code warnings (`Supplied`, `RefreshOutcome.reason`, Vault test
`admit_agent_action_dispatch` / `admit_agent_action_dispatch_with_cancellation`)
appeared without production changes; they were not suppressed.
Flutter version: stable 3.47.2, Dart 3.13.2.

### Required failed-gate disposition

The same five failures reproduced with:

`cd apps/client && flutter test test/features/day/calendar_gateway_test.dart test/features/actions/calendar_action_gateway_test.dart test/features/experts/agent_registry_dialog_test.dart`

Result: **FAIL — 6 passed, 5 failed**, not a parallel-only race.

| Named failing regression | Observed failure |
|---|---|
| `calendar_action_gateway_test.dart`: pending native source cannot authorize an action proposal | `native Calendar unavailable` during source setup |
| `calendar_gateway_test.dart`: sync updates Day mirror without changing Connections authority | `native Calendar unavailable` during source setup |
| `calendar_gateway_test.dart`: all-available inventory changes source authority; read failure does not | `native Calendar unavailable` during source setup |
| `calendar_gateway_test.dart`: large native source still imports although observation publication is bounded | `native Calendar unavailable` during source setup |
| `agent_registry_dialog_test.dart`: registry management remains readable and operates real controller at width 520.0 | Golden `agent_registry.png`: 0.02%, 29px diff |

Diagnosis: `TestAppHost.open` injects a fixture adapter into Day but not the native
acquisition/subject bridge required by reviewed Connections source establishment.
The failure is mapped at `crates/app/src/connection_services.rs:440`; the four tests
do not reach their intended mirror/Action assertions. The pending-source fixture also
assumes the old setup lifecycle. This is not evidence that the production source
checks should be bypassed. The Registry golden difference is a separate visual gate
failure, not an Observe ownership defect. Neither was silently skipped, reclassified
as unavailable, fixed with a compatibility route, or accepted by relaxing assertions.

No code fix was made: the plan requires stopping at a failed gate and 09 cannot
claim completion with these reproducible failures. A same-snapshot passing Flutter
gate and all later gates remain necessary before closure. Golden diagnostic artifacts
were moved out of the worktree to `/tmp/floe09/golden-failures`; raw command logs are
under `/tmp/floe09/`. They are diagnostics, not a second repository progress document.

### Metrics, safety and closure status

- Production code changed/deleted: **0 files / 0 lines**; no defect-fix commit.
- Public/FFI/dependency/schema changes: **0**.
- Test files/tests/goldens changed: **0**.
- Documentation files changed in 09: **7**, including this evidence and parent status.
- Final architecture checkers: **3**, with **3 test suites / 20 checker tests**.
- Focused named Rust regressions: **29 PASS**; full Rust/Go/FFI gates above pass.
- Flutter full gate: **366 passed / 5 failed**; later gates remain **NOT RUN**.
- Worktree was clean before execution, after the two documentation commits and after
  failed-gate evidence commit `025feccaa70ac4e6a2aa5ae4c1dc276c53c3bc91`.
  Evidence finalization changes only this plan; final clean state is checked
  with `git status --porcelain` after committing.
- No TCC, signing, account, shared Keychain or personal Calendar state was changed.
  Only the documented disposable loopback boundary test was run; ignored live tests
  and Foundation-model integration were not enabled.
- Parent README records **09 Incomplete**, not Complete; 00–08 remain Complete.
- Plan bundle and `docs/README.md` active-plan pointer are retained.
- Plan retirement was **not performed**. No work beyond Checkpoint 09 was started.


### Residual closure — 2026-09-30 (blocked, not complete)

#### Snapshot and bounded implementation

- Residual start: clean `main` at `2a08dd1bc81719d4a76f0fa6df6f126872183a24`.
- First and final successful `git fetch origin`: `origin/main` remained
  `2a08dd1bc81719d4a76f0fa6df6f126872183a24`.
- Harness commit: `10b8a2437555d878d19acd1d5edba54fd5b3d6ea`
  (`test: reproduce calendar residual gate in native fixture bundle`).
- Committed reproduction snapshot: that harness commit. Final HEAD is the
  evidence-only commit containing this subsection, titled
  `docs: record checkpoint 09 residual action gate blocker`; resolve it with
  `git log -1 --format=%H -- docs/development/plans/connection-observe-authority/09-verification-and-docs.md`.
- No completion/closure commit was made. Parent 09 remains Incomplete.

Added:
- `apps/client/integration/native_calendar_fixture_test.dart`;
- `apps/client/integration/support/native_calendar_fixture_host.dart`;
- `tools/validation/native-calendar-fixture-Info.plist`.

Changed test support:
- `apps/client/integration/support/disposable_product_profile.dart`: allow an
  explicitly supplied Person for private validation profiles. The default random
  Person remains unchanged for existing callers. Native Calendar requires the
  canonical local Person (`NativeCalendarReadAccess::validate_request`); supplying
  a random Person correctly failed source establishment in the first harness run.
  The Calendar scenarios use `localPersonId` with fresh private profile roots and
  unique device IDs. They never create a Vault or touch shared credential slots.

The parent copies, rather than modifies, the SDK `flutter_tester` into a private
temporary `.app`, builds the child assets, compiles the existing
`NativeCalendarFixture.swift` with `xcrun swiftc -emit-library -warnings-as-errors`,
sets `@rpath/libfloe_eventkit.dylib`, ad-hoc signs and verifies the dylib/bundle,
and launches the copied executable. Only the same-source FFI path is added to the
child environment. Production bundle-relative loading, `probe_calendar_source`,
`DeviceCalendarSubject`, and native provider validation execute unchanged.
The child uses `AppRuntime`, its `AppWireCalendarSourceGateway`, `NativeDayGateway`
and `CalendarActionFacade`; no `flutter_test` or `test/support/app_host.dart` import.
It checks the fixture subject fingerprint and Ready lifecycle explicitly.
The existing Swift fixture's `creates.txt` sentinel proves no native write occurred.

Production Rust/App/Flutter runtime, permission checks, public/FFI contracts,
dependencies, schemas, native loader and provider fixture: **unchanged**.
Registry UI/test, `agent_registry.png`, and golden tolerance: **unchanged**.
No fallback, subject bypass, compatibility route or new skip was added.

#### Exact reproduction and results

Commands actually run:
- `cargo build -p floe-ffi`: PASS.
- `cd apps/client && dart format integration/native_calendar_fixture_test.dart integration/support/native_calendar_fixture_host.dart`:
  PASS; changed test-support Dart was also formatted.
- `cd apps/client && flutter test integration/native_calendar_fixture_test.dart --reporter expanded`:
  FAIL on the working harness and again on committed `10b8a243`;
  parent result **0 passed / 1 failed**, child exit **1**.
- `git diff --check`: PASS; production/golden-path diff search: empty.

Child scenario evidence on the committed snapshot:

| Scenario | Result |
|---|---|
| A. Day mirror/source continuity | PASS — both sync mirror revisions, content change, unchanged source revision/authority, and reopen identity/resources/content/revisions checked; `DAY_MIRROR_SOURCE_CONTINUITY_PASSED` |
| B. AllAvailable 1→2 reconciliation | PASS — production inventory reconciliation grows resources, increments source revision, advances authority; permission-denied read status retains the expanded revision/authority/resources; `ALL_AVAILABLE_SOURCE_AUTHORITY_PASSED` |
| C. Wide source / bounded publication | PASS — reviewed source retains 129 resources, Day imports 129 events without a mirror error; `WIDE_SOURCE_BOUNDED_PUBLICATION_PASSED` |
| D. Reviewed native source without current evidence | FAIL — proposal returned successfully; `ACTION_FENCE_DIAGNOSTIC rejected=false rows=1`; no native write occurred |

All four private profiles emitted `VALIDATION_PROFILE_REMOVED`; parent teardown
removed its temporary bundle/assets. No personal Calendar was read or written.
Raw reproduction logs are `/tmp/floe09-residual/fixture-initial.log`,
`fixture-final.log`, and `fixture-committed.log`, not a repository progress file.

#### Blocker classification and stop boundary

This failure is **not** the missing-dylib environment mismatch: reviewed native
creation succeeds with the fixture. It is a mismatch between §17.4 D's required
evidence-fence assertion and the current user-proposal runtime contract:

- `crates/app/src/vault_host/product_actions.rs:110` maps
  `CalendarActionOperation::Propose` to the user `CalendarActionCommand::Propose`;
- `crates/app/src/action_facade.rs:152` calls Actions `propose_calendar_action`;
- `crates/modules/actions/src/application/service.rs:72` drafts then persists a
  proposal; `draft_calendar_action` checks the future interval and the current
  serving source/destination, not a Context dependency or delegated artifact;
- the old standard test expected Conflict because its source was Pending. The
  reviewed-native lifecycle now correctly starts Ready. Lack of current evidence
  does not recreate that old source-state conflict;
- delegated artifact/evidence publication has a separate current fence, but this
  facade request supplies no delegated artifact/evidence identity. Substituting
  that other request merely to obtain a rejection would not prove the planned
  facade assertion.

No production defect fix was made: imposing a new evidence prerequisite on this
user-proposal path would change its admission meaning rather than fix native
fixture availability. §17.4 D must be reconciled with the intended user versus
delegated proposal contract before proceeding; no assertion was weakened and no
Ready source was made Pending to manufacture a pass.

Execution stopped during R2 at the reproducible D failure. R3 explicitly requires
all bundle scenarios to pass first, so **no standard tests were moved/deleted**;
the original three-test Day file and old Action scenario remain pending that gate.
No skipped duplicates were introduced. The new failing regression is retained as
the deterministic blocker reproducer, not claimed as completed migration coverage.

Later gates were **NOT RUN**, not passed or unavailable:
- R4 Flutter analyze, focused action/Registry tests, full non-golden suite;
- R5 macOS build, disposable local-server pairing, Contacts, Health,
  FeasibilityProvider, ScreenTimeGate and `check-native.sh`;
- R6 three checker suites/checkers, metadata, workspace check/broad serialized
  Rust, final FFI rebuild, Go test/race/vet and disposable loopback.

Non-golden full-suite count: **not measured in this residual run**.
Golden result: **IGNORED — operator excluded golden tests**; no new Registry run,
no golden modification; historical mismatch remains classified separately.
Live smoke: **SKIPPED — no explicitly approved disposable live source/device/account**.

Diagnostic residual searches from §17.9 were executed:
- `native Calendar unavailable`: no literal test/integration matches;
- `pending native source`: original Action test remains, because R3 was not reached;
- `FixtureCalendarAdapter|calendar_gateway_test\.dart`: original Day/Action test
  references and new integration child, classified as unclosed migration placement;
- native library/fixture references: existing runtime bundle paths/provider tests
  plus the bounded validation parent; no production loader override.

`git diff --check` passes. The harness commit had a clean worktree; after the
evidence-only commit final cleanliness is checked with `git status --porcelain`.
The parent README remains **09 Incomplete**, not Complete. Plan bundle and
`docs/README.md` active-plan pointer remain intact; retirement was not performed.
No work beyond Checkpoint 09 was started.

## 17. Residual closure plan — 2026-09-30

This section is the authoritative continuation after the failed full Flutter gate recorded
in section 16. It supersedes the earlier Flutter/golden completion wording only where
stated below. All already-passing architecture/Rust/Go/FFI evidence remains valid but
must be re-run at the final same-source closure snapshot as specified in §17.8.

### 17.1 Operator decision: golden tests do not gate Checkpoint 09

The Registry golden mismatch is explicitly out of scope for this residual closure.

Do not:
- update `apps/client/goldens/agent_registry.png`;
- change `AgentRegistrySettings` UI merely to satisfy the image;
- change golden tolerances;
- treat a golden mismatch as a Checkpoint 09 blocker.

The existing behavioral assertions in
`apps/client/test/features/experts/agent_registry_dialog_test.dart` remain valuable.
Only image-comparison assertions are excluded from acceptance. If an unfiltered
`flutter test` reports golden failures, classify them separately and require **zero
non-golden failures**.

For the current width-520 test, the image assertion is at
`apps/client/test/features/experts/agent_registry_dialog_test.dart:76-81`.
Do not touch it unless moving/tagging the assertion is strictly necessary to run a
clean non-golden test command; if touched, preserve the behavioral assertions and do
not update the image.

### 17.2 Residual blocker and no-bypass rule

The four non-golden failures are test-environment mismatches, not permission-policy
failures.

Current path:
- `apps/client/test/support/app_host.dart:8-53` loads the same-source FFI dylib and
  injects `CalendarAdapter` only into `NativeDayGateway`;
- `apps/client/test/features/day/calendar_gateway_test.dart:13-73` supplies a Dart
  `FixtureCalendarAdapter` and calls the real Connections source gateway;
- source establishment/reconciliation reaches
  `crates/app/src/connection_services.rs:404-447 probe_calendar_source`;
- on macOS, `crates/app/src/vault_host/calendar_access.rs:176-224
  DeviceCalendarSubject::subject` deliberately uses
  `NativeCalendarReadAccess`, not the Flutter Day adapter;
- `crates/adapters/providers/src/sources/native_calendar.rs:586-623` resolves the
  EventKit call through `GatedStringCall`;
- the expected native library is
  `Frameworks/libfloe_eventkit.dylib` at
  `native_calendar.rs:608+`, relative to the running executable's bundle.

A normal `flutter test` process is not the product app bundle and therefore does not
carry that native library. The resulting `native Calendar unavailable` is correct
production behavior for that process.

Do **not** fix the tests by:
- skipping `probe_calendar_source`;
- establishing a Ready source without reviewed native subject evidence;
- adding a production “test/fixture” Calendar connector;
- falling back from the macOS native provider to another internal runtime path;
- adding a legacy source constructor;
- writing beside or modifying the Flutter SDK's own `flutter_tester`;
- adding a release/runtime environment override that lets arbitrary paths replace the
  bundled EventKit dylib.

The test environment must satisfy the existing native boundary instead.

### 17.3 09-R1 — Re-home bundle-dependent Calendar tests to a bundle fixture

Use the same controlled-bundle pattern already established by:
- `crates/adapters/providers/tests/native_calendar.rs:71-111
  run_read_fixture_child`;
- `apps/client/integration/product_conversation_test.dart:22-91`.

Reuse the existing deterministic provider fixture:
`crates/adapters/providers/tests/fixtures/NativeCalendarFixture.swift`.

It implements `view_access` for arbitrary canonical `calendar_ids`, returns stable
subject fingerprint/generation by default, and does not access a personal Calendar.

#### Add parent integration test

Create:

`apps/client/integration/native_calendar_fixture_test.dart`

The parent test must:

1. require macOS and the same-source `target/debug/libfloe_ffi.dylib`;
2. create a private temporary `.app` bundle;
3. copy `Platform.resolvedExecutable` to
   `Contents/MacOS/flutter_tester`;
4. build Flutter assets for the child target in §17.4 with `flutter build bundle`;
5. compile
   `../../crates/adapters/providers/tests/fixtures/NativeCalendarFixture.swift`
   as `Contents/Frameworks/libfloe_eventkit.dylib` using
   `xcrun swiftc -emit-library -warnings-as-errors`;
6. give the dylib the normal `@rpath/libfloe_eventkit.dylib` install name;
7. ad-hoc sign the dylib and temporary bundle;
8. launch the copied `flutter_tester` exactly from that bundle so Rust
   `current_exe()` resolves the fixture through the production
   `NativeLibrary { relative_path: "Frameworks/libfloe_eventkit.dylib" }`;
9. pass only the same-source FFI path and private validation paths through environment;
10. assert a zero child exit and all scenario sentinels;
11. delete only the temporary validation bundle/profile.

Add a validation-specific plist if needed rather than reusing a misleading product
identifier, for example:

`tools/validation/native-calendar-fixture-Info.plist`

Follow the existing product-conversation bundle launch arguments and codesign pattern.
Do not introduce a general native-loader override.

#### Suggested commit

~~~text
test: run calendar ffi scenarios in native fixture bundle
~~~

### 17.4 09-R2 — Child validation host and exact scenario migration

Create:

`apps/client/integration/support/native_calendar_fixture_host.dart`

The child host should use production:
- `AppRuntime`;
- `AppWireCalendarSourceGateway`;
- `NativeDayGateway`;
- `CalendarActionFacade`;

and a Dart-only fixture adapter for Calendar inventory/event contents.

Do not import `flutter_test` or depend on `test/support/app_host.dart` from the child.
Use an explicit `require(bool, label)` helper and exit non-zero on failure, following
`integration/support/product_conversation_host.dart`.

Re-home the exact four bundle-dependent scenarios.

#### A. Day mirror/source continuity

Move the semantics of
`apps/client/test/features/day/calendar_gateway_test.dart:83-143`:

- establish reviewed EventKit source through the real AppWire source gateway;
- fixture `view_access` supplies the native subject;
- sync Day through the Dart Calendar fixture;
- assert mirror revision changes on sync;
- assert Connections source revision and `SourceAuthority` do not change merely
  because the Day mirror changes;
- close/reopen private profile and verify the source/mirror state remains coherent.

#### B. AllAvailable inventory reconciliation

Move
`calendar_gateway_test.dart:145-197`:

- establish `all_available` source with one Calendar;
- sync once;
- grow fixture inventory to two Calendars;
- let production `reconcileNativeInventory` perform reviewed source
  reconfiguration against the native fixture subject;
- assert source revision increments and `SourceAuthority` advances;
- inject a Dart read permission failure for one Calendar;
- assert mirror/source status records the read failure without a second source
  authority change.

#### C. Wide source / bounded observation publication

Move
`calendar_gateway_test.dart:200-234`:

- configure more than `CalendarObservationPublisher.maxCalendarCount` source
  resources;
- reviewed native subject establishment must still succeed for the full set;
- Day import must retain all fixture events;
- bounded observation publication must not turn the successful Day import into a
  source/permission failure.

Do not reintroduce a fixed source-resource count to make this pass.

#### D. Action contract correction — delete the stale proposal-fence scenario

Do **not** re-home the old
`apps/client/test/features/actions/calendar_action_gateway_test.dart:96-149`
scenario into the native fixture bundle.

The earlier residual plan incorrectly assumed that a user-facing
`proposeCalendarAction` requires current Context evidence and must create no Action
row when that evidence is absent. The current Actions contract intentionally does not
work that way.

Current source of truth on the execution baseline:

- `crates/modules/actions/src/application/service.rs:72
  ActionService::propose_calendar_action` drafts and persists a user proposal;
- `service.rs:88 draft_calendar_action` requires a serving current
  `SourceConnection` and exact destination Calendar membership, then creates a
  `Pending` proposal with the current connection revision;
- it does **not** require a `ContextDependency` or observation receipt before the
  person can review the proposal;
- execution-time source/policy continuity is enforced later by
  `service.rs:370 action_block_reason`;
- Expert-originated proposals have the stronger evidence-bound path at
  `crates/modules/actions/src/application/expert.rs:141
  prepare_expert_calendar_action`.

Therefore the observed residual result:

~~~text
rejected=false rows=1
~~~

is the expected user-proposal contract, not a production defect.

The exact source/evidence and uncertain-write invariants are already protected by the
current Rust regressions, including:

- `crates/app/src/vault_host/tests/proposals.rs:205
  source_resource_change_blocks_old_calendar_receipt_without_mirror_revision_change`;
- `crates/app/src/vault_host/tests/native_actions.rs:177
  native_executor_uses_rust_ledger_and_lookup_only_after_response_loss`;
- the Expert Action tests around `prepare_expert_calendar_action`, which require
  recorded dependency evidence before an Expert-origin proposal can advance.

Required residual actions:

1. delete the stale
   `pending native source cannot authorize an action proposal` Flutter test;
2. remove `actionEvidenceFence` and its invocation from
   `apps/client/integration/support/native_calendar_fixture_host.dart`;
3. do not replace it with a test that expects user proposal creation to fail;
4. preserve the two pure AppWire Action gateway tests in
   `calendar_action_gateway_test.dart`;
5. do not change production Actions, Connections, Context, native subject or
   permission code to make the stale expectation pass.

Optional coverage, only if it makes the child fixture clearer rather than larger:
the bundle host may assert that a user proposal creates exactly one Pending row while
performing no native write. That is a proposal-versus-execution assertion, **not** an
evidence-fence assertion and is not required for 09 closure.

### 17.4-D correction evidence — 2026-09-30

The first residual fixture run reached the previously planned Action scenario and
reported `rejected=false rows=1`. Source inspection confirmed that this is the intended
user proposal contract, not a missing source/evidence fence: proposal creation persists
one reviewable Pending Action after validating the serving connection and destination;
the stronger Context/evidence fence belongs to execution and Expert-origin paths.

Accordingly, §17.4 D above replaces the stale assertion. No production contract change
is authorized by this correction.

### 17.5 09-R3 — Remove obsolete standard-test placement, not coverage

After the bundle scenarios pass:

- `apps/client/test/features/day/calendar_gateway_test.dart` contains only the three
  bundle-dependent FFI scenarios at the planning base. Delete the file after their
  assertions are re-homed; do not leave skipped duplicates.
- In
  `apps/client/test/features/actions/calendar_action_gateway_test.dart`, keep the
  two pure AppWire action gateway tests and delete the stale native-source proposal
  scenario; it is **not** re-homed because its assertion contradicts the current
  user proposal contract.
- Remove its import of `FixtureCalendarAdapter, query` from the deleted Day test.
- In the integration child host, delete `actionEvidenceFence` and its call; A/B/C
  are the three bundle-native scenarios that remain in scope.
- Keep `apps/client/test/support/app_host.dart` if other current tests still call it.
  Do not add a compatibility subject bypass to it merely to preserve the old test
  placement.

Run caller search before deleting:

~~~sh
rg -n "FixtureCalendarAdapter|calendar_gateway_test\.dart|TestAppHost" apps/client/test apps/client/integration
~~~

Every moved assertion must have an equivalent child-host assertion or the deletion is
a blocker.

### 17.6 09-R4 — Golden exclusion and Flutter acceptance

Per operator instruction, image goldens do not gate 09.

Required focused checks after R1-R3:

~~~sh
cargo build -p floe-ffi

cd apps/client
flutter analyze
flutter test test/features/actions/calendar_action_gateway_test.dart
flutter test integration/native_calendar_fixture_test.dart
flutter test test/features/experts/agent_registry_dialog_test.dart
~~~

For the Registry file:
- all non-image behavioral assertions must pass;
- any `matchesGoldenFile` failure is recorded as **IGNORED — operator excluded
  golden tests**;
- do not update the golden.

Then run the complete normal suite:

~~~sh
flutter test
~~~

Acceptance:
- zero non-golden failures;
- golden-only failures are reported but do not block 09;
- do not relabel a non-golden failure as golden.

If a clean command is preferred, a narrowly scoped test-only split/tag may isolate the
existing image assertion so `flutter test --exclude-tags=golden` can be used. This is
optional; do not churn the Registry test if exact failure classification is simpler.

### 17.7 09-R5 — Resume the gates that were never run

Only after the three bundle-native Calendar scenarios pass, the stale Action proposal
scenario is removed, and there are zero non-golden Flutter failures, continue the
previously stopped gates:

~~~sh
cd apps/client
flutter build macos
flutter test integration/local_server_pairing_test.dart
cd ../..

swift test --package-path apps/client/apple/FloeAppleContacts
swift test --package-path apps/client/apple/FloeAppleHealth
swift test --package-path apps/client/apple/FeasibilityProvider
swift test --package-path apps/client/ios/ScreenTimeGate

bash tools/s3-validation/check-native.sh
~~~

Rules:
- the pairing integration uses its documented disposable temporary profile;
- do not clear shared credentials/Keychain state to make it run;
- deterministic Apple/native package failures are blockers;
- real EventKit response-loss/live source smoke remains SKIPPED without the explicit
  disposable authorization described by the native validation runbook.

### 17.8 09-R6 — Same-snapshot final re-verification

Because R1-R3 change test/integration support, close 09 from one final committed
snapshot.

Run:

~~~sh
python3 tools/architecture/test_check_boundaries.py
python3 tools/architecture/check_boundaries.py
python3 tools/architecture/test_check_expert_extensibility.py
python3 tools/architecture/check_expert_extensibility.py
python3 tools/architecture/test_check_connection_observe_conformance.py
python3 tools/architecture/check_connection_observe_conformance.py

cargo metadata --no-deps --format-version 1
cargo check --workspace --lib
CARGO_INCREMENTAL=0 cargo test --workspace --no-fail-fast -- --test-threads=1
cargo build -p floe-ffi

(
  cd server
  go test ./...
  go test -race ./...
  go vet ./...
)

cargo test -p floe-provider-adapters --test live_server_access

git diff --check
~~~

Then repeat the Flutter/Apple gates in §§17.6-17.7 against that exact snapshot if any
source/test-support commit landed after their previous run.

No OAuth/model ignored test becomes required.

### 17.9 09-R7 — Residual audit and closure evidence

Run:

~~~sh
rg -n "native Calendar unavailable" apps/client/test apps/client/integration
rg -n "pending native source" apps/client/test apps/client/integration
rg -n "FixtureCalendarAdapter|calendar_gateway_test\.dart" apps/client/test apps/client/integration
rg -n "libfloe_eventkit\.dylib|NativeCalendarFixture" apps/client tools crates/adapters/providers/tests
git diff --check
git status --short --branch
~~~

Required final classification:
- no standard Flutter unit test depends on a missing bundle-native dylib;
- native fixture dylib use is bounded to validation/integration support and existing
  provider tests;
- no production source/permission bypass exists;
- the stale “pending native source” test concept is gone;
- golden image remains untouched and explicitly non-gating.

Append a **Residual closure** subsection under execution evidence with:
1. residual-start HEAD and fetched `origin/main`;
2. implementation/test-harness commit SHA(s);
3. exact files moved/deleted/added;
4. proof production Rust/App permission code was unchanged, or exact defect fix if one
   was genuinely required;
5. three migrated bundle-native scenario results;
6. Action contract reconciliation: stale proposal-fence test/child scenario removed and existing Rust fence regressions retained;
7. non-golden Flutter result/count;
8. golden result classified IGNORED;
9. macOS build;
10. local server pairing integration;
11. four Swift package results;
12. `check-native.sh`;
13. three architecture checker suites;
14. broad Rust;
15. Go test/race/vet;
16. loopback server;
17. live smoke SKIPPED reason;
18. final clean worktree.

When and only when all deterministic **non-golden** gates pass:
- change parent README 09 status to `Complete`;
- mark this plan Complete;
- commit closure;
- stop before plan retirement.

Suggested closure commit:

~~~text
docs: complete connection observe checkpoint 09
~~~

### 17.10 Residual acceptance matrix

Checkpoint 09 may close when:

1. the three real bundle-dependent Calendar scenarios have been re-homed to a
   deterministic native fixture and pass;
2. the stale user-proposal evidence-fence scenario is deleted from both the standard
   Flutter test and child fixture because current contract intentionally permits a
   Pending user proposal without Context evidence;
3. the existing Rust execution/Expert evidence-fence regressions pass;
4. source establishment still executes `probe_calendar_source`;
5. no production native-loader/source/grant bypass was introduced;
6. all non-golden Flutter tests pass;
7. Registry golden mismatch, if still present, is reported and ignored per operator
   instruction;
8. `flutter build macos` passes;
9. local server pairing integration passes or has a concrete environment blocker that
   did not require destructive credential changes;
10. Contacts/Health/Feasibility/ScreenTime Swift tests pass;
11. `check-native.sh` passes;
12. three architecture checker suites pass;
13. broad serialized Rust passes;
14. Go test/race/vet pass;
15. disposable `live_server_access` passes;
16. no newly unexplained production residual exists;
17. README marks 09 Complete;
18. worktree is clean;
19. plan retirement is still not performed.

### 17.11 Residual agent report format

Report only:

1. residual start HEAD / first and final fetched origin-main / final HEAD;
2. residual implementation and closure SHAs;
3. exact Calendar test-harness design;
4. production code changed or unchanged;
5. files added/moved/deleted;
6. Day mirror/source continuity result;
7. AllAvailable 1→2 source-authority result;
8. wide-source bounded-publication result;
9. Action contract reconciliation: stale Flutter/child proposal-fence scenario removed;
10. existing Rust Action execution/Expert evidence-fence regressions result;
11. non-golden Flutter full-suite result/count;
12. golden result as IGNORED;
13. Flutter analyze result;
14. macOS build result;
15. local server pairing integration result;
16. Apple Contacts result;
17. Apple Health result;
18. FeasibilityProvider result;
19. ScreenTimeGate result;
20. `check-native.sh` result;
21. architecture checker/test-suite result;
22. broad Rust/check result;
23. Go test/race/vet result;
24. disposable loopback result;
25. live smoke SKIPPED reason;
26. residual search result;
27. README 09 Complete confirmation;
28. clean worktree;
29. confirmation plan bundle/active-plan pointer were not retired.
