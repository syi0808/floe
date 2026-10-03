# S2 Task Engine journals, trusted blockage and Learner projection

Status: proposal for coordinator review, 2026-10-02. This document does not authorize S2 implementation before G1 passes. It describes one recommended design, not an additional runtime or compatibility path.

This closes three concrete gaps in the accepted [canonical contracts](2026-10-02-canonical-contracts.md), [Rust cutover](2026-10-02-rust-cutover.md) and [Vault cutover](2026-10-02-vault-cutover.md). S1 remains the baseline: the shared planner, prepared object, scope ledger and acknowledged ModelIntent/ModelResult already work for every caller. S2 changes ownership and execution structure, not provider selection, source processing policy or retry permission.

## 1. Task owns its complete Engine journal

The Expert Engine must not write its ValidatedBatch, ToolIntent, cursor or Output into the parent Manager journal. S1's parent journal correctly rejects binding a nested Task's model attempt to a Manager batch. Keep that rejection. Give each admitted Task one canonical ExecutionJournal, physically stored beside its Task record and fenced by its admitted execution and executor generation.

Proposed pure agent-contract values:

```rust
struct TaskExecutionKey {
    task_id: TaskId,
    execution_id: Uuid,
    executor_generation: u64,
}

struct TaskExecutionReceiptRef {
    execution: TaskExecutionKey,
    task_revision: u64,
    journal_revision: u64,
    digest: [u8; 32],
}

struct TaskModelAccounting {
    usage: floe_execution::budget::ModelUsage,
    attempt_refs: Vec<Uuid>,
    unresolved_attempts: Vec<UnresolvedModelAttempt>,
}

struct TaskExecutionReceipt {
    reference: TaskExecutionReceiptRef,
    journal_digest: [u8; 32],
    snapshot: TaskSnapshot,
    accounting: TaskModelAccounting,
}

struct TaskExecutionCommit {
    execution: TaskExecutionKey,
    expected_task_revision: u64,
    expected_journal_revision: u64,
    terminal: TaskSnapshot,
    settlement: Option<EndpointSettlement>,
}

enum TaskExecutionEvidence {
    Unadmitted,
    Admitted(TaskExecutionReceipt),
}
```

Move the existing pure UnresolvedModelAttempt shape from Conversation to agent-contract and re-export that exact type. It retains attempt ID, acknowledged reservation ceiling and unknown flags. Do not introduce another model-attempt table, charge ledger, optional Inference recorder or provider usage estimate.

TaskRecord gains an execution_id assigned during durable Task admission and an immutable terminal execution receipt. Working state has no terminal receipt; terminal admitted state requires one. The receipt digest covers a domain/version tag, the execution key, admitted invocation/request digest, admission/selection identity, exact terminal TaskSnapshot, last journal revision/digest and accounting projection. It is nonsecret integrity evidence, not an authorization capability. The Task owner and repository compare it to the real stored record before use.

TaskReceipt remains the canonical delegation return value and gains TaskExecutionEvidence. Its snapshot must equal the admitted execution receipt's snapshot. Unadmitted is permitted only for a genuine pre-dispatch Rejected delegation that never reached Task admission; it cannot carry attempts or charged usage. The parent validates the exact DelegationIntent and the absence of an admitted Task before accepting that case. A Working Task is queried as a Task snapshot, not returned to Engine as a terminal TaskReceipt. This distinguishes real admission phase variation without a compatibility fallback.

Extend the Experts-owned TaskRepository with these capabilities:

```rust
fn journal(&self, execution: TaskExecutionKey)
    -> Result<Arc<dyn ExecutionJournal>, AgentFailure>;

fn load_journal<'a>(&'a self, execution: TaskExecutionKey)
    -> BoxFuture<'a, Result<Vec<JournalEntry>, AgentFailure>>;

fn read_execution_receipt<'a>(&'a self, reference: TaskExecutionReceiptRef)
    -> BoxFuture<'a, Result<TaskExecutionReceipt, AgentFailure>>;

fn settle_execution<'a>(&'a self, commit: TaskExecutionCommit)
    -> BoxFuture<'a, Result<TaskExecutionReceipt, AgentFailure>>;
```

