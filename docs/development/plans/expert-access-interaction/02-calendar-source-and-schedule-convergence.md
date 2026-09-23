# Checkpoint 02 — Schedule common-runtime cutover from current main

- **Status:** complete
- **Completion baseline:** `main` at `605250d0b8f3bec476299673a8975fa82a073c52`
- **Scope closed:** R1–R7 plus the R4.5 Calendar first-party consumer-policy prerequisite are complete.
- **Next checkpoint:** `03-expert-registry-and-access-authority.md`

This file records the completed Checkpoint 02 cutover. Schedule is now the eighth ordinary built-in Expert, common Calendar reads use actual consumer identities, and the old Schedule production endpoint/history boundary are gone.

---

## 0. Frozen foundation

Treat the following as complete unless a focused Schedule common-path test demonstrates a concrete missing semantic:

1. request-scoped CalendarViewQuery;
2. exact query and coverage validation;
3. bounded pagination support;
4. signed remote Calendar Context acquisition;
5. native Calendar admission against current connection and DataAccessGrant;
6. native EventKit acquisition through Context;
7. common BuiltinExpertHost Calendar reads;
8. SourceReadOutcome with Ready, Unavailable and NeedsUserAction;
9. SourceAccessRequirement bound to current connection/resource/source authority when representable;
10. generic Expert auxiliary artifact and settlement carriers;
11. generic Vault Task settlement validation against the selected Expert/package.

Relevant landed commits:

~~~text
95993ad82b  Make built-in Calendar reads request-scoped
39a3e94b14  Select bounded Schedule calendar ranges from assignments
675d60821b  Preserve paginated Calendar coverage in Context reads
bc0c903d29  Validate Calendar pages against exact Context query
e2f6760ce6  Add grant-bound remote Calendar Context reader
25697999f6  Route common Expert Calendar reads through Context admission
ae2a827f0e  Admit native Calendar reads by current connection and grant
0ff0053933  Route native Calendar views through Context and common Expert host
be22dc59b0  Bind Expert Task settlement to selected package identity
91b4b4258d  Preserve typed Calendar access outcomes for built-in Experts
75e385ca00  Bind Calendar review outcomes to current source identity
8b0f4533c3  Refactor Schedule request planning for common Calendar reads
5633b81505  Run Schedule judgment through BuiltinExpertHost
f8ab18c0d2  Settle stateful Schedule results on common Expert path
076cdf3ad9  Cut Schedule over to the common Directory endpoint (reverted)
98e9d8ce3b  Revert Schedule cutover pending Calendar grant identity
f1e5010b8f  Authorize Calendar reads for canonical first-party consumers
ccb72e691b  Cut Schedule over to the common Directory endpoint
605250d0b8  Delete old Schedule endpoint and history boundary
~~~

### Completion evidence

- **R1–R4:** request-only planning, `schedule::dispatch`, common judgment, stateful settlement and ContextDependency-bound `/focus` evidence remain on the common path.
- **R4.5:** native and remote fresh Calendar grants use the catalogue-derived canonical first-party consumer set; successful Schedule dependencies record `floe.builtin.schedule`, while legacy `calendar.expert` scope is rejection-only fixture state.
- **R5:** `BuiltinExpertKind::ALL`, common setup and `registered_experts()` include Schedule; Directory routes Schedule through `BuiltinExpertEndpoint`.
- **R6:** `ScheduleEndpoint`, `run_calendar_expert_endpoint`, direct registration, `schedule_definition` and their dedicated tests/helpers are deleted.
- **R7:** Conversation owns `ConservativeSourceHistoryBoundary`; `CalendarHistoryBoundary` and the Schedule-specific history module are deleted.
- **Final package:** `schedule/{mod.rs,dispatch.rs,expert.rs,plan.rs}` only.
- **Verification reported at completion:** workspace check/tests, architecture boundary check, FFI build, `git diff --check`, Flutter analyze/tests/macOS build, and focused Context/Access/provider/Vault residual tests all passed.

Current production split:

~~~text
Schedule
  -> ScheduleEndpoint
  -> select_active_setup
  -> CalendarExpertSetup / CalendarViewBinding
  -> run_calendar_expert_endpoint
  -> schedule::ExpertHost

Other built-ins
  -> BuiltinExpertEndpoint
  -> registered_experts
  -> BuiltinExpertHost
  -> domain dispatch
~~~

Do not add another Calendar abstraction while both paths remain alive.

### Frozen-area changes are allowed only when

1. a new common Schedule-path test exists;
2. that test fails;
3. the failure cannot be expressed by the current Context/Access contract;
4. the fix is the smallest owner-correct change.

Do not proactively add new reader layers, permission reason variants, source metadata, or Checkpoint 03 authority work.

---

# 1. Final topology

At checkpoint exit:

~~~text
BuiltinExpertKind::ALL
  -> common built-in setup
  -> enabled Expert cards
  -> Directory
  -> one BuiltinExpertEndpoint
  -> registered_experts
       -> schedule::dispatch
       -> commitments::dispatch
       -> communication::dispatch
       -> relationships::dispatch
       -> focus_attention::dispatch
       -> wellbeing::dispatch
       -> work_context::dispatch
       -> life_logistics::dispatch
~~~

Schedule reads Calendar only through:

~~~text
schedule::dispatch
  -> BuiltinExpertHost::calendar_views
  -> PersonalViewSource
  -> CurrentCalendarContextReader
  -> Context / Access
  -> native or remote adapter
~~~

There is no production ScheduleEndpoint.

CalendarExpertSetup, SourceGrants and calendar_grant_mappings may remain for Checkpoint 03, but none may be required to execute Schedule.

---

# 2. Current line anchors

Current main 75e385ca:

| File | Line | Symbol |
|---|---:|---|
| crates/experts/builtin/src/catalog.rs | 41 | builtin_setup_declarations |
| crates/experts/builtin/src/catalog.rs | 62 | BuiltinExpertKind::ALL |
| crates/experts/builtin/src/catalog.rs | 73 | BUILTIN_SETUP excludes Schedule |
| crates/experts/builtin/src/host.rs | 116 | BuiltinExpertOutput |
| crates/experts/builtin/src/host.rs | 152 | BuiltinExpertHost |
| crates/experts/builtin/src/host.rs | 194 | calendar_views |
| crates/app/src/vault_host/conversation_turn/expert_dispatch.rs | 24 | special schedule module |
| crates/app/src/vault_host/conversation_turn/expert_dispatch.rs | 30 | registered_experts has seven |
| crates/app/src/vault_host/conversation_turn/expert_dispatch.rs | 425 | common Calendar host |
| crates/app/src/vault_host/conversation_turn/expert_host.rs | 773 | CalendarContextReaderApi |
| crates/app/src/vault_host/conversation_turn/expert_host.rs | 884 | CurrentCalendarContextReader |
| crates/app/src/vault_host.rs | 195 | direct Schedule registration |
| crates/app/src/vault_host.rs | 254 | sync_expert_directory |
| crates/app/src/vault_host.rs | 2519 | builtin_setup_specs |
| crates/app/src/vault_host.rs | 2568 | schedule_packaging |
| crates/experts/builtin/src/schedule/plan.rs | 48 | old setup selection |
| crates/experts/builtin/src/schedule/plan.rs | 110 | plan_run depends on provider/setup facts |
| crates/experts/builtin/src/schedule/plan.rs | 149 | requested_range |
| crates/experts/builtin/src/schedule/host.rs | 38 | old Schedule ExpertHost |
| crates/experts/builtin/src/schedule/host.rs | 305 | iterative reasoning |
| crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule.rs | 54 | ScheduleEndpoint |
| crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule.rs | 272 | App select_active_setup |
| crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule.rs | 329 | BoundAccess |
| crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule/agent.rs | 33 | CalendarExpertEndpointRequest |
| crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule/agent.rs | 48 | CalendarExpertEndpointResult |
| crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule/agent.rs | 58 | run_calendar_expert_endpoint |
| crates/adapters/vault/src/repositories/task.rs | 64 | generic settlement validation |
| crates/app/src/vault_host/conversation_turn.rs | 103 | CalendarHistoryBoundary |

