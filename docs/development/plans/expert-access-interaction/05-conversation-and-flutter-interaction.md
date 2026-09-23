# Checkpoint 05 — Conversation-owned user interaction and Flutter chat resume

## Goal

Complete the product path from an agent/tool source requirement to an inline Flutter chat interaction without teaching Manager or Experts anything about Flutter/navigation.

The canonical flow is:

~~~text
Manager or Expert
  -> source/capability call
  -> Context/Access returns NeedsUserAction
  -> host creates Conversation interaction record
  -> Tool/Task carries interaction reference
  -> Manager receives failed/blocked observation and answers naturally
  -> Conversation commits assistant response + interaction message
  -> Run completes
  -> Flutter renders interaction card
  -> user resolves / opens owning connection
  -> owner performs fresh mutation/revalidation
  -> interaction resolves
  -> Conversation starts a linked follow-up turn
  -> blocked work runs again under fresh authority
~~~

The original Run is not held open while waiting for the user.

## Baseline anchors

- crates/runtime/agent/src/engine.rs:633 — Manager execute_tool
- crates/runtime/agent/src/engine.rs:792 — execute_delegation
- crates/modules/experts/src/task.rs — Task terminalization on endpoint failure
- crates/adapters/providers/src/models/wire.rs — ToolExchange/DelegationExchange rendering to model
- crates/modules/conversation/src/turn/session.rs:124 — AgentMessage
- crates/modules/conversation/src/turn/session.rs:212 — AgentEventKind
- crates/modules/conversation/src/domain/mod.rs — RunState / TurnMode / RunReceipt
- crates/modules/conversation/src/application/admission.rs — prepare/admit turn
- crates/modules/conversation/src/application/recovery.rs — continuation/recovery
- crates/app/src/vault_host/conversation_turn.rs — root turn preparation
- apps/client/lib/features/conversation/domain/agent_session.dart:118+ — delegation/capability parsing
- apps/client/lib/features/conversation/presentation/agent_panel.dart:204 — message rendering
- apps/client/lib/features/conversation/presentation/agent_panel.dart:289 — composer/footer
- apps/client/lib/features/conversation/presentation/agent_panel.dart:456 — top-level failure status
- apps/client/lib/features/conversation/application/conversation_runtime_gateway.dart — Run polling
- apps/client/lib/features/conversation/application/agent_controller.dart — controller state/actions

## 1. Persist assistant-triggered interaction records under Conversation

### 1.1 Why Conversation owns this record

The interaction is part of a specific assistant request and has origin identity:

- Session;
- Run/turn;
- optional delegated Task;
- optional Tool/capability call;
- Manager-visible source requirement.

Conversation owns that lifecycle.

The underlying mutation is **not** owned by Conversation:
- Observe toggle/review -> Access/Connections;
- system permission -> native/Connections boundary;
- reconnect -> Connections/provider auth;
- action approval -> Actions;
- processing recipient consent -> Access/Inference authority.

Conversation stores the request and its resolution outcome; it delegates the actual operation to the correct owner through App composition.

### 1.2 Interaction record

Add a durable record with at least:

~~~text
ConversationInteraction
  schema_version
  id
  person_id
  session_id
  origin_run_id
  origin_turn_id
  origin_task_id?
  origin_call_id?
  requirement
  state
  created_at
  resolved_at?
  resolution?
  revision
~~~

State:

~~~text
Pending
Resolved
Denied
Cancelled
Superseded
~~~

Do not add “Executing” unless an interaction owner operation truly needs a durable in-flight state. Most connection/access commands already have their own operation identity and can be reconciled.

Resolution should record semantic outcome, not raw provider/native response.

### 1.3 Origin constraints

Validate:
- Person matches Session owner;
- origin Run belongs to Session;
- Task belongs to origin Run if present;
- call id belongs to origin Run/Task if present;
- source requirement consumer matches the caller that produced it;
- interaction id is stable across replay of the same settled origin result.

Do not create duplicate pending interactions every time the same journaled Tool/Task result is projected.

Use stable identity derived from the settled invocation identity or persist the generated id in the result/journal before it can be observed.

## 2. Interaction creation boundary

### 2.1 Tool path

When ContextToolService receives SourceReadOutcome::NeedsUserAction:

1. ask the injected Conversation interaction sink/port to create-or-replay the interaction for the current Run/call identity;
2. return ToolResult with:
   - no source payload;
   - no dependent source coverage;
   - bounded issue/status saying user action is required;
   - typed UserInteractionRef artifact.

Do not throw AgentFailure solely for this expected condition.

### 2.2 Expert path

When a built-in Expert source read returns NeedsUserAction:

- the host creates/replays the Conversation interaction using Task/call origin;
- the Expert receives ExpertCapabilityObservation::NeedsUserAction when it is in an iterative reasoning loop;
- mandatory-source dispatch may return a deterministic blocked/no-conclusion Expert output with the same interaction artifact;
- the resulting A2A Task is Completed when the Expert correctly reports the blocked domain judgment;
- Task Failed remains for actual execution/integrity failure.

This is the desired semantic difference:

~~~text
Calendar permission is off
  -> Expert successfully reports "cannot judge until Calendar is enabled"
  -> Task Completed / domain status needs_user_action

Vault corrupt
  -> Expert execution failed
  -> Task Failed
~~~

### 2.3 Multiple interactions

One turn may create more than one interaction only if they refer to distinct blocked operations.

Bound the count tightly, for example <= 8 per Run.

If several source reads hit the same connection/requirement, deduplicate by semantic target and origin lineage where safe.

Do not aggregate different connections into one approval.

## 3. Manager model behavior

### 3.1 Provider wire

Update ToolExchange and DelegationExchange model rendering so the Manager sees:

- the operation did not obtain source evidence;
- the semantic reason in bounded product language;
- that a user action is available;
- no secret/authority internals.

Example semantic payload:

~~~json
{
  "status": "needs_user_action",
  "source": "calendar",
  "reason": "observe_disabled",
  "interaction_id": "..."
}
~~~

Do not send:
- fingerprint;
- grant authority keys;
- OAuth state;
- provider bearer;
- Flutter route;
- button labels.

### 3.2 Manager prompt

Amend manager_role.txt minimally.

The current role already says failed source-backed reads are unavailable and the Manager should state the limitation honestly.

Add only the general rule:

- when a host-provided user interaction is available, explain the required user decision briefly;
- do not invent a different scope/account/action;
- do not claim approval before the interaction resolves;
- continue with remaining evidence when useful.

Do not add Calendar-specific examples or workflow scripts.

### 3.3 Run outcome

A turn that produces a correct limitation response plus an interaction is normally Completed.

Do not set the root Run to Failed just because the requested source requires user action.

The interaction is an expected product outcome, not an execution failure.

Hard root failures remain hard.

## 4. Commit interaction as a first-class conversation message

Add the final AgentMessage representation prepared in checkpoint 01:

~~~text
AgentMessage::Interaction {
  turn_id,
  interaction_id,
  kind,
  status
}
~~~

Conversation commits it after the associated source/tool/task result is settled and before/with final response projection according to the existing message ordering rules.

Recommended UI order in the turn:

~~~text
User
Preamble/source technical entries as applicable
Assistant limitation/final response
Interaction card
~~~

If the current message commit architecture requires interaction before final answer, Flutter may reorder only presentation of messages from the same turn if the stored canonical order remains unambiguous. Prefer canonical order that matches presentation.

### 4.1 Message source derivation

Interaction metadata contains no source content. AgentMessage::may_derive_from_source() should therefore treat it as non-source-derived.

Its origin Task/Tool may still have Unknown/Independent coverage because no successful source data was obtained.

### 4.2 Event stream

MessageCommitted already carries AgentMessage. Reuse it unless a separate interaction state-change event is required for updates after the Run finishes.

Because interaction resolution can occur later, add a bounded runtime event or query projection for status changes, e.g.:

~~~text
InteractionUpdated
  interaction_id
  state
  revision
~~~

Do not mutate an old committed message in place without a durable state record.

## 5. Interaction inspect/resolve protocol

Add owner-aligned App commands/queries, not Expert-specific commands.

Suggested wire semantics:

~~~text
conversation.interaction.get
conversation.interaction.resolve
conversation.interaction.refresh
~~~

Resolve request:
- interaction id;
- expected interaction revision;
- decision enum appropriate to the generic lifecycle, e.g. Approve / Deny;
- no connection/grant/fingerprint fields copied from Flutter.

The backend loads the interaction target and calls the owning source/access operation.

### 5.1 Inline enable

For EnableObserve:
- load current connection;
- validate current source;
- activate/review the current Observe grant;
- mark interaction Resolved only after owner mutation succeeds;
- return effective access state.

### 5.2 System permission

For RequestSystemPermission, Rust cannot impersonate the user’s OS decision.

The interaction projection tells Flutter the required native action class.

Flow:

~~~text
Flutter button
 -> native permission request / system settings
 -> refresh connection
 -> conversation.interaction.refresh
 -> backend verifies source is now admissible
 -> interaction Resolved
~~~

Do not mark approved merely because Flutter says the dialog was shown.

### 5.3 Reconnect/resource selection