TaskExecutionCommit contains the exact execution key, expected Task and journal revisions, terminal snapshot and optional existing EndpointSettlement. Storage recomputes/compares the canonical journal projection and commits the terminal Task, assignment-local private state when present, and immutable receipt in the same encrypted transaction. Existing Task admission and private-state CAS remain authoritative. The old settle path is replaced, not retained as an alternate way to terminalize a Task without journal proof.

### Execution identity and validation

Add required execution_id to EngineRequest. Engine uses this admitted value instead of choosing an unobservable ID internally. TaskCoordinator persists the value before Engine starts; Learner derives it from the admitted job/claim; Conversation supplies its admitted work segment ID. A continuation must use the execution ID of its exact stored pending batch. Finalization supplies a different admitted ID under the existing FinalizationStarted transition. No optional legacy default remains.

Move the pure JournalEntry shape to agent-contract. Extract the role-neutral event-order/validated-batch/accounting validator from Conversation recovery into agent-runtime. Its explicit JournalExecutionBinding contains principal, device_id, execution_id, catalog_revision, root_run_id and owning_task_id. Conversation retains its own cross-Run continuation and terminal policy around that projection; Experts retains Task admission and terminal policy. There must not be two competing validators for model attempt ordering or batch/cursor identity.

A Task journal accepts ModelIntent.parent_task_id only when it equals its exact admitted Task ID. A root Conversation journal accepts None after S2; S1's direct nested ModelIntent/Result route is removed in the same slice. Each Task ValidatedBatch belongs to its admitted execution. Shipped and currently supplied Expert packages retain their existing no-subdelegation behavior: their Engine catalog contains declared tools and no agent cards. Do not add a new Task hierarchy while closing this refactor.

The journal bounds reserve a result slot for every acknowledged Model/Tool/Delegation intent. A capacity failure precedes new intent acknowledgment; it must not prevent storing a terminal receipt for an already admitted attempt. The S1 bounded root-ledger tombstones and exact unknown accounting remain unchanged.

### Parent accounting and crash ordering

The live Task receives a child of the parent's ExecutionScope, so Inference charges the same root BudgetLedger exactly once. Preserve one Task-wide allowance across all of its Engine iterations. Current Engine::execute_delegation derives that child allowance from max_attempt_tokens/max_attempt_cost_micros; common multi-step execution needs explicit owner-configured Task allowances distinct from a single model-attempt bound. These remain bounded by the parent lease and are fixed before Task admission; a package/model cannot select or reset them. Freeze those configuration fields in the shared lane without silently expanding the root budget. Task-owned ModelResult acknowledges the immutable scope receipt and releases its payload. The parent does not reserve, settle or charge those attempts again when it receives a Task receipt.

The parent's only durable copy is its canonical DelegationResult referring to the exact TaskExecutionReceiptRef and carrying the Task owner's checked accounting projection. That is a derived parent projection of a Task receipt, not another authoritative ModelResult. Parent recovery recomputes total charges as root ModelResults plus each distinct admitted Task execution receipt once. Repeated references to the same execution never add cost again; a changed digest for the same execution is corruption. Known and estimated token/cost amounts stay separate. Attempt references include the exact Task attempt IDs without inventing parent model attempts.

Manual validated-batch continuation must preserve the original immutable Task receipt, including its original parent Run. Current Engine::replay_resumed_task rewrites TaskSnapshot.parent_run_id for the observing child; remove that mutation. The child Run's physical journal supplies its own observing identity. Conversation validates adoption against the exact stored continuation chain, unchanged execution/batch/step identity, Task ID/invocation and original Task receipt reference. Extend the Task arm of ReplayReceipt with that exact TaskExecutionReceiptRef; a replay cannot substitute a different receipt or count the same execution twice. This adoption rule does not apply to source-review auto-resume, which creates a fresh execution and new Tasks.

Required order:

1. Parent durably acknowledges DelegationIntent with exact Task ID, invocation digest and immutable selection.
2. Task owner durably admits TaskExecutionKey before any Engine call. Every model/source/tool operation uses that Task scope and journal.
3. Task owner settles its journal and private state, then commits its immutable terminal receipt.
4. Parent authenticates/reloads that exact receipt and durably acknowledges DelegationResult before advancing its batch cursor or committing any parent terminal state.
5. Conversation publishes any blocked-review group and commits its own terminal state only after that real Task evidence is linked.

Recovery must activate/reconcile abandoned Tasks before terminalizing parents that reference them. After a crash between steps 3 and 4, load and journal the existing Task receipt; do not execute the endpoint again. After a crash before Task settlement, derive Interrupted and conservative unknown accounting from its journal, then commit the terminal receipt. Missing or inaccessible Task storage is not proof of no charge. A genuinely absent Task record proves zero Task handoff only when checked against the exact acknowledged parent intent and the invariant that no dispatch precedes Task admission. Otherwise retain the unresolved delegation and fail closed. No Task/Run continuation may reissue an unsettled model call.

## 2. Trusted blockage is a typed terminal outcome

Add TaskState::Blocked. It is necessary: a review stop is neither a completed answer nor an execution failure. A Blocked Task has no result text, no issue, and a nonempty trusted blockage; its coverage contains only actual settled observations. Completed/Failed/etc. cannot carry a blockage. Pre-model blockage has an empty attempt list and zero additional charge; earlier real work remains accounted.

Proposed pure agent-contract shapes, using SourceAccessBlockers from context-contract:

```rust
enum TaskBlockage {
    SourceRead {
        tool_call_id: Uuid,
        blockers: SourceAccessBlockers,
    },
    ModelProjection {
        plan: PreparedModelPlan,
        review: SourceProjectionReview,
    },
    Binding {
        requirement_keys: Vec<String>,
    },
}

struct ExpertBlockReport {
    task_id: TaskId,
    principal: String,
    agent_id: String,
    definition_revision: u64,
    coverage: DependencyCoverage,
    blockage: TaskBlockage,
}

enum ExpertExecutionOutcome {
    Completed(ExpertReport),
    Blocked(ExpertBlockReport),
}
```

AgentEndpoint::execute returns ExpertExecutionOutcome. TaskSnapshot gains blockage: Option<TaskBlockage>, present exactly for Blocked. This is a real closed state variation. Task owner verifies the report against its admission and journal before storing it in the terminal receipt. Binding requirement keys must be unique declared unsatisfied requirements in the pinned Task selection. No caller-supplied package, source, grant or candidate mapping becomes authority.

### Source tools must not turn blocker JSON into authority

Use a typed source-review outcome at the ToolPort boundary:

```rust
enum ToolInvocationOutcome {
    Completed(ToolResult),
    NeedsSourceReview {
        call_id: Uuid,
        blockers: SourceAccessBlockers,
    },
}
```

ToolPort::invoke returns this enum. Add JournalEvent::ToolReviewRequired { call_id, blockers } as the single terminal settlement of that acknowledged ToolIntent. It binds the real call ID and its declared source capability. It does not fabricate ToolResult text, an answer, a source observation, a model attempt or a charge. Journal validation treats it as a settled blocked read and rejects a second ToolResult/review settlement for the same call. It is never encoded as provider tool history or parsed from ArtifactPart::Data.

Engine's typed blockage outcome distinguishes model projection from source-tool review and already-settled blocked delegation. It carries the ordinary EngineReport of earlier settled work plus the typed blockage/evidence. Engine journals the ToolReviewRequired or real DelegationResult before returning that outcome, does not execute remaining batch steps, and does not call the model to explain the stop. Experts converts its own Engine blockage to ExpertBlockReport. A binding preflight block can occur before the Engine starts, under a real admitted Task with an empty journal.

The Task owner derives SourceRead blockage identity from the actual ToolReviewRequired event, ModelProjection identity from the exact plan/review returned by Engine, and binding identity from its admitted selection. Full immutable blockage is covered by TaskExecutionReceiptRef.digest. No content authored by a model can construct this evidence.

### Conversation publishes after authenticated Task linkage

Conversation consumes the authenticated Task receipt returned through its direct Experts owner dependency. It verifies principal, device, session, parent Run, invocation/selection and exact stored receipt digest, then requires the matching parent DelegationIntent and acknowledged DelegationResult. Replace the App publication callback; Experts never imports Conversation or writes Conversation rows.