---

# 3. Checkpoint boundaries

## Leave for Checkpoint 03

These may remain after this checkpoint:

~~~text
CalendarExpertSetup
CalendarExpertOverview
CalendarViewBinding
calendar_grant_mappings
SourceGrants
BuiltinSourceBinding
BuiltinSourceState
Calendar access settings wire/UI
~~~

Do not remove them early.

## Remove from Schedule execution now

After cutover:

- App does not select CalendarExpertSetup before Schedule runs.
- App does not build BoundAccess for Schedule.
- App does not call run_calendar_expert_endpoint.
- Schedule appears in the same Directory sync as all other built-ins.
- Schedule model execution uses common ExpertModelHost.
- Schedule source reads use BuiltinExpertHost::calendar_views.

Do not introduce a forwarding endpoint in either direction.

---

# 4. R1 — Separate request planning from source/setup planning

The current schedule plan requires provider, calendar count and remote availability before the Calendar read. That forces the old setup selection.

In crates/experts/builtin/src/schedule/plan.rs:

Delete:

~~~text
ScheduleSetupCandidate
ScheduleSetupSelection
select_active_setup
~~~

Replace plan_run with request-only planning.

Target shape:

~~~text
ScheduleRequestPlan
  range
  starts_at
  ends_at
  propose_focus
~~~

Target API:

~~~text
plan_request(
  assignment,
  local_now,
  now
) -> ScheduleRequestPlan
~~~

The planner may use only assignment/time/range semantics.

It must not use:

- CalendarProvider;
- device id;
- CalendarExpertSetup;
- CalendarViewBinding;
- calendar count;
- remote source availability;
- connection revision;
- grant state.

Keep requested_range.

Remove acquire_remotely and provider-derived pre-read ScheduleReasoning.

### Model placement

Do not re-add provider identity to CalendarContextView to choose model placement.

The common Expert path captures ContextDependency before model execution. Inference/Access owns ProcessingRestriction and exact recipient admission.

Required invariant:

~~~text
LocalOnly dependency -> remote release denied
ApprovedRecipient -> only exact approved recipient
~~~

Schedule does not branch on Google/Microsoft/EventKit to choose inference placement.

### R1 tests

- today;
- this week;
- explicit day;
- explicit bounded range;
- focus lead;
- invalid/too-wide range;
- no provider/setup input required.

R1 is complete when old setup-selection types are gone from the Schedule domain package.

---

# 5. R2 — Create schedule::dispatch on BuiltinExpertHost

Create:

~~~text
crates/experts/builtin/src/schedule/dispatch.rs
~~~

with:

~~~text
pub async fn dispatch<Host: BuiltinExpertHost + ?Sized>(
    host: &Host,
    request: &BuiltinExpertRequest,
) -> Result<BuiltinExpertOutput, AgentFailure>
~~~

Export it from schedule/mod.rs.

### Dispatch sequence

1. validate assignment;
2. compute ScheduleRequestPlan;
3. construct exact CalendarViewQuery;
4. call host.calendar_views;
5. branch on SourceReadOutcome;
6. for Ready, obtain complete bounded evidence and run Schedule judgment;
7. for blocked states, return a bounded blocked result;
8. return BuiltinExpertOutput.

Do not call source_granted as Schedule mandatory-source admission. The actual Calendar read is the runtime observation.

### Ready

For Ready:

- require non-empty valid evidence before asserting schedule facts;
- preserve requested range;
- follow next_cursor using the same range;
- bound total pages/items/bytes;
- reject cursor cycles;
- do not reimplement admission;
- run judgment only after complete coverage.

A small Schedule pagination helper is acceptable. A new Calendar reader abstraction is not.

### Unavailable

Return a valid Schedule unavailable/blocked result.

