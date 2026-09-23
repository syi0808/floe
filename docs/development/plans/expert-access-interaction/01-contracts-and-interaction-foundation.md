# Checkpoint 01 — contracts and recoverable source interaction foundation

## Goal

Introduce the minimum canonical contracts required for a source read to say “this operation is currently blocked, and here is the exact user action that can recover it” without converting that state into a generic infrastructure failure.

This checkpoint establishes the contracts and transport carriers only. It must not yet change connector default permission policy or remove the Schedule endpoint. The old Schedule vertical is allowed to exist until checkpoint 02, but new interaction/source-outcome contracts must already be usable by both Manager tools and Expert source reads.

## Baseline anchors

- crates/contracts/agent/src/expert_model.rs — ExpertStep / ExpertTranscriptEntry / ExpertReasoningStep
- crates/contracts/agent/src/model.rs — ToolResult / OutcomeIssue / ModelConversationEntry
- crates/contracts/context/src/lib.rs — ContextDependency, source/grant value contracts
- crates/experts/builtin/src/host.rs:106 — BuiltinExpertOutput
- crates/experts/builtin/src/host.rs:128 — BuiltinExpertHost
- crates/experts/builtin/src/host.rs:233 — require_mandatory_source
- crates/modules/experts/src/dispatch.rs — completed_expert_task(), expert_report(), task_receipt_to_a2a()
- crates/runtime/agent/src/engine.rs:633 — execute_tool()
- crates/runtime/agent/src/engine.rs:792 — execute_delegation()
- crates/modules/conversation/src/turn/session.rs:124 — AgentMessage
- crates/modules/conversation/src/turn/session.rs:212 — AgentEventKind

Line anchors refer to baseline commit 0615b343bb752da85487a1a728927f0e3affdf5b.

## 1. Design the source-read outcome before touching callers

### 1.1 Add a provider-neutral blocker contract

Add a pure value contract at the Context contract layer. The contract must not name Flutter widgets, route names, EventKit classes, OAuth endpoints or Expert package IDs.

Recommended shape:

~~~text
SourceReadOutcome<T>
  Ready(T)
  Unavailable(SourceUnavailable)
  NeedsUserAction(SourceAccessRequirement)
~~~

Hard execution errors remain the outer Result error:

~~~text
Result<SourceReadOutcome<T>, AgentFailure>
~~~

This distinction is important:

- Ready is authoritative evidence and carries normal ContextDependency.
- Unavailable is an expected no-evidence state that cannot presently be repaired through one explicit user decision, or for which continuing without the source is valid.
- NeedsUserAction is an expected no-evidence state with a stable recovery subject and an explicit action class.
- AgentFailure remains for invalid input, integrity/security failure, storage/vault failure, cancellation/deadline and other failures where the operation itself cannot be treated as a valid source observation.

Do not create an enum whose “Failure” case just wraps every AgentFailure. Keep hard failure in Result so callers cannot accidentally render corrupted state as a normal permission prompt.

### 1.2 SourceAccessRequirement fields

The value must contain enough stable identity for the host to create a user interaction without re-deriving the target from strings.

Required semantic fields:

- source id, e.g. floe.source.calendar;
- connector id when a connector owns the source;
- stable connection id when a concrete connection is known;
- requested operation, initially Read;
- consumer identity;
- purpose;
- resource handles or a bounded user-facing scope summary;
- reason enum;
- current source authority reference/revision when available;
- whether resolving the requirement can be done inline or requires opening the owning connection surface.

Recommended reason enum:

~~~text
SourceAccessRequirementKind
  EnableObserve
  ReviewChangedSource
  RequestSystemPermission
  Reconnect
  ApproveProcessingRecipient
  SelectResource
~~~

Do not encode “open Settings”, button labels or route names in Rust domain contracts.

Do not put bearer credentials, provider tokens, raw native identifiers not already safe in the current connection contract, or provider-specific authorization objects in the requirement.

