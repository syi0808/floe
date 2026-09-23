# Checkpoint 03-D — deletion, verification and documentation convergence

- **Execution baseline:** Checkpoint 03-C completion on top of main bbce2fa2ae2da4bffb05185f39d8d50aa3e6b82a
- **Depends on:** 03-A, 03-B and 03-C complete
- **Goal:** prove one source authority, delete transition-only surfaces, update current architecture docs and run the full applicable verification gates.

03-D must not introduce a new permission model or compatibility layer. If a removed symbol still has a production caller, migrate that caller to the canonical owner and delete the old symbol.

---

## 1. Expected final topology

~~~text
Expert Registry
  packages / installations / assignments
  Expert private state
          |
          | enabled Expert identity only
          v
BuiltinExpertEndpoint
  -> domain dispatch
  -> Context source request
          |
          v
Connections
  current connection / selected resources / source health
          |
          v
Access
  DataAccessGrant
  CalendarGrantPolicy
          |
          v
Context
  provider-neutral view + ContextDependency
~~~

No source permission arrow points back into Registry.

---

## 2. Required deletions

### 2.1 Experts Calendar vertical

Expected files gone:

~~~text
crates/modules/experts/src/registry/calendar_setup.rs
crates/modules/experts/src/calendar_access.rs
~~~

Expected exported types/functions gone:

~~~text
CalendarExpertSetup
CalendarExpertSetupResult
CalendarExpertSetupReceipt
CalendarExpertOverview
CalendarAccessConfiguration  # old Experts owner
CalendarAccessChange         # old Experts owner
CalendarViewBinding
CalendarSetupStore
CalendarAccessSource
AdmittedCalendarSource
install_calendar_expert
apply_calendar_access
calendar_expert_overview
register_calendar_view
set_calendar_view_enabled
calendar_view
~~~

ExpertPackaging may remain only after moving to a source-independent Experts file if the generic built-in package setup still needs it.

### 2.2 Registry source authority

Expected gone:

~~~text
BuiltinSourceBinding
BuiltinSourceState
BuiltinSourceEvidence
SourceGrants
assignment_source_grant
assignment_has_mandatory_source
refresh_builtin_expert_sources
builtin_source_bindings
granted_view_handles
require_mandatory_source
source_grant
source_granted
RegistryConfigurationTarget::CalendarView
~~~

RegistrySnapshot has no:

~~~text
calendar_views
calendar_setups
revoked_calendar_setups
~~~

### 2.3 Calendar grant mapping persistence

Expected tables/symbols gone:

~~~text
calendar_grant_schema
calendar_grant_mappings
remote_calendar_grant_schema
remote_calendar_grant_mappings
CalendarGrantMapping
RemoteCalendarGrantMapping
calendar_grant_connection_id
install_calendar_expert_with_connection
~~~

Fresh profile contains only canonical DataAccessGrant storage plus the minimal CalendarGrantPolicy metadata store.

Generic remote_view_grant_mappings is allowed and must not be deleted merely because its name contains grant mapping.

### 2.4 Expert result fake view identity

Expected old field names gone from current contracts:

~~~text
ExpertResult.view_handle
ExpertFocusProposal.view_handle
AgentActionOrigin.view_handle
~~~

The final evidence identity is source-observation based and is validated against ContextDependency.

### 2.5 App/protocol/FFI

Expected gone:

~~~text
WorkerAction::CalendarExperts
WorkerResult.calendar_experts
CalendarExpertInstall
ExpertCommand::InstallCalendar
ExpertInspection::Calendar
experts.calendar.install
experts.calendar.inspect
CalendarExpertInstallDto
CalendarExpertOverviewDto
calendar_experts
RegistryConfigurationTargetDto::CalendarView
~~~

Calendar Access commands/results that remain must be Access-owned and contain no Registry/setup identity.

### 2.6 Flutter

Expected files gone:

~~~text
apps/client/lib/features/experts/application/agent_calendar_expert_controller.dart
apps/client/lib/features/experts/domain/agent_calendar_experts.dart
apps/client/lib/features/experts/presentation/agent_calendar_expert_dialog.dart
apps/client/test/features/experts/agent_calendar_expert_controller_test.dart
apps/client/test/features/experts/agent_calendar_expert_dialog_test.dart
apps/client/test/features/experts/agent_calendar_experts_test.dart
apps/client/test/support/agent_calendar_experts.dart
~~~

AgentController has no Calendar-Expert controller/gateway.

