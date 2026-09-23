# Checkpoint 02 — Calendar source extraction and Schedule Expert convergence

## Goal

Remove Schedule’s special App endpoint and make it run through the same built-in Expert setup, Directory registration, BuiltinExpertEndpoint, BuiltinExpertHost and Task path as the other seven built-in Experts.

This checkpoint is intentionally large because Schedule’s special endpoint currently owns real Calendar functionality. The correct migration is **extract functionality to its semantic owners first, then delete the endpoint**. Do not replace ScheduleEndpoint with a forwarding wrapper.

By checkpoint exit:

- Schedule is registered in the common built-in dispatch table;
- native and remote Calendar reads are available through a provider-neutral Context host path;
- Schedule’s domain reasoning lives under crates/experts/builtin/src/schedule like every other Expert;
- the App ScheduleEndpoint subtree is deleted;
- the generic Task path can carry Schedule settlement/proposal output;
- the old CalendarExpertSetup persistence may still exist temporarily, but Schedule execution must no longer depend on a Schedule-specific endpoint.

## Baseline anchors

- crates/experts/builtin/src/catalog.rs:62 — BuiltinExpertKind::ALL
- crates/experts/builtin/src/catalog.rs:73 — BuiltinExpertKind::BUILTIN_SETUP excludes Schedule
- crates/experts/builtin/src/schedule/mod.rs:9 — exports old definition/history/host/plan
- crates/experts/builtin/src/schedule/host.rs:38 — private ExpertHost
- crates/experts/builtin/src/schedule/host.rs:68 — invoke_inner
- crates/experts/builtin/src/schedule/host.rs:115 — run_schedule_reasoning
- crates/experts/builtin/src/schedule/host.rs:384 — Answer requires active_view
- crates/experts/builtin/src/schedule/host.rs:395 — Call path
- crates/app/src/vault_host/conversation_turn/expert_dispatch.rs:23 — special schedule module
- crates/app/src/vault_host/conversation_turn/expert_dispatch.rs:29 — registered_experts excludes Schedule
- crates/app/src/vault_host/conversation_turn/expert_dispatch.rs:84 — BuiltinExpertEndpoint
- crates/app/src/vault_host/conversation_turn/expert_dispatch.rs:407 — common calendar_views hook
- crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule.rs:56 — ScheduleEndpoint
- crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule.rs:79 — AgentEndpoint impl
- crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule.rs:91 — select_active_setup
- crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule/agent.rs:48 — CalendarExpertEndpointResult
- crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule/agent.rs:54 — schedule settlement owner
- crates/app/src/vault_host.rs:195 — OpenVault::activate
- crates/app/src/vault_host.rs:257 — sync_expert_directory

## 1. Move Calendar acquisition to Context-owned source reading

### 1.1 What must leave ScheduleEndpoint

Inventory the code in crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule.rs and schedule/agent.rs and classify every symbol before moving anything.

Expected categories:

1. **source identity / connection resolution** -> Connections/provider adapters;
2. **DataAccessGrant admission** -> Access;
3. **native subject/fingerprint check** -> native/provider adapter + Access fence;
4. **Calendar observation acquisition** -> provider adapter;
5. **bounded timeline/View projection** -> Context;
6. **dependency/provenance record** -> Context/Conversation recorder;
7. **query range chosen from assignment** -> Schedule domain;
8. **Schedule reasoning** -> Schedule Expert;
9. **Task settlement / private-state CAS** -> generic Experts/Vault Task path;
10. **action proposal payload** -> Schedule result/Actions bridge.

Nothing in categories 1–6 may remain in a Schedule-specific App endpoint after the checkpoint.

Inventory from the current implementation:

| Current symbol(s) | Destination/ownership |
|---|---|
| `SelectedSetup`, `select_active_setup`, `validate_active_connection` | Connection selection and source identity; remove Registry setup authority in checkpoint 03 |
| `BoundAccess`, `GrantBoundCalendarAccess` | Access admission/revalidation composed by Context, with no Expert setup key |
| `Access`, `DeviceCalendarAccess`, `NativeCalendarReadAccess`, native batch/schedule conversion | Real native/provider adapter under Context Calendar acquisition |
| `VaultRemoteCalendarBackend`, `RemoteCalendarAccess` | Remote provider transport and Access-signed admission |
| `FixtureAccess` | Context Calendar reader test fixture only |
| `ExpertTimelineViews`, `CalendarTimelineViews` | Context bounded projection, dependency and lease lifecycle |
| `CalendarExpertEndpointRequest`, `run_calendar_expert_endpoint` | Split into generic host input, Schedule judgment and Experts Task lifecycle |
| `CalendarExpertEndpointResult`, `CalendarExpertSettlement`, `CALENDAR_EXPERT_SETTLEMENT_OWNER` | Generic `BuiltinExpertOutput` / `ExpertSettlement` and Task settlement |
| `NoCapabilityJournal`, `calendar_invocation`, `schedule_expert` | Remove with the old Registry-scoped execution path |

### 1.2 Canonical Calendar reader

Build one Context-owned Calendar reader used by any consumer, not only Schedule.

It must compose the existing abstractions rather than introducing a second Calendar subsystem:

- floe_context::CalendarSource
- floe_context Calendar timeline/view projection
- floe_access::CalendarReadAdmission / CalendarReadAccessRequest
- native provider adapters such as NativeCalendarReadAccess / EventKit acquisition
- remote ServerSourceClient / remote grant authorization
- Calendar mirror fallback only where the existing Context contract explicitly supports it

The reader input must be semantic and request-scoped:

~~~text
person
device
consumer
purpose
requested range
resource/connection selection resolved from current source state
deadline/cancellation
~~~

The Expert must not provide:
- connection credentials;
- bearer token;
- server base URL;
- grant id chosen by the model;
- source authority chosen by the model;
- native fingerprint chosen by the model.

The reader resolves authoritative current connection/grant state behind its owner boundaries.

Implementation note: Context now has a signed remote Calendar View read and reauthorization operation that binds an exact resource, consumer, query, grant authority and source preview. The common App host injects that read for selected remote resources and records the returned dependency; native Calendar acquisition and Schedule endpoint deletion remain in this checkpoint.

### 1.3 Native + remote parity at the semantic boundary

Current common PersonalViewSource.calendar_views() only reads through ServerSourceClient and returns an empty list when no remote source client exists. That behavior is insufficient.

Replace it with a host-injected Calendar reader that can serve:

- EventKit on the current Apple device;
- fixture/test Calendar;
- supported remote Google/Microsoft Calendar;
- Android only to the extent required for shared compilation, not new product parity.

The BuiltinExpertHost method can remain calendar_views() if it is genuinely provider-neutral, but its implementation must no longer infer “no remote source client == no calendars”.

A better concrete host field is conceptually:

~~~text
calendar_reader: Option<&dyn CalendarContextReaderApi>
~~~

and DelegatedMessageExperts::calendar_views() forwards the request to that reader under the requesting Expert’s consumer identity.

Do not inject provider-specific readers into the Expert crate.

### 1.4 Preserve exact range behavior

Schedule’s current request-scoped range selection is a product invariant from ADR 0023.

The generic Calendar reader must accept a bounded requested interval. Do not regress to the current generic calendar_views() fixed “now ± 1 day” window.

Change BuiltinExpertHost Calendar acquisition contract to include a query/range object rather than a no-argument list call.

For example:

~~~text
CalendarViewQuery
  range_start_unix_ms
  range_end_unix_ms
  cursor?
  max_items
  max_bytes
~~~

The host enforces absolute maximum duration/items/bytes. The Expert selects the requested interval from the assignment.

Tests must preserve:
- today -> today coverage;
- this week -> a fresh wider read;
- explicit past/future range;
- partial/paginated coverage remains distinct from empty;
- stale coverage is not reused for a wider request.

## 2. Rebuild Schedule as a normal built-in dispatch

### 2.1 File structure

Converge crates/experts/builtin/src/schedule toward the same shape as the other Expert folders:

~~~text
schedule/
  mod.rs
  dispatch.rs
  expert.rs
~~~

Optional small domain helpers are allowed when they contain real Schedule semantics.

Delete by checkpoint end:

- schedule/definition.rs
- schedule/calendar_history.rs
- the old infrastructure-heavy schedule/host.rs

schedule/plan.rs may remain only if it is pure Schedule-domain parsing/planning and has no App/provider/Access/Registry dependencies. Prefer folding small logic into dispatch.rs or expert.rs if the separate file only preserves the old endpoint shape.

### 2.2 dispatch()

Add Schedule to crates/app/src/vault_host/conversation_turn/expert_dispatch.rs:registered_experts().

The dispatch implementation should follow the same pattern as Commitments/Focus/Wellbeing:

1. inspect the source requirement through the generic host;
2. resolve the requested Calendar interval from the assignment;
3. read the Calendar source through the provider-neutral host port;
4. record exact dependencies;
5. run Schedule judgment;
6. return BuiltinExpertOutput with Schedule result and optional proposal/interaction artifacts.

Do not add a special branch in BuiltinExpertEndpoint for Schedule.

### 2.3 Iterative reasoning

Schedule currently has a multi-step tool-capable reasoning loop while other built-ins usually perform one bounded model call.

It is acceptable for Schedule to keep a richer internal reasoning strategy **inside the Schedule Expert package**, provided:

- it uses the shared ExpertModel/ExpertReasoner contract;
- capability execution is supplied by BuiltinExpertHost;
- failed source reads become ExpertCapabilityObservation from checkpoint 01;
- App does not contain Schedule-specific orchestration;
- Task/usage/deadline/cancellation are the common path.

If the existing iterative loop is still needed for range selection/search/free-window operations, move only that domain loop into schedule/expert.rs. Its tool execution callback must call generic Calendar Context acquisition.

A Schedule-specific internal algorithm is valid. A Schedule-specific App endpoint is not.

### 2.4 Answer without a successful Calendar view

Remove the old invariant at schedule/host.rs around the baseline Answer branch that requires active_view to exist before Schedule can answer.

The new behavior is:

- mandatory source Ready -> normal Schedule result;
- mandatory source NeedsUserAction -> Schedule returns a Manager-ready bounded result explaining that schedule evidence was not available and includes the interaction artifact;
- mandatory source Unavailable -> Schedule returns a degraded/no-conclusion result when the domain schema supports it, or a typed non-hard blocked result;
- hard integrity/storage/cancellation failure -> Err(AgentFailure).

Do not fabricate an empty Calendar view to make the result schema pass.

The Schedule result schema should explicitly represent “no conclusion because source was unavailable/user action required” if its existing no-conclusion fields are insufficient.

## 3. Put Schedule into the common built-in setup

### 3.1 Remove ALL vs BUILTIN_SETUP divergence

At crates/experts/builtin/src/catalog.rs:62–80:

- make the installation declaration list contain all eight built-in Experts;
- delete BUILTIN_SETUP if it exists only to mean “all except Schedule”;
- use one canonical iteration list for built-in setup and Directory sync.

If another semantic subset is genuinely needed later, name it for that semantic reason. Do not retain BUILTIN_SETUP as a compatibility alias.

builtin_setup_declarations() at baseline line 41 must emit Schedule.

### 3.2 Common packaging

At crates/app/src/vault_host.rs:2521 and :2570:

- builtin_setup_specs() must package Schedule through expert_setup_spec() / expert_packaging();
- delete schedule_packaging();
- delete any separate schedule_definition path;
- the Directory definition for Schedule must be generated from the same AgentCard / contract_definition path as every other built-in.

### 3.3 Vault open / Directory

At OpenVault::activate baseline lines 195–255:

delete:
- creation of ScheduleEndpoint;
- direct schedule_definition();
- unconditional directory.register() for Schedule;
- Schedule-specific settlement owner passed only because of that endpoint.

OpenVault should construct:
- one BuiltinExpertEndpoint;
- one Directory;
- one TaskCoordinator / VaultTaskRepository that can settle generic Expert tasks;
- then sync enabled Expert cards through the same sync_expert_directory() path.