Do not fabricate an empty Calendar.
Do not emit NoFocusWindow as authoritative.
Do not create an action proposal.

### NeedsUserAction

Preserve the exact SourceAccessRequirement.

Do not throw AccessReviewRequired only to keep old behavior.
Do not generate a UserInteractionRef before Checkpoint 05.

Carry the requirement in one typed auxiliary artifact. Add a stable media type if needed:

~~~text
application/vnd.floe.source-access-requirement+json;version=1
~~~

This artifact is a requirement description, not permission and not a durable interaction.

Task completion for the blocked result must allow Manager to perform the next iteration.

---

# 6. R3 — Move Schedule judgment out of schedule/host.rs

Target package:

~~~text
schedule/
  mod.rs
  dispatch.rs
  expert.rs
  plan.rs
~~~

Move into expert.rs only Schedule semantics:

- prompt;
- iterative ExpertReasoner loop if still required;
- capability request parsing;
- conflict/free-window/focus analysis;
- result helpers.

Delete the Schedule-local ExpertViews source abstraction.

Do not wrap BuiltinExpertHost into ExpertViews and preserve the old host. That leaves two infrastructure paths.

### Current failure behavior to remove

The old reasoning loop propagates source failure directly and requires an active successful view before Answer.

New rule:

- Ready complete evidence -> reason normally.
- Unavailable -> deterministic blocked result or typed unavailable observation.
- NeedsUserAction -> preserve requirement artifact and blocked result.
- integrity/storage/cancel/deadline -> hard AgentFailure.

A model cannot produce schedule facts without successful complete Calendar evidence.

### Successful output compatibility

For successful Schedule work, retain the current ExpertResult evidence shape in Checkpoint 02 so proposal inspection/private-state settlement remain compatible.

Blocked results may use a small separate result shape because they never carry action proposals.

Do not fake ExpertResult source/view fields for a blocked read.

---

# 7. R4 — Add one generic stateful built-in settlement hook

Checkpoint 01 already lets BuiltinExpertOutput carry settlement. The common host now needs to produce it.

Add one generic host operation or equivalent service with semantics:

~~~text
settle_stateful_result(
  request,
  draft
) -> BuiltinExpertOutput
~~~

Do not add settle_schedule_result.

A draft contains domain/evidence result fields only:

- source_handle;
- data_class;
- expires_at;
- insights;
- action proposals;
- summary;
- model call count;
- view call count.

It does not contain credentials, caller-chosen grant id, Registry snapshot, CalendarExpertSetup id, token or provider transport state.

### App-side implementation

The common App host:

1. loads current Registry;
2. resolves selected built-in assignment from request.agent_id;
3. validates package/assignment identity;
4. uses the existing assignment view handle only as current ExpertResult identity;
5. constructs ExpertResult;
6. stages private-state completion;
7. obtains exact captured Context dependencies;
8. creates ExpertSettlement with owner equal to selected Expert package id;
9. returns BuiltinExpertOutput with result + generic EndpointSettlement.

The existing granted view handle is temporary result identity only. It must not authorize the Calendar read. Checkpoint 03 removes the duplicated source authority.

### /focus proposal evidence

The common Context path uses Context-owned Calendar observation identity. The old proposal inspection still depends on Registry CalendarViewBinding.

For governed Calendar proposals, move validation to recorded ContextDependency:

- Person;
- source/connection identity;
- resources;
- observation id;
- current authority/freshness;
- Task/action origin.

Recognize the current Context Calendar observation handle and bind it to dependency.observation_id.

Do not require registry.calendar_view(result.view_handle) for a governed common Calendar read.

Action write authority and approval semantics remain unchanged.

### R4 tests

- correct selected Expert settlement;
- foreign owner rejected;
- stale Registry revision rejected;
- result exactness;
- successful /focus proposal inspectable;
- proposal evidence validated by ContextDependency;
- blocked result cannot publish an action.

---

# 8. R4.5 — Establish canonical first-party Calendar consumer authority

## Why this moved into Checkpoint 02

