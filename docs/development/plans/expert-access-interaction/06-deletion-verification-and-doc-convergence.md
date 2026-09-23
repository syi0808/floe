# Checkpoint 06 — obsolete-path deletion, verification and documentation convergence

## Goal

Prove that the repository has converged to one architecture and remove every transition-only surface left by checkpoints 01–05.

This checkpoint is not a place to add new compatibility wrappers. If an obsolete symbol still has a production caller, migrate that caller and delete the symbol.

Completion requires:

- one built-in Expert runtime path;
- one Observe authority;
- one connection permission editor;
- one assistant interaction lifecycle;
- one provenance-driven history path;
- no old Schedule/Calendar Expert vertical;
- no old Settings LLM/source permission path;
- no stale architecture/product docs describing the removed design;
- full affected-surface verification.

## 1. Final topology audit

The repository must match this topology:

~~~text
Manager Conversation
  |
  +-> Manager Tools --------------------------+
  |                                          |
  +-> TaskCoordinator -> BuiltinExpertEndpoint
                           |
                           +-> Schedule
                           +-> Commitments
                           +-> Communication
                           +-> Relationships
                           +-> FocusAttention
                           +-> Wellbeing
                           +-> WorkContext
                           +-> LifeLogistics
                                      |
                                      v
                                  Context
                                      |
                               Access / Grant
                                      |
                           Connection/source adapter

NeedsUserAction
  -> Conversation interaction record
  -> Manager response
  -> Flutter inline interaction
  -> Connections/Access owner mutation
  -> linked follow-up Run
~~~

Schedule has no parallel vertical.

## 2. Required production deletions

Delete obsolete files when still present.

### 2.1 Schedule App vertical

Expected gone:

~~~text
crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule.rs
crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule/
~~~

No ScheduleEndpoint, CalendarExpertEndpointRequest/Result, select_active_setup or schedule-only source composition may remain.

### 2.2 Schedule infrastructure files in built-in package

Expected gone/replaced:

~~~text
crates/experts/builtin/src/schedule/definition.rs
crates/experts/builtin/src/schedule/calendar_history.rs
crates/experts/builtin/src/schedule/host.rs
~~~

If schedule/host.rs still exists, it must be pure Schedule domain logic and should be renamed expert.rs unless “host” is semantically accurate. There must be no provider/Access/App composition in the Expert package.

### 2.3 Calendar Expert Registry/access vertical

Expected gone:

~~~text
crates/modules/experts/src/registry/calendar_setup.rs
crates/modules/experts/src/calendar_access.rs
~~~

Expected symbols gone:

~~~text
CalendarExpertSetup
CalendarExpertSetupResult
CalendarExpertOverview
CalendarAccessConfiguration
CalendarAccessChange
CalendarViewBinding     # if only old Expert setup meaning
install_calendar_expert
apply_calendar_access
calendar_expert_overview
~~~

### 2.4 Schedule/Registry-coupled grant persistence

Expected obsolete concepts gone:

~~~text
calendar_grant_mappings
calendar_grant_connection_id
install_calendar_expert_with_connection
setup_id as Access grant lookup identity
tool_installation_id in Calendar grant identity
expert_installation_id in Calendar grant identity
tool_assignment_id in Calendar grant identity
expert_assignment_id in Calendar grant identity
~~~

Calendar-specific native subject evidence may remain only when keyed by Access/source authority.

### 2.5 Registry source permission

Expected gone from built-in permission decisions:

~~~text
SourceGrants
BuiltinSourceBinding
BuiltinSourceState
BuiltinSourceEvidence
assignment_source_grant
assignment_has_mandatory_source
refresh_builtin_expert_sources
~~~

If a similarly named generic type remains, demonstrate that it is not an Observe authority and is required by a real current consumer.

### 2.6 Calendar Expert wire/API

Expected gone:

~~~text
experts.calendar.install
experts.calendar.inspect
CalendarExpertInstallDto
CalendarExpertInstall
ExpertCommand::InstallCalendar
ExpertInspection::Calendar
calendar_experts result field
~~~

No deprecated aliases.

### 2.7 Flutter Calendar Expert permission vertical

Expected deleted:

~~~text
apps/client/lib/features/experts/application/agent_calendar_expert_controller.dart
apps/client/lib/features/experts/domain/agent_calendar_experts.dart
apps/client/lib/features/experts/presentation/agent_calendar_expert_dialog.dart
apps/client/test/features/experts/agent_calendar_expert_controller_test.dart
apps/client/test/features/experts/agent_calendar_expert_dialog_test.dart
apps/client/test/features/experts/agent_calendar_experts_test.dart
apps/client/test/support/agent_calendar_experts.dart
~~~

Keep generic Expert Registry UI only if the product still intentionally exposes Expert enablement.

### 2.8 Settings permission duplication

Expected gone or non-editing:

- external model “Allow external model providers” settings toggle;
- editable Calendar Expert access under Data & privacy;
- ordinary source grant review/pause controls under server pairing settings;
- duplicate Use with Floe toggles outside the owning connection.

Action permissions remain.

## 3. Conversation/history cleanup

### 3.1 Remove SourceHistoryBoundary if canonical coverage is sufficient

Search:

~~~text
SourceHistoryBoundary
narrow_by_source_boundary
carries_source_history
bounded_source_history_start
CalendarHistoryBoundary
~~~

The final canonical history projection should use recorded coverage/dependencies.

Delete the boundary abstraction if it only exists to infer provenance from capability/agent names.

Required regression:

1. successful Calendar source read creates dependency;
2. Manager answer derived from it is recorded under that coverage;
3. grant revoked;
4. later turn does not see source-derived history;
5. user-authored historical messages remain;
6. no string check for “calendar.” or “floe.builtin.schedule” participates.

If a fallback boundary remains for a real unproven legacy message class, that means the architecture has not fully converged. Because internal compatibility is not required, prefer deleting/resetting the obsolete stored message form rather than keeping the fallback.

### 3.2 Top-level failure cleanup

Search UI/Rust mappings for expected permission failures that should now be normal interactions:

~~~text
AccessReviewRequired
ConsentRequired
ReviewSource
agentAccessReviewRequired
agentCapabilityAccessDenied
recovery_action = ReviewSource
safe_actions = ContinueWithoutSource
~~~

Do not globally delete AgentFailure variants if they still represent real non-conversation API errors. Instead verify the canonical conversation source-read paths no longer use them as the product interaction mechanism.

Flutter should not show a global danger badge for a normal pending source permission request.

## 4. Generic built-in Expert invariant

Add/retain table-driven tests that make a new Schedule exception difficult to reintroduce.

Required assertions:

- BuiltinExpertKind::ALL contains all built-ins exactly once;
- builtin_setup_declarations covers ALL exactly once;
- registered_experts covers ALL exactly once;
- every installed enabled built-in card routes to the same BuiltinExpertEndpoint type;
- no source availability check is used to decide card existence;
- each declaration’s mandatory_source appears in required_sources;
- source consumer policy has explicit first-party coverage for declared sources;
- third-party package identity is not part of the default first-party permission set.

Avoid manually duplicated “expected 8 ids” arrays in many tests. One authoritative declaration plus derived checks is preferred.

## 5. Interaction invariant

Add tests proving one lifecycle:

- SourceReadOutcome::NeedsUserAction creates/replays one ConversationInteraction.
- Tool/Task carries only a reference.
- AgentMessage::Interaction carries the same id.
- Flutter queries/resolves that id.
- resolution invokes Access/Connections current owner path.
- linked follow-up Run references the resolved interaction.
- no second “permission request” persistence table exists under Experts or Flutter.

A permission request must not be represented simultaneously as:
- top-level Run failure;
- Expert Registry pending flag;
- Access review row;
- Conversation interaction;
unless those records have distinct owner meanings and exact linkage. Delete duplicate representations that only mirror state.

## 6. Documentation convergence

Implementation changes ownership/runtime and a durable product decision. Update current docs in the same final change set.

### 6.1 docs/architecture/runtime.md

Replace current language that says the host store is cloned into separate “root, built-in Expert and Schedule compositions”.

Document:
- one BuiltinExpertEndpoint for all built-ins;
- provider-neutral source acquisition through Context/Access;
- recoverable source blockers becoming Tool/Task observations;
- Conversation-owned interaction record and linked follow-up Run;
- interaction resume distinct from budget continuation.

Remove Schedule as a special runtime path.