### 1.3 Stable reason mapping

Existing AgentFailure values continue to exist for compatibility inside current code during this checkpoint, but new source owners must stop using them for expected recoverable states when they can produce SourceReadOutcome.

Do **not** globally map the following errors inside Agent Runtime based on enum name alone:

- CapabilityDenied
- ConsentRequired
- AccessReviewRequired
- CredentialExpired

Those values do not carry enough target identity. A generic “AgentFailure -> permission card” mapping would recreate the current problem.

Instead, the owner that knows the source/connection must construct SourceAccessRequirement.

During the checkpoint, legacy callers may still return AccessReviewRequired; that is temporary and must be listed in the residual report.

## 2. Define the agent-facing interaction reference

### 2.1 Pure reference, not mutation authority

Add a small agent contract for an assistant-triggered interaction reference.

Recommended fields:

~~~text
UserInteractionRef
  interaction_id
  kind
  status
~~~

The reference is safe for ToolResult / Task artifacts and model projection. It is **not** authorization to perform the requested mutation.

The detailed request and decision state remain owned by Conversation in checkpoint 05. The reference only lets execution and presentation point at that durable record.

### 2.2 Media type

Introduce one typed media type for an interaction reference carried by Tool/Task artifacts, for example:

~~~text
application/vnd.floe.user-interaction+json;version=1
~~~

Keep the media type in the agent/Experts contract package that already owns A2A artifact media types. Do not create a Flutter-only payload.

The model may see a bounded semantic status such as “calendar access needs user action”, but it must not receive internal security fingerprints or secret-shaped state.

## 3. Generalize Expert output carriers before Schedule migration

### 3.1 BuiltinExpertOutput

At crates/experts/builtin/src/host.rs:106, extend BuiltinExpertOutput so a built-in Expert can return:

- its normal domain result artifact;
- zero or more auxiliary typed artifacts, initially interaction references;
- optional generic settlement metadata if required by a domain.

Do not add Schedule-named fields.

A viable end state is conceptually:

~~~text
BuiltinExpertOutput
  artifact_name
  summary
  data
  artifacts[]
  settlement?
~~~

The exact artifact type must follow the existing dependency policy. Prefer existing agent Artifact/A2A artifact contracts rather than inventing a parallel payload representation.

BuiltinExpertOutput::from_result() should remain the normal convenience constructor and produce no auxiliary artifacts.

Add a second constructor/builder only if it represents the durable optional-artifact semantic; do not add a migration-only “legacy” constructor.

### 3.2 Experts dispatch path

In crates/modules/experts/src/dispatch.rs:

- completed_expert_task() must preserve auxiliary artifacts in addition to the primary expert-result artifact;
- expert_report() must forward the Task artifacts that are intended for the delegating Manager instead of unconditionally returning artifacts: vec![];
- task_receipt_to_a2a() must preserve valid auxiliary artifacts for completed Tasks;
- completed Task validation must continue to require exactly one primary expert result;
- an interaction artifact must never be mistaken for the primary expert-result artifact.

Add tests with:
1. one primary Expert result and one interaction artifact;
2. missing primary result + interaction only -> invalid;
3. duplicate primary results -> invalid;
4. malformed interaction artifact -> invalid;
5. completed Task artifact propagation into ExpertReport.

### 3.3 Generic settlement

Schedule currently relies on EndpointSettlement while the common Expert path always emits settlement: None.

Before checkpoint 02, make the common endpoint path capable of forwarding a generic optional ExpertSettlement from BuiltinExpertOutput/Expert report assembly.

Do **not** yet change Vault settlement implementation in this checkpoint. The requirement here is only that the common path can carry settlement without a Schedule-specific endpoint.

Tests must prove:
- no settlement remains the normal path for other Experts;
- a well-formed generic settlement can reach EndpointSettlement;
- settlement owner/payload validation remains bounded;
- no settlement payload leaks into serialized public ExpertReport, matching the existing endpoint contract.