The first R5 cutover proved a dependency that the original checkpoint ordering missed.

The common host correctly reads Calendar for Schedule as:

~~~text
consumer = floe.builtin.schedule
~~~

while the legacy native and remote Calendar grant-creation paths currently create scopes containing only:

~~~text
consumer = calendar.expert
~~~

Admission is exact and therefore rejects the common Schedule consumer. This is correct fail-closed behavior. Do not weaken it with a compatibility rewrite.

R4.5 is now a hard prerequisite of R5. The rest of Checkpoint 03 remains deferred.

## Authority invariant

A ContextDependency records the actual consumer that performed the read.

Forbidden fixes:

- translate `floe.builtin.schedule` to `calendar.expert` at read time;
- treat `calendar.expert` as an implicit first-party group;
- special-case Schedule in Calendar admission;
- accept a grant merely because both consumer strings are first-party;
- silently expand an already-active legacy grant.

The canonical grant scope contains the explicit approved first-party consumer identities.

## Canonical policy owner

Access must remain unaware of the built-in Expert catalogue.

Create one product-composition helper in App, or the narrow composition layer that already knows both built-in declarations and Access contracts, that derives the Calendar first-party consumer set from canonical declarations:

~~~text
BuiltinExpertKind::ALL
  -> declaration.required_sources contains Calendar
  -> GrantConsumer::builtin(kind.package_id())
~~~

With the current catalogue that yields:

~~~text
floe.builtin.schedule
floe.builtin.commitments
floe.builtin.focus-attention
floe.builtin.wellbeing
~~~

Do not add `assistant` unless a current direct Manager Calendar-read path actually executes under that consumer and is covered by a focused runtime test.

Do not include extension/third-party ids.

Canonicalize, sort and deduplicate the consumer list; reject an empty Calendar policy.

## Thread policy into grant creation

Change grant creation/review APIs so product composition supplies the bounded consumer set. Do not create an admission-time alias.

### Native / temporary CalendarExpertSetup path

The temporary CalendarExpertSetup install/review path may still create the DataAccessGrant until Checkpoint 03.

Change it so:

- App supplies the canonical Calendar first-party consumer set;
- Vault/adapter uses that set when constructing GrantScope;
- no `calendar.expert` literal is injected by the adapter;
- setup ids and mappings may remain temporarily, but they do not decide the consumer set.

Do not make Vault depend on `floe-experts-builtin`.

### Remote Calendar review path

Change `remote_calendar_scope` / remote review activation so the caller supplies the canonical consumer set.

The grant preview must not present `calendar.expert` as if it were the real security identity. If the current preview DTO has one `consumer: String`, replace it with either a bounded consumer-id list or a product-facing summary plus the exact reviewed consumer list.

This is the only narrow Checkpoint 02 wire adjustment allowed by R4.5. Do not redesign the surrounding permission UX.

## Existing grants: no silent expansion

A grant previously reviewed only for `calendar.expert` does not prove consent for the new explicit first-party set.

Therefore:

- never mutate its scope in place solely because the application updated;
- never infer the new set from an old Registry enabled bit;
- an old grant remains non-admitting/review-required for the common Schedule path;
- a fresh explicit review or a fresh disposable development profile creates the canonical scope.

Because Floe is pre-stable, R5 success acceptance should use a fresh isolated profile. Checkpoint 03 later deletes the obsolete CalendarExpertSetup/mapping persistence entirely.

## R4.5 tests

Add table-driven tests proving:

1. Calendar first-party consumers are derived from built-in declarations, not a duplicated string list;
2. every built-in Expert that declares Calendar is present;
3. a built-in Expert that does not declare Calendar is absent;
4. extension/third-party consumers are absent by default;
5. a fresh native Calendar grant admits `floe.builtin.schedule`;
6. that native grant admits Commitments, Focus & Attention and Wellbeing under their exact ids;
7. a foreign consumer is denied;
8. a fresh remote Calendar grant carries the same canonical set and admits Schedule;
9. a legacy `calendar.expert`-only grant does not silently admit Schedule;
10. ContextDependency.consumer for a successful Schedule read is exactly `floe.builtin.schedule`.