### 6.2 docs/architecture/modules.md

Document:
- Experts owns Expert identity/package/Task/judgment, not source permission;
- Access owns Observe consumer authority;
- Connections owns connection/resource selection;
- Context owns provider-neutral source Views;
- Conversation owns assistant interaction lifecycle;
- Flutter only presents/requests decisions.

Ensure dependency direction matches manifests and tools/architecture/module-dependencies.json.

### 6.3 docs/architecture/authority-recovery.md

Add/clarify:
- user approval is intent, not read authorization;
- interaction resolution must re-read current source/connection/grant authority;
- linked follow-up reexecutes and reauthorizes;
- pending interaction holds no provider/executor transaction;
- restart/duplicate decision and lost-response behavior;
- source identity change supersedes stale interaction;
- Observe/Act separation.

### 6.4 docs/architecture/invariants.md

Only add a new invariant if it is repository-wide and not already covered by one-owner/one-path rules.

A useful concise addition may be:
- “user interaction is not authorization”: presentation/approval records never substitute for the owning authority’s fresh admission.

Do not copy checkpoint status into invariants.md.

### 6.5 docs/product/intelligence.md

Clarify:
- all eight built-in Experts share the same host/runtime model;
- Experts declare required sources but do not own grants;
- a blocked source yields a bounded no-conclusion/user-action result rather than an invented answer;
- Manager remains sole user-facing synthesizer.

### 6.6 docs/product/integrations-and-privacy.md

Change the current product rule that a successful read connection does not imply AI-use permission.

New precise rule:

- connecting a supported first-party source with Use with Floe enabled creates bounded Observe authority for Floe’s approved first-party consumers over the selected resources;
- turning Use with Floe off pauses Observe without disconnecting;
- this never grants Act authority;
- this never grants arbitrary third-party Expert access;
- this never grants arbitrary external model recipient approval;
- sensitive/external processing remains independently fenced.

### 6.7 docs/product/experience.md

Document inline visual escalation:

~~~text
request
 -> Manager/Expert source need
 -> normal Manager limitation response
 -> inline permission/recovery card
 -> user decision
 -> linked follow-up response
~~~

Keep voice/text as surfaces over the same interaction record.

### 6.8 ADR 0028

At implementation time:

- if ADR 0028 is still proposed, edit it before acceptance to the final connection/Observe policy;
- if it has become accepted, supersede/amend it with a new ADR rather than rewriting accepted history.

Required decision delta:
- pair authenticates server relationship;
- concrete first-party source connection establishes default Observe permission;
- resource selection + Observe scope are one product interaction;
- Use with Floe controls Observe;
- third-party/Act/external-model consent remain separate.

### 6.9 New ADR for conversation interaction lifecycle

Add an ADR for the durable runtime decision if no accepted ADR already owns it.

It should record **why**:
- Run/Task are not held open for human latency;
- Conversation owns assistant-triggered interaction lifecycle;
- underlying mutations remain with their semantic owners;
- resolved interaction starts a linked fresh Run rather than replaying a stale read;
- UI is presentation, not authority.

Do not make the ADR an implementation checklist.

### 6.10 This execution plan

After all implementation/docs verification is committed and no checkpoint remains:
- remove docs/development/plans/expert-access-interaction/ from the active tree in a final cleanup commit, or
- leave it only until the implementation PR/branch is merged if the team needs an active checklist.

Git history is the archive. Do not keep completed checkpoint/status prose as permanent current documentation.

## 7. Repository-wide residual search matrix

Run exact and conceptual searches.

### Schedule infrastructure

~~~text
ScheduleEndpoint
CalendarExpertEndpoint
schedule_definition
SCHEDULE_DEFINITION_REVISION
schedule_packaging
CALENDAR_EXPERT_SETTLEMENT_OWNER
select_active_setup
CalendarHistoryBoundary
floe.builtin.schedule/v1
~~~

Expected: zero production infrastructure matches.

### Calendar Expert vertical

~~~text
CalendarExpertSetup
CalendarExpertOverview
CalendarAccessConfiguration
CalendarAccessChange
calendar_experts
experts.calendar.
install_calendar_expert
apply_calendar_access
~~~

Expected: zero production matches.

### Duplicated source authority

