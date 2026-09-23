# Checkpoint 03 — Expert Registry and Access authority convergence

- **Status:** residual active
- **Execution baseline:** main at dd4652997b9bd88668bcd59e98a2f3d1a1300368 (plan commit; code baseline bbce2fa2ae2da4bffb05185f39d8d50aa3e6b82a)
- **Landed commits:** 03-A `3ca515af`, 03-B `7cda37e8`, 03-C `a725739c`, 03-D `aaefc659`. A post-implementation audit reopened bounded residual work in 03-E.
- **Next:** 03-E residual closure; Checkpoint 04 remains blocked
- **Precondition:** Checkpoint 02 is complete. Schedule already runs only through BuiltinExpertEndpoint and Context/Access.
- **Scope:** remove source-access authority from Expert Registry, remove the Calendar-Expert setup vertical, replace Calendar setup/mapping persistence with source-owned Access records, and remove the old App/wire/Flutter Calendar-Expert management path.
- **Compatibility posture:** pre-stable. Do not preserve old Registry/Calendar setup wire or migrate disposable local authorization state.

This file is the authoritative index for Checkpoint 03. Execute the child plans in order:

1. [03-A — Registry source-authority removal](03-a-registry-source-authority.md)
2. [03-B — Calendar Access persistence convergence](03-b-calendar-access-persistence.md)
3. [03-C — App, protocol and Flutter ownership cutover](03-c-app-protocol-flutter-cutover.md)
4. [03-D — deletion, verification and documentation convergence](03-d-deletion-verification.md) — landed at `aaefc659`
5. [03-E — residual authority and Calendar access closure](03-e-residual-authority-calendar-access.md) — active

03-A through 03-D are landed. Execute only 03-E from the aaefc659 baseline unless a 03-E stop condition proves an earlier ownership assumption wrong. Do not restore the deleted Calendar-Expert surface. Do not start Checkpoint 04 product permission redesign or Checkpoint 05 conversation interactions until 03-E closes.

---

## 1. Why this checkpoint exists

Checkpoint 02 removed Schedule-specific execution, but two source-access authorities still coexist:

~~~text
Expert Registry
  builtin source bindings
  granted_view_handles
  mandatory-source card gating
  CalendarViewBinding / CalendarExpertSetup

Access
  DataAccessGrant
  source authority
  resource scope
  consumer set
  processing restriction
~~~

That duplication still creates states where Registry says a source is granted while Access denies it, or where a source/connection change rewrites Registry state even though the Expert package itself has not changed.

Checkpoint 03 makes the ownership unambiguous:

~~~text
Experts / Registry
  package identity
  installation enabled
  assignment enabled
  private Expert state

Connections
  connection identity
  selected source resources
  connection/source health

Access
  DataAccessGrant
  source binding
  exact resources
  consumers
  purpose / processing
  grant authority
  consumer-policy authority
  reviewed native subject continuity

Context
  actual source acquisition
  exact ContextDependency
~~~

An Expert declaration may still say that Calendar or Mail is required. That declaration is static product metadata in floe-experts-builtin. It is not persisted as a runtime permission decision in Registry.

---

## 2. Current baseline defects

At bbce2fa the following production surfaces still exist.

### Registry duplication

- crates/modules/experts/src/registry/expert_setup.rs: BuiltinExpertSetup.sources, refresh_builtin_expert_sources, mandatory-source filtering, assignment_source_grant, SourceGrants.
- crates/modules/experts/src/builtin_setup.rs: BuiltinSourceEvidence and source-refresh orchestration.
- crates/modules/experts/src/registry.rs: BuiltinSourceBinding, BuiltinSourceState, granted_view_handles, calendar_views, calendar_setups and revoked_calendar_setups.
- crates/app/src/vault_host.rs: ensure_builtin_experts still computes builtin_source_bindings from connection/server availability.
- crates/app/src/vault_host/conversation_turn/expert_dispatch.rs: common host still receives SourceGrants.
- crates/experts/builtin/src/host.rs: source_grant, source_granted and require_mandatory_source still preflight Registry state.

### Calendar-Expert vertical