Do not start R5 until native and remote fresh-grant success tests pass.

---

# 9. R5 — Put Schedule into common setup and registration

## catalog.rs

- builtin_setup_declarations derives from BuiltinExpertKind::ALL;
- remove BUILTIN_SETUP if it only means all except Schedule;
- Schedule installs through ExpertSetupSpec like every other built-in.

Do not create another seven/eight alias.

## expert_dispatch.rs

At registered_experts:

- change seven registrations to eight;
- add BuiltinExpertKind::Schedule -> schedule::dispatch;
- do not add any Schedule branch elsewhere in BuiltinExpertEndpoint.

Add a table-driven test proving registered ids equal BuiltinExpertKind::ALL.

## vault_host.rs

At sync_expert_directory:

- use one canonical built-in set;
- Schedule is synchronized like every other card.

At builtin_setup_specs:

- package Schedule through expert_setup_spec / expert_packaging.

### Existing CalendarExpertSetup

It may still exist for settings/persistence until Checkpoint 03.

During this checkpoint:

- common built-in assignment is the runtime Schedule assignment;
- old Calendar setup assignment is not used by Directory/runtime;
- do not synchronize the two assignments;
- identical package records may be reused;
- fixture-only packaging conflicts should be fixed in fixture tests, not with production compatibility.

### schedule_packaging

Old settings may still need package metadata.

Remove Schedule-specific packaging policy by calling the generic expert_packaging path for BuiltinExpertKind::Schedule.

---

# 10. R6 — Cut production routing and immediately delete old endpoint

R4.5 is a hard precondition. Before deletion, add one focused common-path E2E using a **fresh Calendar grant created with the canonical first-party consumer set**:

~~~text
Manager
 -> common Schedule card
 -> BuiltinExpertEndpoint
 -> schedule::dispatch
 -> BuiltinExpertHost::calendar_views
 -> Context / Access
 -> Schedule Task
 -> Manager answer
~~~

Also prove NeedsUserAction produces a completed blocked Schedule Task and lets the root Manager Run continue.

Once those pass, delete in the same cutover:

~~~text
crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule.rs
crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule/agent.rs
crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule/agent/tests.rs
~~~

Remove:

~~~text
pub(in crate::vault_host) mod schedule
ScheduleEndpoint::new
schedule_definition
direct Directory registration for Schedule
CalendarExpertEndpointRequest
CalendarExpertEndpointResult
run_calendar_expert_endpoint
BoundAccess
old endpoint-only access wrappers
old endpoint-only capability journal
old endpoint-only calendar_invocation
old endpoint-only schedule_expert helper
CALENDAR_EXPERT_SETTLEMENT_OWNER
SCHEDULE_DEFINITION_REVISION
~~~

Delete schedule/definition.rs.

No forwarding wrapper.

### Port old tests by responsibility

| Old responsibility | New owner |
|---|---|
| range/query | schedule plan/dispatch |
| native admission | Context native Calendar |
| remote admission | Context/Access remote |
| Schedule reasoning | schedule expert |
| settlement | generic Experts/Vault |
| delegation | common BuiltinExpertEndpoint E2E |
| proposal | Actions + common Schedule E2E |

No direct ScheduleEndpoint fixture remains.

---

# 11. R7 — Remove Schedule-specific history boundary

Current Conversation imports CalendarHistoryBoundary from Schedule.

Remove that dependency.

If SourceHistoryBoundary is still required by the current admission API, use a Conversation-owned conservative fallback:

- successful source Tool result may be source-derived;
- completed delegated Expert result/artifact may be source-derived;
- compaction is unknown/source-derived;
- user-authored messages remain user-owned.

Do not map Expert ids to sources.
Do not create BuiltinExpertHistoryBoundary.

Over-pruning is acceptable; exposing source-derived history after authority loss is not.