Conversation's Task interaction origin gains the exact TaskExecutionReceiptRef, with an optional real tool call ID when the origin is a source read. Source review goes through Connections; binding review goes through Experts' review API. Task-specific binding review preparation accepts the receipt reference and declared requirement key, resolves the real stored Task/admission internally, and calls the same binding-review policy as explicit settings. It is not a second binding mutation path.

Generalize BlockedRunCommit's publication payload into a closed owner-audit union:

- ModelProjection audit retains PreparedModelPlan + SourceProjectionReview, as in S1.
- SourceRead audit retains Task receipt reference, actual tool call, SourceAccessBlockers and exact Access review references. It contains no fake model plan.
- ExpertBinding audit retains Task receipt reference and exact Experts BindingReview reference.
- Navigation-only source requirements retain the real Task/source requirement and safe destination; they carry no grant-mutation reference.

Replace the parallel RunBlockRecord origins/review_refs/interaction_refs arrays with bounded BlockedInteractionLink entries containing one real Conversation interaction ID, its authenticated origin, and a closed reviewed target reference (Source/ExpertBinding/Navigation). This permits non-model source and binding blocks without optional fictitious Access/model identity. Preserve prior_exhaustion. Manager projection-only blocks use the same shape with their real projection origin.

Connections/Experts prepare immutable reviews before the terminal transaction. Conversation then atomically stores the complete audit group, deterministic interaction rows, no-answer Blocked terminal and real settled coverage/accounting. Replay compares the whole immutable commit. If preparation fails, no fabricated Blocked success is returned. If a crash occurs after Task settlement or parent DelegationResult but before the group commit, recovery loads that same receipt and rejoins the same deterministic review operation/group; it neither reruns the Task nor reselects a model.

Whole-group resume semantics do not change: all cards terminal, at least one Resolved, an eligible terminal parent, exact session CAS, unique pending resume/child slot, original user-message reference and a fresh child Run. New user input supersedes the pending request. The child selects a new common model plan and current Expert environment; it does not replay the blocked Task Engine or reconstruct its prepared transport.

## 3. Learner projection preserves the Engine operation identity

Keep the canonical inward Knowledge-owned LearnerProjectionPort and Context implementation. Make its request sufficient to carry the exact Engine projection identity and immutable admitted job claim:

```rust
struct LearnerClaimRef {
    job_id: Uuid,
    claim_attempt: u8,
}

struct LearnerProjectionBounds {
    max_input_bytes: usize,
    max_output_bytes: usize,
}

struct LearnerProjectionRequest {
    actor: OwnerActor,
    claim: LearnerClaimRef,
    projection_operation_id: Uuid,
    plan: PreparedModelPlan,
    correction: Option<ModelCorrection>,
    evidence_refs: Vec<LearningEvidenceRef>,
    bounds: LearnerProjectionBounds,
    expires_at: DateTime<Utc>,
}
```

LearnerProjectionPort::project returns ModelProjectionOutcome through the existing BoxFuture/ExecutionScope convention. No public input can provide trusted prompt text or a replacement source grant. Context reads the exact admitted job/evidence snapshot through a Knowledge-defined read-only evidence repository capability supplied to ContextCore; it verifies the claim, Person, source session/revision, exact evidence set, evidence freshness and current source permission before building the bounded envelope. The candidate-only role/output contract and prompt are Knowledge-owned. The complete plan is checked against that claim's actor/purpose/consumer; its transport is never reconstructed.

Knowledge supplies a job-bound ModelProjectionPort adapter to Engine. The adapter copies projection_operation_id, plan and correction from Engine's ModelProjectionRequest into LearnerProjectionRequest. Context copies that operation ID into Ready/NeedsSourceReview and binds Ready to the same plan ID/digest. Correction changes only bounded corrective instruction, while Engine retains the same prepared call and uses fresh projection and attempt IDs. A failed dispatch or result acknowledgment is never a correction retry.