- crates/modules/experts/src/registry/calendar_setup.rs
- crates/modules/experts/src/calendar_access.rs
- crates/app/src/vault_host/calendar_access.rs::VaultCalendarSetups
- WorkerAction::CalendarExperts and the ExpertCommand::InstallCalendar / ExpertInspection::Calendar path.
- RegistryConfigurationTarget::CalendarView.

### Calendar persistence duplication

- native calendar_grant_mappings stores setup/view/install/assignment ids plus source/grant/policy state.
- remote_calendar_grant_mappings separately copies source/scope/policy state.
- data_access_grants already stores the canonical grant source/scope/state.
- Context dependency reauthorization consults the Calendar mapping tables for consumer-policy authority.

### Product/wire duplication

- experts.calendar.install
- experts.calendar.inspect
- access.calendar.configure still carries Registry instance/revision/setup ids.
- LocalAccessResult / ExpertOperationResult still return calendar_experts.
- Flutter AgentCalendarExpertController, AgentCalendarExperts and AgentCalendarSettings remain wired into AgentController and ConnectorScreen.

---

## 3. Final state after Checkpoint 03

### 3.1 Registry

RegistrySnapshot contains no Calendar source binding or Calendar setup collections.

Built-in setup is source-independent:

~~~text
BuiltinExpertSetup
  instance_id
  expected_revision
  setup_id

BuiltinExpertSetupReceipt
  setup_id
  person_id
  expected_revision
  assignments

BuiltinExpertAssignmentReceipt
  expert
  tool_installation_id
  expert_installation_id
  tool_assignment_id
  expert_assignment_id
~~~

No source state, mandatory-source runtime state or granted view handle is stored in those receipts.

An enabled built-in Expert card depends only on package/installation/assignment integrity and execution availability. Missing source access does not hide the card.

### 3.2 Evidence identity

Built-in Expert results must not use a Registry-granted Calendar view handle as evidence authority.

Replace the remaining fake view identity with source evidence identity:

~~~text
ExpertResult.view_handle       -> evidence_id
ExpertFocusProposal.view_handle -> evidence_id
AgentActionOrigin.view_handle   -> evidence_id
~~~

For the stateful Schedule result, evidence_id is the exact ContextDependency observation identity that backs the result/proposal. Registry validates Expert identity/private-state settlement; Access/Context validates source evidence.

Do not keep a random or synthetic Registry view UUID merely to satisfy the old result schema.

### 3.3 Calendar Access persistence

DataAccessGrant is the canonical Calendar Observe record. Calendar-specific persistence may store only authority metadata not already represented by the grant.

Target adapter record:

~~~text
CalendarGrantPolicy
  grant_id
  person_id
  consumer_policy_authority
  reviewed_native_subject_fingerprint?   # native only
~~~

The policy row does not duplicate setup id, Registry revision, installation ids, assignment ids, source binding or grant scope.

Native and remote Calendar use the same policy record. Source/scope are read from DataAccessGrant.

### 3.4 Source lookup

Calendar lookup is source-owned:

~~~text
current connection/source
  -> bounded DataAccessGrant query by exact source identity
  -> exact resource / consumer / purpose / processing admission
  -> CalendarGrantPolicy by grant_id
~~~

Native may have one current grant covering its selected resource set. Remote may have multiple grants for one source, one per resource. Lookup must be bounded and must reject ambiguity.

### 3.5 App / protocol / Flutter

Experts APIs manage Registry only.

Calendar Observe management is Access-owned and connection/source keyed. No public command contains setup_id or expected Registry revision.

The old Calendar-Expert Flutter feature is deleted. The connection screen may temporarily retain equivalent review/enable/scope/remove controls through an Access-owned connection surface; Checkpoint 04 later simplifies that surface to the final Use with Floe product model.

---

## 4. Important scope decisions

### 4.1 Consumer policy from Checkpoint 02 is frozen

Reuse calendar_first_party_consumers from product composition. Access/Vault must not duplicate the built-in catalogue or revive calendar.expert.

### 4.2 Checkpoint 04 is not pulled forward

Do not make Connect automatically create Observe grants yet. Do not introduce the final Use with Floe switch in this checkpoint. Preserve current explicit review/pause semantics while moving them to the correct owner.

### 4.3 Checkpoint 05 is not pulled forward

Do not add durable ConversationInteraction or inline chat permission cards here.