---

## 3. Repository-wide residual audit

Run exact searches from repository root.

### Registry source authority

~~~sh
rg -n 'BuiltinSourceBinding|BuiltinSourceState|BuiltinSourceEvidence|SourceGrants|assignment_source_grant|assignment_has_mandatory_source|refresh_builtin_expert_sources|builtin_source_bindings|require_mandatory_source|source_granted\(|source_grant\(' .
~~~

Expected: zero production matches. Completed plan docs/history references may remain only when clearly historical.

### Calendar Expert vertical

~~~sh
rg -n 'CalendarExpertSetup|CalendarExpertOverview|CalendarViewBinding|CalendarSetupStore|install_calendar_expert|apply_calendar_access|calendar_experts|experts\.calendar\.' .
~~~

Expected: zero production matches.

### Registry Calendar collections

~~~sh
rg -n 'calendar_views|calendar_setups|revoked_calendar_setups|CalendarView \{' crates apps/client
~~~

Inspect each match. Context/Day Calendar views are legitimate; Registry snapshot/setup meanings are not.

### Grant mappings

~~~sh
rg -n 'calendar_grant_mappings|remote_calendar_grant_mappings|CalendarGrantMapping|RemoteCalendarGrantMapping' crates server apps
~~~

Expected: zero. Do not count remote_view_grant_mappings.

### Evidence identity

~~~sh
rg -n 'view_handle' crates/contracts/agent crates/modules/actions crates/modules/experts crates/app apps/client
~~~

Expected: no old Expert result/proposal/action-origin field. A domain-unrelated view handle must be justified individually.

### Calendar Expert Flutter

~~~sh
rg -n 'AgentCalendarExpert|AgentCalendarSettings|agent_calendar_expert|calendarExperts' apps/client
~~~

Expected: zero production matches.

### Calendar wire

~~~sh
rg -n 'experts\.calendar\.install|experts\.calendar\.inspect|setup_id|calendar_experts' crates/bindings apps/client
~~~

Expected: old Calendar Expert/setup matches zero. Unrelated setup_id fields outside Calendar are allowed after inspection.

---

## 4. Architecture dependency audit

Inspect Cargo manifests and tools/architecture/module-dependencies.json.

Required final boundaries:

- floe-experts does not depend on Access, Connections, Context provider adapters or native Calendar;
- floe-access does not depend on built-in Expert catalogue;
- floe-context requests access through Access contracts and does not inspect Registry;
- floe-app composition is the only place deriving built-in first-party consumer policy;
- floe-vault persists owner records but does not reinterpret Registry state as authorization;
- Flutter presents Access/connection state and never computes grant admission;
- Actions consume ContextDependency/current authority and do not inspect CalendarExpertSetup.

Do not add an architecture allow-list edge merely to keep a deleted owner relationship alive.

---

## 5. Final behavior acceptance

### A. Expert availability independent of source

1. fresh profile with built-in Experts installed;
2. no Calendar grant;
3. Schedule remains in enabled Expert cards/Directory;
4. asking a Calendar question delegates to Schedule;
5. Context/Access reports expected unavailable/review-required state;
6. Registry revision does not change because source is absent.

### B. Fresh native Calendar grant

1. current EventKit connection/resources;
2. explicit Checkpoint 03 Access review;
3. active DataAccessGrant with canonical first-party consumers;
4. CalendarGrantPolicy with native reviewed subject;
5. Schedule read succeeds as floe.builtin.schedule;
6. ContextDependency reauthorizes after Vault reopen.

### C. Pause/review

1. pause the native grant;
2. Schedule card remains enabled;
3. new Schedule read is blocked by Access, not Registry;
4. Registry revision unchanged;
5. fresh review reactivates under GrantAuthority CAS;
6. old dependency remains invalid.

### D. Source drift

1. change source authority/native subject;
2. old grant/policy does not admit;
3. no Registry mutation attempts to repair it;
4. Access requires fresh review.

### E. Remote Calendar

1. paired producer and concrete Google/Microsoft Calendar source;
2. explicit review creates DataAccessGrant + CalendarGrantPolicy;
3. exact resource/consumer read succeeds;
4. second resource may have its own grant without ambiguity;
5. pause/reopen/source drift behave fail-closed.

### F. /focus proposal

1. successful Schedule read yields ContextDependency observation;
2. Expert result/proposal evidence_id binds that observation;
3. proposal inspection/publication reauthorizes current grant/policy/source;
4. Registry Calendar view state is not consulted;
5. Act authority remains separately enforced.

