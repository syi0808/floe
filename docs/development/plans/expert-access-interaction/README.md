# Expert access and conversation interaction convergence

- **Status:** execution plan
- **Baseline:** `main` at `0615b343bb752da85487a1a728927f0e3affdf5b`
- **Current execution snapshot:** code baseline `main` at `d180cecdfc41d6f100cd9fe3389f7f53b6effca3`. Checkpoints 01, 02, and 03 (03-A through 03-E) are complete. Checkpoint 04 is next.
- **Scope:** built-in Expert runtime, Calendar source acquisition, Expert Registry, Access/DataAccessGrant authority, connector permission product model, conversation interaction escalation, Flutter chat/connection surfaces, protocol/persistence cleanup
- **Compatibility posture:** pre-stable internal APIs and local development data may be replaced directly. Do not add compatibility paths or migrate disposable local state merely to preserve the current Calendar vertical.
- **Primary product target:** Apple ecosystem. Android code may be adjusted only where shared contracts require compilation; do not expand Android parity work as part of this plan.

This directory is the authoritative execution plan for removing the Schedule/Calendar special path and making source-access recovery a first-class conversation flow.

It is deliberately split by checkpoint because the change crosses authority, runtime, persistence, wire, provider and Flutter boundaries. Each checkpoint must converge to one canonical path before the next checkpoint depends on it. Temporary dual paths are allowed only where this plan names them explicitly and must be deleted at the checkpoint's deletion gate.

This plan is not durable architecture documentation. When implementation completes, the current architecture and product/ADR documents named in checkpoint 6 become authoritative and this directory may be removed to Git history.

## 1. Problem statement

This plan began with two conflicting models. Checkpoint 02 has now removed the Schedule runtime split; the diagrams below describe the historical defect that motivated the work. The remaining checkpoints converge Registry/Access authority, connector permission UX, Conversation interactions and final obsolete surfaces.

The generic model was:

~~~text
Manager
  -> generic Directory / TaskCoordinator
  -> BuiltinExpertEndpoint
  -> BuiltinExpertHost
  -> provider-neutral Context views
  -> Access authorization
  -> provider/native adapters
~~~

Seven built-in Experts originally used that model while Schedule used the older Calendar vertical:

~~~text
Manager
  -> Directory
  -> ScheduleEndpoint
  -> CalendarExpertSetup / CalendarViewBinding
  -> calendar_grant_mappings
  -> Calendar-specific Access + source acquisition
  -> schedule-only Expert host / settlement
~~~

That exception leaks through App, Experts, Access/Vault, Conversation history, protocol and Flutter. It creates several concrete product defects:

1. Schedule is registered independently from the generic built-in Expert setup.
2. UI/Registry “Active” state can disagree with DataAccessGrant admission and current native Calendar authority.
3. legacy/disposable local state can contain enabled Schedule Registry objects without a matching Calendar grant mapping.
4. source permission is represented in both Expert Registry source bindings and Access/DataAccessGrant.
5. missing permission is represented primarily as AgentFailure rather than a normal recoverable source outcome.
6. Schedule’s capability loop hard-fails when Calendar read fails instead of returning the failed capability observation to the Expert.
7. Manager can already continue after a failed delegated Task, but the current product contract does not promote a recoverable source denial into a typed user action.
8. Flutter can receive failure strings but has no generic inline permission/recovery interaction.
9. connector permission UI is fragmented between Connections, Expert-specific Calendar UI, server connector grant controls, Data & privacy and the external-model processing toggle.
10. Conversation still depends on Schedule-owned CalendarHistoryBoundary to classify source-derived history.

The completed design must fix the authority model and runtime path, not only the observed Calendar error.

## 2. Final target architecture

### 2.1 Built-in Expert runtime

Schedule is an ordinary built-in Expert.

~~~text
BuiltinExpertKind::ALL
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
        common setup / Registry
               |
               v
        common Directory endpoint
               |
               v
        BuiltinExpertEndpoint
               |
               v
        per-domain dispatch()
~~~

There must be no Schedule-only App endpoint, Registry schema, Vault task settlement owner, install wire command or Flutter permission controller.

Schedule-specific **judgment** remains schedule-specific. Schedule-specific **infrastructure** does not.

### 2.2 Authority ownership

The final authority split is:

| Concern | Semantic owner |
|---|---|
| Expert identity, package/revision, enabled/disabled Expert | Experts / Agent Registry |
| Connection identity, credential presence, account/resource selection, source health | Connections |
| Whether a consumer may Observe selected data | Access / DataAccessGrant |
| Observe source acquisition and provider-neutral View projection | Context |
| Provider/OAuth/EventKit/native execution | provider/native adapter |
| action/write authority and review | Actions |
| assistant Session/Run/Manager response and assistant-triggered user interaction lifecycle | Conversation |
| UI rendering and explicit user choice | Flutter |

The same authority question must not be answered independently by two owners.

In particular:

- Expert Registry does **not** decide whether Schedule may currently read Calendar.
- Access does **not** reference Schedule installation IDs, assignment IDs or Registry view handles.
- Connection selection is the maximum source scope, not a second consumer grant.
- Observe permission never implies Act permission.
- every successful source dependency records the **actual admitted consumer identity**. Compatibility aliases such as `calendar.expert` must not stand in for `floe.builtin.schedule` or another real consumer.
- first-party source-consumer policy is assembled at product composition from canonical first-party declarations and passed to Access as grant scope; Access validates the scope but does not depend on the built-in Expert catalogue.
- model processing/recipient consent remains independent from Observe permission internally even when product UI is simplified.

### 2.3 Source use at runtime

All built-in Experts declare required/mandatory semantic sources, but runtime admission is performed at read time.

~~~text
Expert declaration:
  required_sources = [...]
  mandatory_source = ...

Expert dispatch:
  ask host for semantic source

Context:
  resolve current connection/source
  ask Access to authorize exact consumer/scope/purpose/processing
  acquire source
  return provider-neutral View + dependency

Expert:
  reason over View or typed unavailable/user-action outcome
~~~

The presence or absence of a current grant must not determine whether the Expert exists in Manager's catalog. An enabled Expert may be callable while one of its sources is disabled; that is how the system can explain the missing permission and create a recovery interaction.

### 2.4 User-action-required is not infrastructure failure

Expected recoverable states must not collapse into a hard root failure.

The source boundary distinguishes at least:

~~~text
Ready(view, dependency)
Unavailable(reason)
NeedsUserAction(requirement)
~~~

Hard AgentFailure remains for integrity, forged identity, corrupted durable state, exhausted hard limits, cancellation/deadline and other cases in which execution itself is invalid or cannot continue safely.

Examples:

| Condition | Runtime semantic outcome |
|---|---|
| connector Observe paused | NeedsUserAction(enable source) |
| no current OS Calendar permission | NeedsUserAction(system permission) |
| source identity/fingerprint changed | NeedsUserAction(review changed source) |
| OAuth credential expired | NeedsUserAction(reconnect) |
| exact external processing recipient not approved | NeedsUserAction(processing consent) |
| optional source temporarily down | Unavailable |
| invalid authority signature / foreign identity | hard failure |
| Vault unavailable/corrupt | hard failure |

### 2.5 Conversation interaction lifecycle

Do **not** keep a Run or Task alive indefinitely while waiting for a person.

The first turn completes with an interaction request linked to the exact origin:

~~~text
session_id
run_id / turn_id
task_id? / call_id?
source/connection target
requested operation/scope
~~~

Manager produces a normal user-facing response explaining the limitation.

Flutter renders a typed inline interaction card.

When the user decides:

1. the owner re-reads current connection/source/grant state;
2. the mutation is admitted using fresh authority and CAS;
3. the interaction is durably resolved;
4. if the decision enables the blocked operation, Conversation starts a linked follow-up turn using the original user intent and explicit interaction-resume linkage;
5. all access is reauthorized. The previous failed/blocked read is never blindly replayed as if it had succeeded.

This is intentionally separate from budget continuation. Existing continuation remains execution-budget semantics.

### 2.6 Connector product model

The common user path becomes:

~~~text
Connect source/account
  -> source authentication/system access
  -> select resources
  -> create/activate default Observe grant for Floe
  -> connected and usable
~~~

Each connection detail exposes one simple durable control:

~~~text
Use with Floe  [on/off]
~~~

Off pauses Observe admission without destroying credentials or selected resources. On performs fresh source/grant validation.

Action authority remains separate.

The generic Settings-level “LLM may use this connector/data” toggle is removed. Exact processing-recipient consent remains enforced internally and is requested contextually when required.

## 3. Repository baseline anchors

Line numbers below refer to the baseline commit and are planning anchors; symbols are authoritative if surrounding lines move during implementation.