~~~text
SourceGrants
BuiltinSourceBinding
BuiltinSourceState
BuiltinSourceEvidence
assignment_source_grant
assignment_has_mandatory_source
calendar_grant_mappings
~~~

Expected: zero matches for the obsolete permission model.

### UI duplication

~~~text
AgentCalendarExpertController
AgentCalendarSettings
agent_calendar_expert
Allow external model providers
reviewRemoteCalendarGrant
pauseRemoteCalendarGrant
~~~

Expected: zero production product-path matches.

### Interaction

~~~text
NeedsUserAction
UserInteractionRef
ConversationInteraction
AgentMessage::Interaction
ResumeInteraction
Use with Floe
~~~

Expected: matches only in canonical contract/owner/presentation/tests/docs.

### Authority leakage / secrets

Search serialized interaction/source requirement structs for:

~~~text
token
bearer
credential
secret
password
base_url
private_key
oauth_state
~~~

No interaction payload may contain secret-shaped authority.

## 8. Dependency and public-surface audit

Inspect:
- Cargo manifests;
- tools/architecture/module-dependencies.json;
- public Rust exports;
- App service enums;
- protocol command/query enums;
- FFI headers;
- Dart gateways.

Required final properties:

- Conversation business module does not depend on built-in Expert package.
- Experts does not depend on provider/native adapters.
- Access does not depend on Flutter/App or Expert Registry implementation.
- Context does not own credentials.
- App composes owners but does not duplicate their authority.
- no public symbol exists solely to preserve removed Calendar Expert callers.
- no optional field exists solely for old/new compatibility.
- no v2/legacy/compat route was added for the internal cutover.

If a new dependency edge is required for the final architecture, update module-dependencies.json deliberately and document the owner reason.

## 9. Full verification

Run focused tests first, then the full relevant gates.

### 9.1 Rust/architecture

~~~sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
git diff --check
~~~

Record actual failures and distinguish pre-existing unrelated issues. Do not weaken tests.

### 9.2 FFI/protocol

~~~sh
cargo build -p floe-ffi
~~~

Also run:
- protocol crate tests;
- FFI C ABI tests;
- app-wire fixtures/tests;
- generated binding consistency command if current repository tooling requires one.

Inspect the final wire enum manually to confirm experts.calendar.* is absent and interaction/connection commands have one representation.

### 9.3 Flutter/macOS

From apps/client:

~~~sh
flutter analyze
flutter test
flutter build macos
~~~

Run focused goldens for:
- connection detail Use with Floe;
- chat permission interaction;
- generic Expert registry if still visible.

If native/EventKit files changed, run the repository’s macOS Swift/native tests/scripts and verify bundle/sign/load as required by the current Apple runbook.

Do not claim iOS validation unless it was actually run. Apple is the priority, but macOS stabilization may be the practical gate for this change.

### 9.4 Go server

If checkpoint 04 changes Go pairing/source connection/grant behavior, from server/ run:

~~~sh
go test -race ./...
go vet ./...
~~~

Run current credential/authority tests from server/README.md when applicable.

### 9.5 Persistence/authority

Use fresh isolated profiles and verify:

- fresh Vault creation;
- restart/reopen;
- grant active/pause/revoke;
- source authority change;
- interaction pending/restart;
- duplicate decision;
- linked resume accepted then response lost;
- no duplicate external action;
- no secret leakage in errors/debug/trace.

## 10. End-to-end acceptance matrix

### A. Happy Calendar use

1. fresh profile;
2. connect macOS Calendar;
3. grant EventKit permission;
4. select Personal + Work;
5. Use with Floe displays Active;
6. ask “What is on my calendar today?”;
7. Manager delegates through common Schedule Expert;
8. generic Calendar reader authorizes/read;
9. Manager answers;
10. source coverage recorded.

### B. Observe paused

1. turn Use with Floe Off;
2. ask Calendar question;
3. Schedule still exists in Manager catalog;
4. Calendar read produces NeedsUserAction;
5. Schedule Task is not an infrastructure failure;
6. Manager says Calendar access is needed;
7. inline card appears;
8. Allow;
9. current source/grant revalidated;
10. linked follow-up Run;
11. Calendar answer returned.

### C. Deny