## 4. Make Expert reasoning transcripts capable of observing a failed capability

Schedule’s private iterative host currently uses ExpertTranscriptEntry::Capability with a success-only result String. That prevents an Expert model from seeing a typed failed read without throwing.

Change the Expert reasoning contract in crates/contracts/agent/src/expert_model.rs so capability transcript entries distinguish success from a recoverable unavailable/user-action observation.

Do not serialize arbitrary AgentFailure debug text.

Recommended semantic representation:

~~~text
ExpertCapabilityObservation
  Success { result }
  Unavailable { reason_code }
  NeedsUserAction { interaction_ref, summary }
~~~

Then:

~~~text
ExpertTranscriptEntry::Capability {
  call_id,
  capability_id,
  input,
  observation
}
~~~

Update:

- validation bounds;
- provider wire rendering used by ExpertModelHost;
- replay tests;
- Schedule private host tests while it still exists;
- any fixtures constructing ExpertTranscriptEntry directly.

The Expert model should be able to answer naturally after NeedsUserAction. It must not receive the permission mutation command itself.

## 5. Runtime soft-failure behavior

At crates/runtime/agent/src/engine.rs:633, retain the existing successful pattern where expected capability barriers become model-visible ToolResult observations instead of aborting the root Engine.

However, do not simply add AccessReviewRequired to the existing soft-failure match and call the work complete. That error has no target identity.

The checkpoint change should instead make ToolPort capable of returning a ToolResult whose artifacts include UserInteractionRef and whose issue/status states that source evidence was not obtained.

Requirements:

- ToolResult with interaction artifact has DependencyCoverage::Unknown or Independent according to whether it contains source data. It must never claim dependent source coverage without a successful read.
- model wire renders status=error or an equivalent bounded non-success state plus a user-action semantic summary;
- the Manager loop continues;
- the Tool call is journaled and replayable;
- the interaction is not auto-resolved by replay.

Keep existing soft handling for capability/policy barriers until all relevant tools have migrated. Mark the blanket legacy error mapping as deletion work for checkpoint 06.

## 6. Conversation carrier preparation

At crates/modules/conversation/src/turn/session.rs:124 and :212, reserve the final conversation representation for interaction references.

Preferred direction:

~~~text
AgentMessage::Interaction {
  turn_id,
  interaction_id,
  kind,
  status
}
~~~

This checkpoint may add the enum variant and protocol validation without producing it in production yet. Production emission is checkpoint 05.

Why a first-class message is preferred over hiding the interaction only inside a Capability expansion:

- delegated Expert and direct Manager Tool paths can converge on one presentation primitive;
- the user-facing request survives after technical capability details are collapsed;
- voice/other presentation surfaces can observe the same message;
- Flutter does not need to parse domain result JSON to discover required action.

If introducing the variant now forces broad unrelated wire churn, the checkpoint may instead add the lower-level artifact/reference contract first and defer AgentMessage::Interaction to checkpoint 05. Record that deferral explicitly; do not introduce two permanent interaction representations.

Implementation deferral: `AgentMessage::Interaction` and its protocol mirror remain checkpoint 05 work. Checkpoint 01 carries only `UserInteractionRef` in Tool/Task artifacts and Expert capability observations, avoiding a second message representation before Conversation owns durable interaction identity and emission.

## 7. Persistence boundary decision

Do not add a new business module merely to hold one reference if Conversation can own the lifecycle without dependency inversion.

For the target design in this plan:

- Conversation owns assistant-triggered interaction identity, origin and pending/resolved lifecycle.
- Access/Connections/Actions remain the semantic owners of the underlying mutation.
- the interaction record stores a typed target reference and requested operation, never copied credentials or grant state;
- resolution calls the current owner and then records the decision/outcome.