| Area | Baseline anchor |
|---|---|
| built-in declaration split | crates/experts/builtin/src/catalog.rs:41, 62, 73, 141, 156 |
| common built-in output/host | crates/experts/builtin/src/host.rs:106, 128, 233 |
| Schedule private host loop | crates/experts/builtin/src/schedule/host.rs:38, 68, 115, 384, 395 |
| common App Expert registration | crates/app/src/vault_host/conversation_turn/expert_dispatch.rs:23, 29, 84, 107 |
| common Calendar view hook | crates/app/src/vault_host/conversation_turn/expert_dispatch.rs:407 |
| Schedule App endpoint | crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule.rs:17, 56, 79, 91 |
| Schedule endpoint settlement | crates/app/src/vault_host/conversation_turn/expert_dispatch/schedule/agent.rs:48, 54, 60 |
| Vault open / directory split | crates/app/src/vault_host.rs:195, 257 |
| Calendar Expert worker commands | crates/app/src/vault_host.rs:1576, 1645 |
| built-in setup helpers | crates/app/src/vault_host.rs:2521, 2570 |
| generic Registry source-grant duplication | crates/modules/experts/src/registry/expert_setup.rs:22, 49, 236, 294, 313, 448 |
| Calendar Expert Registry vertical | crates/modules/experts/src/registry/calendar_setup.rs:72, 125, 231, 414 |
| Calendar Expert access vertical | crates/modules/experts/src/calendar_access.rs:74, 93 |
| Calendar Vault grant mapping | crates/adapters/vault/src/vault/calendar_grants.rs:50, 188, 331, 1150 |
| Schedule-only Task settlement | crates/adapters/vault/src/repositories/task.rs:13, 72, 101 |
| Schedule-owned Conversation history boundary | crates/modules/conversation/src/turn/source_history.rs:14, 69; crates/app/src/vault_host/conversation_turn.rs:101 |
| generic runtime tool/delegation handling | crates/runtime/agent/src/engine.rs:633, 792 |
| conversation messages/events | crates/modules/conversation/src/turn/session.rs:124, 212 |
| Flutter chat presentation | apps/client/lib/features/conversation/presentation/agent_panel.dart:204, 289, 456 |
| Flutter Calendar Expert controller | apps/client/lib/features/experts/application/agent_calendar_expert_controller.dart:9, 223, 230 |
| Flutter Calendar Expert settings | apps/client/lib/features/experts/presentation/agent_calendar_expert_dialog.dart:18 |

## 4. Execution checkpoints

Read and execute these files in order.

1. [01 — contracts and recoverable source interaction foundation](01-contracts-and-interaction-foundation.md) — **complete**
2. [02 — Schedule common-runtime convergence + Calendar consumer-policy prerequisite](02-calendar-source-and-schedule-convergence.md) — **complete**
3. [03 — Expert Registry and Access authority convergence](03-expert-registry-and-access-authority.md) — **complete**
   - [03-A — Registry source-authority removal](03-a-registry-source-authority.md)
   - [03-B — Calendar Access persistence convergence](03-b-calendar-access-persistence.md)
   - [03-C — App, protocol and Flutter ownership cutover](03-c-app-protocol-flutter-cutover.md)
   - [03-D — deletion, verification and documentation convergence](03-d-deletion-verification.md)
   - [03-E — residual authority and Calendar access closure](03-e-residual-authority-calendar-access.md) — **complete**
4. [04 — connector permission product model](04-connector-permission-product-model.md) — **next**
5. [05 — Conversation and Flutter interaction/resume](05-conversation-and-flutter-interaction.md)
6. [06 — obsolete-path deletion, verification and documentation convergence](06-deletion-verification-and-doc-convergence.md)

Do not skip directly to Flutter. A chat permission button is unsafe until its target and decision path are owned by the canonical Access/Connections path.

Checkpoint 02 moved the narrow Calendar first-party consumer-policy prerequisite forward and is complete. Checkpoint 03-A through 03-D established the source-independent Registry and source-owned Calendar grant path, and 03-E closed the post-03-D behavioral residuals in grant selection/CAS/policy lifecycle plus the native Access management surface. The deleted Expert Calendar vertical was not reopened.

Do not make connection-time Observe implicit before Checkpoint 03 has removed the competing Expert Registry permission authority. Do not start Checkpoint 04's final connection ceremony or Checkpoint 05's durable chat interactions from a Checkpoint 03 patch.

## 5. Global implementation invariants

Every checkpoint must preserve these properties.

### Authorization and identity