For Reconnect or SelectResource:
- interaction card opens the owning connection detail;
- after the connection operation completes, Flutter asks backend to refresh the interaction;
- backend verifies the requirement no longer applies;
- only then mark Resolved.

No connection screen directly edits Conversation storage.

### 5.4 Denial

Deny:
- mark interaction Denied;
- do not mutate source permission;
- do not auto-resume;
- preserve the completed origin turn and audit identity.

A later user request can create a new interaction if they ask again.

## 6. Linked follow-up turn

### 6.1 Do not reuse budget continuation

Existing TurnMode::Continue and AgentContinuation mean “continue execution after budget/deadline soft stop”.

Permission resolution is a new product event. Add an explicit interaction resume reference.

Recommended domain value:

~~~text
InteractionResumeRef
  interaction_id
  origin_run_id
  interaction_revision
~~~

Conversation start/admission validates:
- interaction is Resolved;
- origin Run belongs to the same Session/Person;
- interaction points at that origin;
- the resolution has not already started a successful resume command, or duplicate command identity rejoins idempotently.

### 6.2 User text

Do not require Flutter to invent a new user message such as “continue”.

Conversation can recover the original user intent from the origin Run/turn.

The new Run should have explicit mode/origin metadata so UI does not render the original user text as if the user typed it twice.

Possible final shape:

~~~text
TurnMode
  New
  Continue(BudgetContinuationRef)
  ResumeInteraction(InteractionResumeRef)
~~~

The new Run receives a host-scoped instruction/context event meaning “the previously blocked requirement has been resolved; continue the original request”. Do not encode this as arbitrary user text.

### 6.3 Fresh execution

The resume is a new Run:

- Manager chooses delegation/tools again;
- source read executes again;
- Access reauthorizes current grant/source;
- no old ToolResult is treated as successful;
- new dependencies are recorded.

This avoids keeping executor/deadline/budget state alive while a person decides.

### 6.4 Interaction resolution races

Handle:
- source enabled from Settings/connection before user presses Allow;
- connection removed while interaction pending;
- source authority changed before resolve;
- interaction clicked twice;
- app restart between resolve and resume start;
- resume command accepted but response lost.

Use existing command/run identity and CAS/recovery rules. Persist enough linkage that restart can determine whether resume was already admitted.

## 7. Flutter domain/controller

### 7.1 Domain parsing

In apps/client/lib/features/conversation/domain/agent_session.dart:

- add AgentInteractionMessage;
- parse interaction message kind;
- validate UUID/status/kind;
- do not infer target details from failure strings;
- keep capability/delegation technical messages separately inspectable.

### 7.2 Controller

Add to AgentController or a focused conversation-interaction controller owned by AgentController:

- inspectInteraction(id);
- resolveInteraction(id, decision);
- refreshInteraction(id);
- resumeResolvedInteraction(id) when backend reports resumable;
- busy state scoped to the interaction so the composer does not become a global error state unnecessarily.

Do not route through AgentCalendarExpertController.

### 7.3 Runtime gateway

ConversationRuntimeGateway currently polls until AppRunState.finished.

Keep that behavior. Origin turn ends before user decision.

Add explicit interaction command/query methods through the same FloeClient/App read model boundary.

When resolve returns a linked resume command:
- run it using the standard conversation runtime polling;
- stream normal run progress;
- update the same Session;
- preserve cancellation semantics for the new Run.

## 8. Flutter presentation

### 8.1 Inline card

Add a generic AgentInteractionCard rendered from AgentInteractionMessage.

Example states:

~~~text
Calendar access is off
Floe needs Calendar access to answer this request.

[Not now] [Allow]
~~~

For non-inline repair:

~~~text
Calendar needs attention
Choose the calendars Floe can use.

[Manage Calendar]
~~~

The copy comes from localized interaction kind/reason plus safe display metadata, not from provider error strings.

### 8.2 Card actions

Actions depend on host projection, not Agent choice:

- inline approval;
- deny/not now;
- request OS permission;
- open connection;
- reconnect.

Do not expose arbitrary URLs from model/tool output.

### 8.3 Technical capability UI

The existing ExpansionTile “Source” can continue to show technical source/delegation details, but recovery is no longer hidden there.

Failed delegation/capability messages should not be the only visible signal.

### 8.4 Footer/top-level failure cleanup

agent_panel.dart baseline _status() maps top-level access_review_required/capability access errors to footer failures.

After canonical source blockers migrate:
- expected access interactions should appear as messages/cards, not controller.failure;
- keep top-level mappings only for truly top-level failures or remove them if unreachable;
- do not reinsert the user’s composer text simply because an expected interaction was produced. The Run completed.

## 9. Action proposal coexistence