Checkpoint 01 should add only the contract types needed for this final shape. Actual Vault persistence and commands are checkpoint 05.

If implementation proves Conversation cannot own the lifecycle without a forbidden dependency, stop and update this plan before creating a new Interactions module. Do not hide the inversion behind an App-only store.

## 8. Files expected to change

Expected core files:

- crates/contracts/agent/src/expert_model.rs
- crates/contracts/agent/src/model.rs or the file currently owning ToolResult/OutcomeIssue
- crates/contracts/agent/src/lib.rs
- crates/contracts/context/src/lib.rs plus one focused source-access contract file
- crates/experts/builtin/src/host.rs
- crates/modules/experts/src/dispatch.rs
- crates/runtime/agent/src/engine.rs
- crates/app/src/vault_host/conversation_turn/expert_host.rs
- provider model wire files that serialize Expert transcripts / Tool results
- focused contract/runtime/Experts tests

Possible but not mandatory in this checkpoint:

- crates/modules/conversation/src/turn/session.rs
- protocol DTO mirrors for the future interaction message

Do not touch Flutter in checkpoint 01 except generated/compile-required contract fallout.

## 9. Tests to add before checkpoint exit

### Contract tests

- SourceReadOutcome validates bounded requirement identity.
- requirement rejects empty/oversized source/connection/consumer/resource identifiers.
- interaction ref rejects nil identity and unknown kind/status.
- serialization contains no token/bearer/credential/secret fields.

### Runtime tests

- Manager Tool returns NeedsUserAction observation -> Engine performs a subsequent model iteration.
- Manager can answer after the failed source read.
- Tool journal contains one intent/result pair.
- replay does not dispatch the source twice.
- Tool interaction artifact carries no source DependencyCoverage.

### Expert tests

- Expert reasoning transcript can contain a failed capability observation.
- the Expert model can answer after the failed observation.
- malformed failed observations are rejected.
- existing successful Schedule reasoning tests remain green before Schedule is moved.

### Settlement/artifact tests

- generic built-in output with auxiliary artifact reaches the Task.
- Task -> ExpertReport preserves the artifact.
- primary result remains unique and required.
- optional settlement round-trips through common path.

## 10. Residual audit

Search at checkpoint end:

~~~text
ExpertTranscriptEntry::Capability
BuiltinExpertOutput
completed_expert_task(
expert_report(
settlement: None
AccessReviewRequired
ConsentRequired
CapabilityDenied
~~~

Every AccessReviewRequired/ConsentRequired hit in a source read should be classified as:

1. hard/legacy code intentionally deferred to checkpoints 02–04;
2. test explicitly asserting old behavior before cutover;
3. unrelated model/provider consent;
4. bug.

Checkpoint 01 residual classification: `AccessReviewRequired` from the current Schedule App endpoint, Access personal/native Calendar reads, and Context personal/remote source reads is legacy source-denial behavior to replace in checkpoints 02–04. Their current tests continue to assert the old path until cutover. Model-dispatch recipient consent and hard identity/integrity denials remain separate; no AgentFailure-name-to-interaction mapping was added to Agent Runtime.

Do not claim source interaction foundation complete if expected source denials still have no typed target available to the caller that owns that source.

## 11. Checkpoint exit criteria

Checkpoint 01 is complete when:

- provider-neutral source read outcome contract exists;
- user-action requirement contains stable source/connection/consumer identity but no UI route;
- generic Expert output can carry auxiliary interaction artifacts and optional generic settlement;
- Expert reasoning can observe a failed capability without throwing;
- Manager Tool path can continue after a typed user-action-required ToolResult;
- no security check has been converted into a user prompt merely by matching AgentFailure;
- targeted contract/runtime/Experts tests pass;
- workspace check and architecture boundary check pass;
- Schedule still works through its old endpoint, but all contracts required to remove that endpoint now exist.

The next checkpoint must consume these contracts immediately. Do not leave them as unused speculative abstractions.