- current Person/device/connection identity is obtained from canonical owners;
- no request supplies credentials, bearer tokens or arbitrary connection endpoints;
- native subject/fingerprint changes fail closed and require a fresh review where policy requires it;
- exact recipient / processing restriction remains enforced at the dispatch/release boundary;
- every successful ContextDependency consumer equals the actual caller admitted for that read; no service alias may hide or broaden consumer identity;
- first-party consumer policy never auto-includes extension/third-party package ids;
- a user-interaction approval is not a source-read receipt and never bypasses fresh admission.

### Provenance

- every successful source result records exact ContextDependency coverage;
- Manager/Expert must not convert denied/unavailable reads into empty evidence;
- historical source-derived content is retained only if recorded dependencies reauthorize;
- removing CalendarHistoryBoundary must not weaken this rule.

### Persistence and CAS

- interaction decisions, grant changes and connection changes use expected revision/authority semantics;
- duplicate decision submission is idempotent or conflicts safely;
- app restart can inspect the resolved/pending interaction without inventing a decision;
- no global transaction is held during model/provider/native I/O.

### Runtime/cancellation

- child Task cancellation never cancels the parent Manager Run unless the parent explicitly cancels;
- an interaction does not leave an executor lease or timer running while waiting for the user;
- retry/resume uses fresh authority and stable origin linkage, not a stale source result.

### External side effects

- Observe grant changes never imply write authority;
- Calendar create/update/delete continues through Actions policy/review/recovery;
- uncertain external writes are never retried as part of interaction resume.

## 6. Explicit non-goals

This plan does not:

- add new Android product capability;
- make third-party Experts implicitly trusted;
- grant connector Act permission at connection time;
- weaken external-processing recipient consent;
- migrate disposable old local Calendar Expert state;
- preserve experts.calendar.* wire compatibility;
- add a second “v2/vNext” runtime;
- make Experts render Flutter UI;
- make prompts encode permission workflows;
- turn connection credentials into domain/Expert input.

## 7. Local data/reset policy

The current product is pre-stable and local development state is disposable. The implementation should prefer deleting the obsolete Calendar-Expert persistence shape instead of migrating it.

If a changed schema cannot safely open an old development profile:

1. detect the incompatible Floe-owned schema explicitly;
2. fail with a clear development reset requirement;
3. reset only the identified Floe-owned local profile/key slots when the operator explicitly chooses reset;
4. never delete provider data, unrelated files, credentials whose ownership is uncertain, or unresolved external-action records.

No code path may infer a new active DataAccessGrant from an old Registry enabled bit.

## 8. Required reporting per checkpoint

An implementation agent should report only after the checkpoint is internally converged.

Use this structure:

1. changed files and symbols;
2. final owner/contract introduced or changed;
3. callers migrated;
4. obsolete symbols/routes deleted in this checkpoint;
5. residual searches and remaining matches;
6. targeted verification;
7. broader verification required by the checkpoint;
8. explicit blocker, if any.

A checkpoint is not complete because the new path compiles. It is complete when its named old path is absent or the plan explicitly says the old path survives until the next checkpoint.

## 9. Final definition of done

The whole plan is complete only when all of the following are true:

- Schedule is included in the same built-in setup, Directory registration and endpoint dispatch as the other seven Experts.
- no Schedule-specific App AgentEndpoint remains.
- no CalendarExpertSetup/CalendarExpertOverview/CalendarAccessConfiguration product contract remains.
- Expert Registry no longer acts as a source-access authority.
- Access/DataAccessGrant is the only consumer Observe authority.
- canonical first-party Calendar grants authorize real approved first-party consumer identities; the legacy `calendar.expert` compatibility consumer is not production authority.
- Access persistence does not store Expert installation/assignment IDs as grant identity.
- disabled/review-required sources do not make an enabled Expert disappear from the Manager catalog.
- an expected source permission problem reaches the Expert/Manager as a typed recoverable outcome rather than terminating the root turn.
- Manager returns a natural user-facing answer for the blocked request.
- chat can render and resolve a typed permission/recovery interaction.
- interaction resolution revalidates source/grant state and can start a linked follow-up turn.
- connection detail owns Use with Floe.
- connection establishment activates the default Observe grant defined by the amended product decision.
- external model recipient consent is no longer a disconnected Settings “LLM connector access” toggle.
- Observe and Act remain distinct.
- Conversation has no dependency on Schedule-specific CalendarHistoryBoundary.
- source history is governed by recorded dependency provenance.
- experts.calendar.* wire and Flutter agent_calendar_expert_* implementation surfaces are gone.
- architecture docs and affected ADR/product docs describe the resulting code.
- residual search and the full applicable Rust/FFI/Flutter/macOS gates pass.