1. repeat paused flow;
2. choose Not now;
3. interaction Denied;
4. no grant mutation;
5. no automatic linked Run;
6. original Manager limitation remains valid.

### D. OS permission revoked

1. Use with Floe previously Active;
2. revoke Calendar permission in macOS;
3. ask question;
4. source check identifies system permission requirement;
5. UI does not say Active/usable based on stale Registry;
6. interaction opens native/system recovery;
7. after permission restore, backend refreshes and verifies;
8. linked Run reads fresh Calendar.

### E. Source identity changed

1. create active grant;
2. change source identity/authority/fingerprint;
3. old grant does not admit;
4. interaction says review/repair, not simple enable;
5. stale Allow cannot reactivate without fresh review;
6. after review, linked Run succeeds.

### F. Remote source

1. pair server;
2. pair alone creates no arbitrary source account access;
3. connect concrete SaaS account;
4. default first-party Observe becomes Active;
5. Use with Floe pause/resume works;
6. external model recipient remains separately fenced.

### G. Expert uniformity

For every BuiltinExpertKind:
- common setup declaration;
- common Directory registration;
- common BuiltinExpertEndpoint;
- common TaskCoordinator lifecycle;
- no App endpoint selected by agent id.

### H. Restart

1. create pending interaction;
2. quit/restart;
3. interaction still pending and inspectable;
4. resolve;
5. kill after resume command admission but before client receives response;
6. restart;
7. same command/run recovered;
8. no duplicate source mutation or action.

## 11. Final acceptance checklist

The implementation is done only when all boxes are true:

- [ ] Schedule special App endpoint deleted.
- [ ] Schedule is in common built-in setup and dispatch.
- [ ] request-scoped Calendar read preserved.
- [ ] native EventKit read goes through generic Context/Access path.
- [ ] generic Expert settlement handles Schedule.
- [ ] Conversation has no Schedule-specific history boundary.
- [ ] Registry source permission authority deleted.
- [ ] CalendarExpertSetup vertical deleted.
- [ ] Calendar grant identity no longer references Registry setup/assignment/installation.
- [ ] Access/DataAccessGrant is sole Observe authority.
- [ ] enabled Expert remains discoverable while source is off.
- [ ] connector connection creates default first-party Observe authority.
- [ ] Use with Floe is the connection-level Observe control.
- [ ] Settings external-model/source toggle removed.
- [ ] Act authority remains separate.
- [ ] external model recipient consent remains separate.
- [ ] source blocker creates typed Conversation interaction.
- [ ] Manager returns natural response instead of global error-only UX.
- [ ] Flutter renders generic inline interaction.
- [ ] interaction resolution revalidates current owner authority.
- [ ] linked follow-up Run is distinct from budget continuation.
- [ ] duplicate/restart interaction behavior verified.
- [ ] old experts.calendar.* wire removed.
- [ ] old Flutter Calendar Expert files removed.
- [ ] architecture/product/ADR docs updated.
- [ ] residual searches clean.
- [ ] Rust/architecture gate passes.
- [ ] FFI/protocol gate passes.
- [ ] Flutter analyze/test/macos build passes.
- [ ] relevant native/macOS validation passes or exact unavailable prerequisites are reported.
- [ ] Go server gate passes if server code changed.

## 12. Final agent report

Use this exact structure at completion:

1. **Final owner/path**
   - one paragraph naming Experts, Context, Access, Connections, Conversation and Flutter ownership.

2. **Major deleted surface**
   - ScheduleEndpoint;
   - Calendar Expert setup/access vertical;
   - Registry source permission;
   - Calendar grant mapping coupling;
   - experts.calendar.* wire;
   - Flutter Calendar Expert settings;
   - Settings LLM/source toggle;
   - Schedule history boundary.

3. **Interaction behavior**
   - blocked source -> Manager response -> interaction -> owner resolution -> linked fresh Run.

4. **Residual audit**
   - commands/searches run and remaining justified matches.

5. **Verification**
   - exact Rust/FFI/Flutter/native/Go commands and results.

6. **Documentation**
   - architecture docs updated;
   - ADR 0028 disposition;
   - new interaction ADR if added;
   - product docs updated.

7. **Unverified surface/blocker**
   - state only concrete unavailable validation, not confidence language.

The final report must not call the architecture complete while an old production path or authority remains.