Checkpoint 06 may remove SourceHistoryBoundary entirely once stored coverage is sufficient.

Delete:

~~~text
crates/experts/builtin/src/schedule/calendar_history.rs
CalendarHistoryBoundary export
~~~

Conversation tests must no longer depend on calendar. or floe.builtin.schedule string prefixes for provenance.

Final Schedule package:

~~~text
schedule/
  mod.rs
  dispatch.rs
  expert.rs
  plan.rs
~~~

schedule/host.rs is deleted after domain logic moves to expert.rs.

---

# 12. Behavior matrix

## Successful native or remote Calendar read

~~~text
Schedule dispatch
 -> CalendarViewQuery
 -> Ready
 -> complete coverage
 -> Schedule judgment
 -> stateful ExpertResult
 -> generic settlement
 -> Task Completed
 -> Manager synthesis
~~~

No provider branch exists in Schedule.

## NeedsUserAction

~~~text
Calendar reader
 -> NeedsUserAction(SourceAccessRequirement)
 -> blocked Schedule result
 -> source-access-requirement artifact
 -> Task Completed
 -> Manager next iteration
 -> root Run can complete honestly
~~~

No fake evidence, no proposal, no random UserInteractionRef.

## Unavailable

Return a blocked/unavailable Schedule result. Do not interpret it as empty Calendar evidence.

## Hard failure

Integrity, foreign identity, Vault/storage error, cancellation, deadline and invalid model output remain hard AgentFailure.

## /focus

Only a complete successful Calendar read may create Focus proposal. Proposal remains bound to Task/invocation, common Schedule assignment, exact ContextDependency and existing Action authority.

---

# 13. Stop conditions

Stop and reassess before adding plumbing if the cutover appears to require:

1. forwarding ScheduleEndpoint;
2. a second Calendar reader;
3. copied DataAccessGrant state inside Schedule;
4. provider identity added to CalendarContextView only for routing;
5. both Schedule production endpoints registered;
6. Checkpoint 03 work just to make Checkpoint 02 compile;
7. optional legacy Schedule setup fields in BuiltinExpertRequest;
8. fake Calendar evidence for NeedsUserAction;
9. generated non-durable UserInteractionRef;
10. Action authority inside Experts.

---

# 14. Verification sequence

After R1: Schedule plan tests.

After R2/R3: floe-experts-builtin and focused common Expert dispatch tests.

Required cases:

- complete Ready;
- paginated Ready;
- cursor cycle;
- Unavailable;
- NeedsUserAction requirement artifact;
- hard failure;
- no answer before complete evidence;
- no proposal on blocked result.

After R4: focused Experts/Vault/Actions settlement and proposal tests.

After R5/R6:

~~~sh
cargo check --workspace --lib
cargo test --workspace --no-fail-fast
python3 tools/architecture/check_boundaries.py
cargo build -p floe-ffi
git diff --check
~~~

Flutter permission UX is not part of Checkpoint 02. Run Flutter only if the cutover changes client-visible result/wire shape.

Do not rerun native manual acceptance after every Schedule edit. Run relevant native/macOS validation once at final cutover only if native/provider code changed after the frozen baseline.

---

# 15. Residual gate

Checkpoint 02 is incomplete while production matches remain for:

~~~text
ScheduleEndpoint
CalendarExpertEndpointRequest
CalendarExpertEndpointResult
run_calendar_expert_endpoint
CALENDAR_EXPERT_SETTLEMENT_OWNER
schedule_definition
SCHEDULE_DEFINITION_REVISION
CalendarHistoryBoundary
ScheduleSetupCandidate
ScheduleSetupSelection
pub(in crate::vault_host) mod schedule
~~~

BuiltinExpertKind::BUILTIN_SETUP should be gone if it only represents the seven-Expert subset.

Search select_active_setup and verify the old Schedule setup selector is absent.

Search `calendar.expert`. At Checkpoint 02 completion it must not create or admit the canonical current Calendar grant and must not act as a consumer alias for common built-in reads. Historical/regression fixtures may mention it only to prove legacy scope is rejected; Checkpoint 03 removes the rest of the old vertical.