### G. Registry operations

1. disabling Schedule assignment removes Schedule card;
2. Calendar grant remains unchanged;
3. re-enabling Schedule restores card;
4. source grant state did not change.

### H. Restart

1. active grant/policy survives reopen;
2. paused grant survives reopen;
3. corrupt/missing policy fails closed;
4. obsolete old Calendar authorization schema is rejected/reset, not migrated.

---

## 6. Documentation convergence

Update current docs after code is final.

### docs/architecture/runtime.md

Document:

- Registry is source-independent;
- built-in card availability is Expert state only;
- Context/Access decides source reads at invocation;
- Calendar grant/policy are connection/source-owned;
- result/proposal evidence is ContextDependency observation identity.

Remove references to old Registry Calendar mapping/setup authority.

### docs/architecture/modules.md

Clarify:

- Experts: package/assignment/private-state only;
- Connections: current source/resources;
- Access: Observe grant + policy authority;
- Context: source acquisition/provenance;
- App: product composition and first-party policy derivation;
- Flutter: presentation.

### docs/architecture/authority-recovery.md

Clarify source recovery no longer mutates Registry. Revocation/pause/source drift affect Access authority and later dependency admission only.

### docs/development/plans/expert-access-interaction

- mark Checkpoint 03 complete;
- record completion baseline and semantic commits;
- mark Checkpoint 04 next;
- keep these files temporary execution history, not current architecture authority.

Do not prematurely implement the Checkpoint 04/05 product behavior in docs.

---

## 7. Full verification gate

### Rust / architecture

~~~sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

### FFI / protocol

~~~sh
cargo build -p floe-ffi
~~~

Also run protocol and FFI ABI/app-wire tests currently documented by the repository.

### Flutter / macOS

~~~sh
cd apps/client
flutter analyze
flutter test
flutter build macos
~~~

If native EventKit/provider files changed during 03-B, run the repository's focused macOS/native Calendar tests once after the final cutover.

### Server

If remote Calendar protocol/server code changed:

~~~sh
cd server
go test -race ./...
go vet ./...
~~~

### Persistence

Use fresh isolated development profiles for:

- fresh Vault create/reopen;
- native grant review/pause/review;
- remote grant review/pause;
- source drift;
- policy corruption rejection;
- old schema incompatibility/reset behavior.

Never run destructive reset against the operator's normal profile.

---

## 8. Final Checkpoint 03 acceptance checklist

- [ ] Expert Registry contains no source permission snapshot.
- [ ] built-in Expert catalogue availability is independent of sources.
- [ ] Registry contains no Calendar setup/view authority.
- [ ] DataAccessGrant is the sole Calendar Observe grant authority.
- [ ] one CalendarGrantPolicy path owns consumer-policy/native-review metadata.
- [ ] native and remote Calendar share that policy path.
- [ ] no Calendar grant mapping duplicates source/scope/Registry ids.
- [ ] source/dependency reauthorization does not inspect Registry.
- [ ] Expert result/proposal evidence uses Context observation identity.
- [ ] /focus action flow remains governed and reauthorized.
- [ ] Experts public API is Registry-only.
- [ ] Calendar Observe public API is Access/connection-owned.
- [ ] experts.calendar.* wire is gone.
- [ ] Flutter Calendar-Expert management surface is gone.
- [ ] source permission changes do not mutate Registry revision.
- [ ] Expert enable/disable does not mutate source grants.
- [ ] old authorization state is not migrated.
- [ ] residual searches are clean.
- [ ] Rust/architecture/FFI/Flutter/macOS gates pass.
- [ ] Go gates pass if server changed.

---

## 9. Final agent report

Report:

1. **Final authority topology** — Registry vs Connections vs Access vs Context.
2. **Deleted source authority** — source fields/APIs/setup vertical.
3. **Persistence result** — final tables/records and lookup semantics.
4. **Public surface result** — App/worker/protocol/FFI/Flutter changes.
5. **Evidence/proposal result** — final evidence identity and /focus reauthorization.
6. **Legacy/reset behavior** — proof no authorization migration occurs.
7. **Residual audit** — exact commands and remaining justified matches.
8. **Verification** — exact command outputs/results.
9. **Next checkpoint** — Checkpoint 04 only after every item above is complete.

Do not mark Checkpoint 03 complete while any source-access decision, Calendar setup identity or Calendar grant mapping remains under Expert Registry authority.