At sync_expert_directory baseline lines 257–282:
- iterate all built-in kinds;
- unregister/register all through one rule;
- do not special-case Schedule;
- card inclusion must eventually stop depending on source grant availability in checkpoint 03. During checkpoint 02, if the old Registry source gating still exists, tests may use a prepared Calendar source so Schedule can register; this temporary dependency must be removed next.

## 4. Generic Schedule settlement and proposal flow

### 4.1 Remove CalendarExpertSettlement naming

The common settlement contract already exists as ExpertSettlement.

Move Schedule private-state/task settlement generation into generic Expert output/Task settlement introduced in checkpoint 01.

Replace names such as:

- CalendarExpertSettlement
- CalendarExpertEndpointResult
- CALENDAR_EXPERT_SETTLEMENT_OWNER

with generic Expert settlement semantics.

### 4.2 VaultTaskRepository

At crates/adapters/vault/src/repositories/task.rs baseline lines 13, 72 and 101:

the repository currently receives one settlement_owner string and dispatches to settle_calendar_expert_task_checked().

Refactor so settlement validation is based on the Task’s selected Expert/assignment and the generic ExpertSettlement owner recorded by Experts, not a repository-wide hard-coded Schedule owner.

Required checks:

- Task id matches completion;
- principal matches;
- invocation/assignment identity matches;
- expected Registry revision/authority matches;
- result digest/result value matches the settlement;
- duplicate settlement is idempotent only under the exact same identity;
- stale/foreign settlement conflicts;
- no other Expert may settle Schedule state and vice versa.

Rename Vault method settle_calendar_expert_task_checked() to a generic settle_expert_task_checked() only when its body contains no Calendar-specific assumptions.

### 4.3 Action proposals

Schedule action proposals are domain output, not reason to keep a private endpoint.

Carry proposal references as typed artifacts through the generic BuiltinExpertOutput -> A2ATask -> ExpertReport -> Task path.

Existing AgentProposalCard behavior may continue to inspect the proposal through the current Actions owner until checkpoint 05. Do not move action authority into the new interaction contract.

Observe permission interaction and Act approval remain separate.

## 5. Remove Schedule-specific Conversation history knowledge

The root Conversation currently passes schedule::CalendarHistoryBoundary from crates/app/src/vault_host/conversation_turn.rs baseline line 101.

Delete that dependency.

### 5.1 Immediate replacement

Use recorded DependencyCoverage / ContextDependency identity as the primary history-authority mechanism.

The repository already has project_model_conversation_history() using recorded coverage. The temporary SourceHistoryBoundary exists because some session messages cannot prove provenance.

During this checkpoint:

- ensure all successful generic Schedule source reads record their dependencies on the capability/task/message identity that Conversation later projects;
- ensure Schedule Manager answers inherit the Task/source coverage through the existing coverage fold;
- add tests that revoking the Calendar grant removes old Schedule-derived history even without checking the agent id string.

### 5.2 SourceHistoryBoundary

If the generic source-history fallback is still required for old message forms during the same checkpoint, replace CalendarHistoryBoundary with an owner-neutral rule based on “message has source-dependent recorded coverage”.

Do not create a BuiltinExpertHistoryBoundary that maps every Expert id to source names. That only generalizes the wrong abstraction.

The target for checkpoint 06 is to delete SourceHistoryBoundary entirely if all canonical messages have recorded coverage sufficient for projection.

## 6. Deletion gate

Before marking checkpoint 02 complete, delete these production files if no unrelated pure-domain code remains:

~~~text
crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule.rs
crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule/agent.rs
crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule/agent/tests.rs
crates/experts/builtin/src/schedule/definition.rs
crates/experts/builtin/src/schedule/calendar_history.rs
~~~

The old schedule/host.rs must also be deleted or reduced to a pure domain implementation renamed/moved to expert.rs. It must not retain source/provider/App orchestration.

Remove production symbols:

~~~text
ScheduleEndpoint
schedule_definition
SCHEDULE_DEFINITION_REVISION
CALENDAR_EXPERT_SETTLEMENT_OWNER
CalendarExpertEndpointRequest
CalendarExpertEndpointResult
run_calendar_expert_endpoint
select_active_setup
schedule_packaging
CalendarHistoryBoundary
~~~

Tests named calendar_experts.rs may temporarily remain if they now exercise generic Schedule + Calendar integration. Prefer renaming/splitting them before final checkpoint so test names describe the new architecture.

## 7. Tests

### Built-in registration

Add a table-driven test over BuiltinExpertKind::ALL proving every declaration:

- appears exactly once in built-in setup declarations;
- packages through the same ExpertSetupSpec path;
- can be registered through registered_experts();
- has no dedicated App endpoint requirement.

### Schedule generic-path E2E

Drive a Manager turn:

~~~text
user -> Manager -> Schedule delegation -> generic BuiltinExpertEndpoint
     -> generic Calendar reader -> Context/Access -> fixture/native mock
     -> Schedule result -> Task -> Manager final answer
~~~

Assert:
- Task is Completed;
- Manager Run is Completed;
- result coverage contains Calendar dependency;
- no ScheduleEndpoint symbol/path is used.

### Source failure

For Calendar source NeedsUserAction:
- Schedule Task returns the non-hard blocked/degraded output defined in checkpoint 01;
- Manager receives the Task result and performs another model iteration;
- Manager final answer states the limitation;
- root Run is not failed solely because Calendar permission is absent.

For hard Vault/integrity failure:
- failure remains hard;
- do not convert it into a permission interaction.

### Range and coverage

Port the strongest tests from schedule/agent/tests.rs:
- request range validation;
- stale view rejected;
- authorization rechecked after acquisition;
- subject/fingerprint change between checks rejected;
- failed read does not pin stale authority;
- pagination/coverage bounds;
- dependency exactness.

The tests should now target Context Calendar acquisition and Schedule dispatch separately rather than one giant ScheduleEndpoint fixture.

## 8. Residual searches

Run concept searches, not only exact file deletion:

~~~text
ScheduleEndpoint
schedule_definition
schedule_packaging
CALENDAR_EXPERT_SETTLEMENT_OWNER
CalendarExpertEndpoint
select_active_setup
CalendarHistoryBoundary
BuiltinExpertKind::BUILTIN_SETUP
pub(in crate::vault_host) mod schedule
floe.builtin.schedule/v1
~~~

Allowed remaining “schedule” matches are domain names, prompt/result identifiers, Action proposal semantics and generic test fixtures. Infrastructure ownership must not be Schedule-specific.

Also inspect dependency manifests. Conversation/business modules must not gain a dependency on floe-experts-builtin merely to replace CalendarHistoryBoundary.

## 9. Verification

Targeted:
- floe-experts-builtin tests;
- floe-experts tests;
- Context Calendar source/view tests;
- Vault Task settlement tests;
- App Schedule/Calendar conversation tests;
- runtime delegation tests.

Broad gate:
- cargo check --workspace --lib
- cargo test --workspace --no-fail-fast
- python3 tools/architecture/check_boundaries.py
- git diff --check

Because this checkpoint removes App/FFI-visible Schedule infrastructure indirectly, build floe-ffi even if the explicit Calendar Expert wire is removed in checkpoint 03.

## 10. Checkpoint exit criteria

Checkpoint 02 is complete only when:

- Schedule is the eighth ordinary built-in Expert in common setup and dispatch;
- no Schedule-specific App AgentEndpoint remains;
- Calendar source acquisition is provider-neutral and can read the Apple native source through Context/Access;
- Schedule request-scoped ranges are preserved;
- expected Calendar access absence does not automatically hard-fail the root conversation;
- generic Expert settlement can settle Schedule state/proposals;
- Conversation no longer imports Schedule’s CalendarHistoryBoundary;
- old Schedule infrastructure symbols are absent by residual search;
- all targeted and broad Rust/architecture checks pass.

Checkpoint 03 may now remove the CalendarExpertSetup/Registry permission authority because Schedule execution no longer needs that special vertical.