### 4.4 Remote non-Calendar view grants are out of scope

remote_view_grant_mappings belongs to generic remote views, not the removed Calendar-Expert vertical. Do not refactor it unless a Calendar-specific compilation dependency forces a small mechanical change.

### 4.5 No authorization migration

Never convert Registry enabled/source bits or old CalendarExpertSetup records into an active DataAccessGrant.

Old development profiles containing the obsolete Calendar setup/mapping schema are incompatible authorization state. Fail explicitly and use a fresh/reset Floe-owned development profile. Never silently broaden or resurrect access.

---

## 5. Checkpoint sequencing

### 03-A — Registry source-authority removal

Make built-in Registry state independent of sources before removing Calendar-specific persistence.

Exit gate:

- no BuiltinSourceBinding / BuiltinSourceState / SourceGrants;
- no Registry mandatory-source card gating;
- built-in setup never refreshes when a connector changes;
- common built-in host has no source_grant/source_granted preflight;
- built-in assignments carry no source view authority;
- evidence identity is no longer a Registry Calendar view handle.

### 03-B — Calendar Access persistence convergence

Replace native and remote Calendar mapping stores with DataAccessGrant + CalendarGrantPolicy.

Exit gate:

- no calendar_grant_mappings;
- no remote_calendar_grant_mappings;
- no setup/assignment/install ids in Calendar access persistence;
- current native/remote read and dependency reauthorization work from source-owned records only;
- old profiles are rejected/reset, not migrated into grants.

### 03-C — App/protocol/Flutter ownership cutover

Delete the Calendar-Expert management API and replace the local Calendar controls with Access-owned source/grant APIs.

Exit gate:

- no experts.calendar.install / experts.calendar.inspect;
- no CalendarExpertSetup/Overview/AccessConfiguration product contract;
- no WorkerAction::CalendarExperts;
- no CalendarView Registry configuration target;
- no AgentCalendarExpertController / AgentCalendarSettings;
- connector detail still has an owner-correct way to review/pause/update/remove Calendar access without Checkpoint 04 UX redesign.

### 03-D — deletion and verification

Delete remaining old files/types/tests, update current docs, perform residual searches and full Rust/FFI/Flutter/macOS gates.

---


### 03-E — residual authority and Calendar access closure

Post-03-D audit of aaefc659 found four bounded acceptance gaps:

- remote Calendar source-wide uniqueness rejects valid sibling resource grants;
- native existing-grant mutations do not carry the GrantAuthority the Person reviewed;
- ConsumerPolicyAuthority lifecycle/error handling is incomplete;
- native Calendar Observe management has subject preview but no Access-owned mutation/overview product path.

Execute [03-E](03-e-residual-authority-calendar-access.md) before Checkpoint 04.

Exit gate:

- remote sibling resources coexist and exact-resource duplicates fail closed;
- native and remote existing-grant mutation is bound to reviewed CAS expectation;
- ConsumerPolicyAuthority is stable only for an exact semantic no-op and policy errors fail closed;
- native Calendar has one Access-owned inspect/review/pause/remove path with no Registry identity;
- the full 03-E regression and repository verification gates pass.

## 6. Global invariants during implementation

- Registry enablement and Access admission are separate questions.
- an enabled Expert remains in the Manager catalogue when its source is unavailable or paused.
- Access never imports floe-experts-builtin.
- Experts never imports provider/native adapters.
- App composition may derive first-party consumer policy because it knows product declarations and Access contracts.
- no Registry state authorizes a source read.
- no Flutter/FFI payload chooses grant authority, source authority or native subject beyond an explicitly reviewed expected value; backend reloads current state before mutation.
- source/grant changes use CAS and fresh source validation.
- ConsumerPolicyAuthority changes when the approved consumer policy changes and remains stable on semantic no-op review.
- a ContextDependency is reauthorized against current DataAccessGrant and current CalendarGrantPolicy.
- Observe changes never mutate ActionAuthority.
- no global transaction is held during provider/native/model I/O.

---

## 7. Stop conditions

Stop and update the plan instead of adding a workaround if implementation appears to require:

1. keeping SourceGrants only for Schedule/Calendar compatibility;
2. hiding source availability inside enabled_expert_cards;
3. making Access import the built-in catalogue;
4. retaining CalendarExpertSetup as an opaque wrapper around DataAccessGrant;
5. renaming calendar_grant_mappings while keeping setup/assignment/install ids;
6. treating a Registry view handle as source authorization after 03-A;
7. silently upgrading an old Calendar setup into a new grant;
8. preserving experts.calendar.* as deprecated aliases;
9. implementing Checkpoint 04 Use with Floe behavior early;
10. implementing Checkpoint 05 durable user interactions early.

---

## 8. Required verification per child plan

Each child plan must run its targeted tests and report residual searches. Do not wait until 03-D to discover an old authority still has callers.

Broad checkpoint gate at 03-D:

~~~sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
cargo build -p floe-ffi
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

Flutter/client gate after 03-C:

~~~sh
cd apps/client
flutter analyze
flutter test
flutter build macos
~~~

If server code changes, also run from server/:

~~~sh
go test -race ./...
go vet ./...
~~~

Use a fresh isolated development profile for persistence/macOS acceptance because old authorization state is intentionally not migrated.

---

## 9. Final Checkpoint 03 definition of done

- [x] Registry snapshot contains no Calendar setup/view/revocation collections.
- [x] BuiltinExpertSetup/Receipt contains no source bindings or runtime source state.
- [x] BuiltinExpertAssignmentReceipt contains no required-source permission snapshot or granted view handles.
- [x] SourceGrants, assignment_source_grant and assignment_has_mandatory_source are deleted.
- [x] builtin source refresh orchestration and builtin_source_bindings are deleted.
- [x] built-in Expert cards are source-independent.
- [x] common built-in host has no Registry source preflight.
- [x] source availability is discovered only by Context/Access reads.
- [x] Registry CalendarView configuration target is deleted.
- [x] CalendarExpertSetup/Receipt/Overview/AccessConfiguration/AccessChange are deleted.
- [x] crates/modules/experts/src/registry/calendar_setup.rs is deleted.
- [x] crates/modules/experts/src/calendar_access.rs is deleted.
- [x] Calendar grant persistence contains no Registry setup/install/assignment identity.
- [ ] native and remote Calendar reads select the exact DataAccessGrant by current source/resource/consumer without rejecting valid sibling resources.
- [x] Calendar consumer-policy/native-review metadata is keyed by grant/source authority, not Expert setup.
- [x] calendar_grant_mappings and remote_calendar_grant_mappings are deleted.
- [x] old Calendar grant/setup authorization state is not migrated.
- [x] Expert result/proposal evidence no longer depends on Registry Calendar view authority.
- [x] experts.calendar.install and experts.calendar.inspect are deleted.
- [x] access.calendar commands contain no Registry instance/revision/setup id.
- [x] WorkerResult / App result no longer expose calendar_experts.
- [x] AgentCalendarExpertController, AgentCalendarExperts and AgentCalendarSettings are deleted.
- [ ] connector detail uses an Access/Connections-owned native Calendar Observe management path; the deleted Expert Calendar UI is not restored.
- [x] Schedule success, missing-access and /focus proposal flows still pass.
- [x] canonical first-party Calendar consumers remain exact real package identities.
- [ ] existing-grant Calendar mutations honor the GrantId/GrantAuthority state the Person reviewed.
- [ ] ConsumerPolicyAuthority is stable on exact semantic no-op and advances on reviewed policy change; policy load failures fail closed.
- [ ] full Rust/architecture/FFI/Flutter/macOS gates pass after 03-E.

---

## 10. Required agent report

Report Checkpoint 03 only after 03-A through 03-D are complete:

1. **Authority result** — final Registry, Access, Context, Connections ownership.
2. **03-A Registry cleanup** — deleted source fields/APIs and evidence identity changes.
3. **03-B persistence** — final Calendar grant/policy tables and current-source lookup.
4. **03-C surface cutover** — App, worker, protocol, FFI and Flutter removals/replacements.
5. **Residual audit** — exact searches and justified remaining matches.
6. **Persistence policy** — how old profiles fail/reset and proof no authorization migration exists.
7. **Verification** — exact commands and results.
8. **Blocker** — any unmet item means the checkpoint remains open.

Do not report Checkpoint 03 complete while any production Calendar source access depends on Registry setup/view state.