Knowledge's Engine request has no tools/delegation, a bounded candidate-only output validator and an explicit execution ID derived from the actual job/claim. Its ExecutionJournal is the existing encrypted job journal keyed by Person/job/claim, not the origin Conversation run. Keep acknowledged ModelResult/unknown usage and the S1 no-reissue recovery rule. Source blockage remains a typed job outcome with no candidate activation and no fabricated model intent. Knowledge owns any future product-facing review exposure; it must not append cards to a terminal origin Conversation.

## 4. Atomic implementation packages and dependencies

Land the pure contracts, role-neutral journal validation and manifest edges together after G1, before parallel implementations. Then these are nonoverlapping packages:

1. Native shared lane: agent-contract/runtime Engine types, generic journal projection, Conversation parent receipt accounting/block publication/recovery, and TaskRepository contract. This lane fixes journal bounds, digest rules and all crash invariants.
2. Experts owner lane: modules/experts execution/environment/binding services and repository ports. Preserve current Task and registry CAS; consume injected package values, CandidateCatalog, ModelPort and declared source ports. Delete A2A conversion and legacy Expert model vocabulary after all package callers move.
3. Context lane: Task source/projection adapters, ContextCandidateCatalog and ContextLearnerProjection over ContextCore. No Experts/Knowledge reverse imports or whole-service construction callback.
4. Package lanes: disjoint builtin domain folders implement the frozen role/tools/output contract and retain domain/evidence validators. Keep shared builtin host/registration files with the coordinator. Schedule and Actions proposal sealing need one coordinated lane.
5. Knowledge lane: owner Engine runner and read/use/review/discovery policy. Keep actual encrypted claim/settlement/journal implementation in a separate Vault storage lane.
6. Storage lane: Task execution journal/receipt, binding reviews and Knowledge records implement exact owner-supplied decisions in short transactions. No provider/model calls or policy copies in repositories.

Affected current files include agent-contract/{delegation.rs,ports.rs,expert_model.rs}; agent-runtime/engine.rs; experts/{task.rs,dispatch.rs,a2a.rs,manifest.rs,registry.rs,settlement.rs}; Vault repositories/task.rs and vault/{tasks.rs,registry.rs,learning.rs}; Conversation recovery/coordinator/storage/source-review projections; builtin/{host.rs,shared.rs,registration.rs,*/dispatch.rs,*/expert.rs}; App expert_setup, expert_binding_settings, Expert host/dispatch/stateful settlement and learner_worker. Exact new owner ports and lib exports are coordinator-owned so cloud packages cannot create alternate definitions.

Experts promotes agent-runtime to production and adds kernel; the old Inference dependency is already removed. Knowledge adds agent-runtime and does not import Context/Inference/Access. Context adds the Experts inward-port edge and already has Knowledge. Construct repositories/native/Gateway adapters, then Access/Connections and independent Inference, then ContextCore and its small adapters, then Experts/Knowledge, then complete ContextService, then Conversation. No new workspace crate, optional late initialization, service locator, Arc construction cycle or business-owner import of builtin packages.

No S2 implementation, tests or checks are authorized by this proposal alone. After coordinator review fixes these exact shapes, use bounded disjoint worker packages; perform the planned structural closure before G2/S3 checks and behavioral tests.


## S3 regression: configuration changes after Task admission

Exercise the actual TaskCoordinator and common Engine endpoint with an admitted source selection A. Pause before endpoint execution, replace the assignment binding with B, and disable the assignment or installation. The already admitted Task must retain A and its exact definition/admission; all actual reads use A, and none use B. With A's source/grant authority still valid, the Task may settle normally. A stateful settlement must preserve the newer B binding and disabled configuration while applying only its assignment-local private-state CAS.

Repeat with A revoked or its source authority changed before acquisition/dispatch. The live source fence must deny or return its typed source review; the Task must never fall through to B. An incompatible installed manifest/admission or private-state revision must still conflict. Replaying a committed receipt after rebind/disable must return the exact original execution, selection, parent identity and accounting without source/model I/O or another charge. No binding-equality comparison against current Registry state may serve as a Task authority check.

This is a behavioral regression obligation for the coordinated S3 stage; it records no test implementation or execution result.