AgentProposalCard is the existing pattern for “agent work creates a durable owner record and Flutter renders an inline affordance”.

Keep it separate from permission interaction:
- proposal = potential external Act;
- permission interaction = access/recovery decision.

Both may share visual primitives but not authority/state types.

Do not migrate Calendar action approval into Conversation interaction just for UI uniformity.

## 10. Tests

### Rust Conversation

1. direct Manager tool NeedsUserAction:
   - interaction record persisted once;
   - Manager receives observation;
   - final answer committed;
   - Interaction message committed;
   - Run Completed.

2. delegated Schedule NeedsUserAction:
   - Task Completed with blocked domain result;
   - interaction artifact/ref preserved;
   - Manager final answer;
   - Run Completed.

3. hard source integrity failure:
   - no interaction;
   - correct hard failure behavior.

4. deny:
   - interaction Denied;
   - no grant mutation;
   - no resume Run.

5. approve inline:
   - current owner revalidated;
   - grant active;
   - interaction Resolved;
   - one linked resume Run;
   - resumed read succeeds.

6. app restart:
   - pending interaction inspectable;
   - resolved interaction does not duplicate mutation;
   - accepted resume command recovered by id.

7. source changed before approval:
   - old interaction becomes Superseded/NeedsReview;
   - stale approval cannot activate.

8. duplicate click:
   - same decision is idempotent or returns current resolved state;
   - no duplicate resume.

### Model behavior

Use deterministic fixture responses:
- first Manager iteration delegates;
- Schedule returns needs_user_action;
- second Manager iteration answers limitation.
Verify provider call counts and exact delegation/tool history.

After resolution:
- linked Run makes a new source read;
- model sees successful evidence;
- final answer does not claim data from the blocked first Run.

### Flutter

agent_session tests:
- parse interaction;
- reject malformed identity/state.

agent_panel tests:
- inline Allow/Not now card;
- Manage connection card;
- resolved/denied state rendering;
- no global error badge for normal interaction.

controller/runtime tests:
- resolve -> linked resume;
- deny -> no resume;
- lost response recovery;
- refresh after connection screen;
- cancellation applies only to active linked Run.

Golden:
- update/add one focused agent-panel interaction golden after behavior tests pass.

## 11. Residual audit

Search expected permission handling:

~~~text
access_review_required
consent_required
capability_access_denied
ReviewSource
agentAccessReviewRequired
recoveryAction == 'review_source'
failureSafeActions
AgentCalendarExpertController
~~~

For conversation source access, normal permission recovery should no longer depend on top-level AgentVaultFailureDto safe_actions.

Search interaction lifecycle:

~~~text
UserInteractionRef
AgentMessage::Interaction
ConversationInteraction
ResumeInteraction
~~~

Verify there is one canonical interaction record and one resume path, not artifact-only plus message-only duplicate authorities.

## 12. Verification

Rust:
- targeted Conversation/Runtime/Experts/Access/Vault tests;
- restart/replay/CAS tests;
- cargo check --workspace --lib;
- cargo test --workspace --no-fail-fast;
- architecture boundary check;
- git diff --check.

FFI/protocol:
- cargo build -p floe-ffi;
- protocol tests;
- C ABI/app wire tests.

Flutter:
- flutter analyze;
- agent_session tests;
- agent_panel tests/golden;
- conversation_runtime_gateway tests;
- full flutter test;
- flutter build macos.

Manual macOS acceptance on fresh dev profile:
1. connect Calendar and turn Use with Floe Off;
2. ask “What is on my calendar today?”;
3. Manager responds that Calendar access is needed;
4. inline card appears;
5. Allow;
6. linked follow-up runs automatically;
7. Manager returns actual Calendar answer;
8. turn Off again;
9. repeat and choose Not now;
10. no retry occurs;
11. revoke EventKit permission, request again, use system-permission recovery, return and verify fresh read.

## 13. Checkpoint exit criteria

Checkpoint 05 is complete when:

- agents only request sources/capabilities; they do not know Flutter/navigation;
- a recoverable source blocker creates one durable Conversation interaction;
- Manager receives the blocked observation and returns normal user-facing text;
- origin Run completes instead of waiting;
- Flutter renders a generic inline interaction card;
- inline and connection/system recovery use owner-authorized operations;
- resolution never bypasses fresh Access admission;
- resolved interaction can start one linked follow-up Run with explicit resume identity;
- duplicate/restart behavior is deterministic;
- expected source permission no longer surfaces only as a global chat error;
- targeted and broad Rust/FFI/Flutter/macOS gates pass.

Checkpoint 06 is now pure convergence: delete old surfaces, update architecture/ADR/product docs, and prove residual absence.