The following may remain for Checkpoint 03, but none may be on Schedule execution:

~~~text
CalendarExpertSetup
CalendarViewBinding
calendar_grant_mappings
SourceGrants
BuiltinSourceBinding
BuiltinSourceState
~~~

Report those matches explicitly rather than deleting them early.

---

# 16. Architecture doc update

After cutover, update docs/architecture/runtime.md to state:

- one BuiltinExpertEndpoint serves all built-ins;
- Schedule uses common Context Calendar reader;
- generic Task settlement validates selected Expert identity;
- no separate Schedule endpoint exists;
- remaining CalendarExpertSetup persistence is a Checkpoint 03 settings/authority concern, not Schedule runtime.

Remove the current production description:

~~~text
ScheduleEndpoint -> run_calendar_expert_endpoint
~~~

Do not turn this plan into a patch diary. Add only final completion evidence.

---

# 17. Recommended commit sequence

R1–R4 already exist on current main and must not be replayed.

1. **Authorize Calendar reads for canonical first-party consumers**
   - R4.5 only; native + remote fresh-grant tests.

2. **Cut Schedule over to the common Directory endpoint**
   - R5 plus green success/review E2E under the canonical consumer policy.

3. **Delete old Schedule endpoint and history boundary**
   - R6 + R7 + residual cleanup + architecture doc.

Do not return to Calendar-reader refinement without a failing common-path test.

---

# 18. Completion checklist

- [x] Schedule planning no longer requires provider/setup selection.
- [x] schedule::dispatch uses the common BuiltinExpertHost signature.
- [x] Schedule Calendar reads only through BuiltinExpertHost::calendar_views.
- [x] Schedule domain reasoning owns no source/provider host.
- [x] NeedsUserAction produces a valid blocked Schedule Task result.
- [x] exact SourceAccessRequirement survives in a typed artifact.
- [x] /focus proposal works through common path.
- [x] generic host/endpoint creates Schedule settlement.
- [x] canonical Calendar first-party consumer policy is derived from built-in declarations in product composition.
- [x] fresh native and remote grants admit `floe.builtin.schedule` under its exact consumer identity.
- [x] legacy `calendar.expert` scope is never treated as implicit approval for Schedule.
- [x] successful Schedule ContextDependency.consumer is `floe.builtin.schedule`.
- [x] builtin_setup_declarations includes Schedule.
- [x] registered_experts includes Schedule.
- [x] Schedule uses common BuiltinExpertEndpoint.
- [x] OpenVault no longer directly registers Schedule.
- [x] ScheduleEndpoint files deleted.
- [x] run_calendar_expert_endpoint deleted.
- [x] schedule_definition deleted.
- [x] CalendarHistoryBoundary deleted.
- [x] Conversation no longer imports Schedule for history classification.
- [x] Checkpoint 03-only state is not on Schedule execution.
- [x] successful common Schedule E2E passes.
- [x] NeedsUserAction common Schedule E2E passes and Manager root Run can complete.
- [x] Rust workspace, architecture and FFI gates pass.

The checkpoint is complete only after the **Schedule runtime cutover**, not because the generic Calendar reader is feature-complete.

---

# 19. Required agent report

When done, report:

1. **Schedule cutover**
   - new common path;
   - old endpoint deletion.

2. **Changed files/symbols**
   - grouped by R1–R7.

3. **Stateful settlement and proposal**
   - generic settlement hook;
   - ContextDependency-based proposal evidence.

4. **Remaining Checkpoint 03 surface**
   - CalendarExpertSetup / SourceGrants / grant mapping matches intentionally left;
   - proof they are not on Schedule execution.

5. **Residual search**
   - exact searches and justified remaining matches.

6. **Verification**
   - exact commands/results.

7. **Blocker**
   - if any criterion remains, stop and report it instead of broadening Calendar infrastructure.

Do not report Checkpoint 02 complete while ScheduleEndpoint or direct Schedule Directory registration exists.
