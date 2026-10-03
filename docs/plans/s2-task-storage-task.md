Historical S2 task handoff/inventory, retained for provenance. Its implementation is complete; current source and the central execution status govern remaining work. Do not restart the handoff from this file.

# Implement canonical Vault Task storage

Implementation handoff for the saved-cloud worker. Frozen owner-contract checkpoint: `735a8b4f`. The coordinator will publish the shared bundle before starting this task. Work on the coordinator-provided branch and source snapshot; do not fetch an unrelated baseline or preserve the obsolete Task API for compatibility.

## Scope and workflow

Implement production code only in:

- `crates/adapters/vault/src/repositories/task.rs`
- `crates/adapters/vault/src/vault/tasks.rs`

Read root AGENTS.md, the relevant `.agents/skills/architecture-change/SKILL.md`, the active S2 plan and the frozen owner files listed below. This is an implementation task against fixed contracts, not a request to redesign ownership. Root exports, module declarations, manifest changes and adjacent registry visibility/deletion changes belong to the coordinator. Send a precise proposed diff for those seams; do not edit them yourself. Other owners are editing Conversation, Actions and Experts concurrently.

Do not run formatter, compiler, analyzer, tests, builds, repository checkers, lockfile resolution or live operations. Add no tests. Do not touch user databases, keys, credentials, caches or provider data. No reset, migration chain, fallback database, publication or commit is authorized by this handoff. Read source and implement the bounded storage changes; the coordinator owns the whole-S2 G2 gate and checkpoint.

Report useful results right away, before scratch notes. If work remains, send a short progress message; otherwise return the result. Keep the report short and complete the implementation at the necessary depth. If authorization is missing or approval review denies an action for that reason, pause and report the exact action, target and blocker. If the coordinator supplies transcript evidence authorizing that action and target, proceed; if the call was denied already, retry that same call once without adding evidence to tool arguments. Stop and report a repeated denial.

## Fixed owner contracts

Read these exact sources from the published `735a8b4f` bundle:

- `crates/modules/experts/src/domain/task_record.rs`
- `crates/modules/experts/src/ports/task_repository.rs`
- `crates/contracts/agent/src/task_execution.rs`, `delegation.rs`, `ports.rs`, `endpoint.rs`
- `crates/runtime/agent/src/journal.rs`
- `docs/plans/2026-10-02-s2-engine-owner-contracts.md`

Use `floe_experts::TaskRecord` as the only Task record. Delete the duplicate `VaultTaskRecord` struct, its independent validators/state machine and `to_vault_record`/`from_vault_record` conversions. Use canonical `TaskAdmission` and `TaskActivation` directly; ask the coordinator to remove obsolete Vault exports and update adjacent imports. Do not keep aliases or a second stored representation merely to compile old callers.

TaskRecord adds the immutable `execution_id`, `device_id`, `catalog_revision`, `model_allowance` and optional terminal `receipt` to the existing snapshot/admission/selection/invocation/request digest/revision/executor-generation fields. `catalog_revision` equals the admitted selected definition revision. The Task execution key is `{task_id, execution_id, executor_generation}`. Executor generation on the record belongs to the original admission and never changes during recovery.

Implement the complete frozen TaskRepository trait:

- `activate() -> BoxFuture<Result<TaskActivation, AgentFailure>>`
- `admit(TaskRecord) -> BoxFuture<Result<TaskAdmission, AgentFailure>>`
- existing `compare_and_swap(task_id, expected_aggregate_revision, executor_generation, snapshot) -> TaskRecord`, now only Submitted→Working
- `journal(TaskExecutionKey) -> Result<Arc<dyn ExecutionJournal>, AgentFailure>`
- `load_journal(TaskExecutionKey) -> BoxFuture<Result<Vec<JournalEntry>, AgentFailure>>`
- `read_execution_receipt(TaskExecutionReceiptRef) -> BoxFuture<Result<TaskExecutionReceipt, AgentFailure>>`
- `validate_settlement(&EndpointSettlement) -> Result<(), AgentFailure>`
- `settle_execution(TaskExecutionCommit) -> BoxFuture<Result<TaskExecutionReceipt, AgentFailure>>`
- `get(TaskId) -> BoxFuture<Result<Option<TaskRecord>, AgentFailure>>`

`TaskExecutionCommit` is exactly `{execution, expected_task_revision, expected_journal_revision, terminal, settlement}`. `settlement` is the existing optional EndpointSettlement, not a callback. Replace the old settle method rather than forwarding it through the new path.

Use these owner helpers exactly:

- `TaskRecord::validate(maximum_bytes)` and `validate_initial(maximum_bytes)`
- `TaskRecord::execution()` and `journal_binding()`
- `TaskRecord::transition(expected_aggregate_revision, executor_generation, snapshot, maximum_bytes)`
- `settle_task_execution(&TaskRecord, &TaskExecutionCommit, &[JournalEntry], maximum_bytes) -> Result<TaskRecord, AgentFailure>`
- `interrupt_task_execution(&TaskRecord, replacement_generation, &[JournalEntry], maximum_bytes) -> Result<Option<TaskRecord>, AgentFailure>`
- `validate_task_artifact(&TaskRecord, &TaskExecutionReceiptRef, artifact_id, &OwnerActor) -> TaskArtifactEvidence`

Use `floe_agent_contract::MAX_OUTPUT_BYTES` for the owner snapshot bound. Do not reproduce receipt hashing, coverage policy or Task transition logic inside Vault. The pure settlement helper binds the exact canonical output, journal digest/revision, actual accounting, admitted selection and Task identity into the immutable receipt. It validates typed blockage and rejects subdelegation/finalization in these Tasks.

## Encrypted storage and journal appends

Keep the Task and its bounded journal in the existing encrypted Person Vault. Store the canonical receipt inside TaskRecord; do not create a second receipt table, plaintext journal or model-accounting ledger. A physical per-Task journal table is expected for the canonical ExecutionJournal entries. Each persisted entry binds the exact Task execution key and monotonically contiguous revision. Validate row metadata against the decoded canonical value; a payload filter must not turn an oversized/corrupt existing row into NotFound.

The journal adapter owns an Arc to the actual Vault plus its immutable TaskExecutionKey. It accepts no parent Conversation journal or caller-supplied replacement binding. Every append reopens the exact stored Task in one short immediate transaction and verifies:

1. Vault access is still valid; Person and Task execution key match.
2. Task state is Working and the Task's original generation equals both the durable active executor fence and this Vault instance's active fence.
3. Existing entry sequence and SQL metadata are exact; append revision is the next contiguous revision.
4. The method accepts only its canonical JournalEvent category, then the candidate sequence passes the shared `project_execution_journal(&record.journal_binding()?, entries, JournalProjectionMode::DurablePrefix)` and `validate_journal_capacity` checks.
5. Reject Task subdelegation/finalization entries, ModelIntent whose parent_task_id is not this Task, and reservation ceilings above the stored whole-Task allowance. These are storage enforcement of the admitted owner contract, not another model policy.

Commit the entry before returning `JournalAck::Accepted { revision }`. There is no acknowledged intent before persistence. The shared 512-entry capacity rule reserves a terminal result slot for each acknowledged model/tool intent. Keep explicit encoded-byte/row limits and strict decoding; never silently truncate. Admission and encoded-record limits must reserve sufficient terminal receipt/accounting capacity so an already acknowledged operation cannot become unrecordable solely because the new receipt duplicates its snapshot. Report any exact storage-limit adjustment required by the canonical envelope to the coordinator; do not silently weaken a bound or discard acknowledged accounting.

Completed and Blocked Tasks reject all further appends, as do failed/cancelled/timed-out/interrupted Tasks. Historical receipt/journal reads remain possible under valid Vault access even when their original generation is older than the active executor. Do not use current-generation equality to erase historical evidence.

## Admission, executor fencing and recovery

Preserve exact Task ID/invocation-key uniqueness and full admission comparison. Same identity/request/selection replay returns the actual existing record. Changed payload or selection conflicts. A replay cannot replace execution ID, admitted device or allowance on an existing record.

Activation advances the existing durable global Task executor generation under one immediate transaction. Before interrupting old records, the old executor must be fenced against further appends/settlement; no provider/model callback runs in that transaction. For each bounded Submitted/Working orphan, load its actual journal and call `interrupt_task_execution` with the new global generation. Persist the returned Interrupted Task and its receipt atomically with the generation change. Keep every orphan's original execution key unchanged. Update the instance AtomicU64 only after successful commit. Coordinator owns root field declarations; there must be exactly one `task_executor_generation` field.

Submitted recovery requires an empty journal. Working recovery preserves all known and estimated model accounting, including unresolved acknowledged intent ceilings. No endpoint/model/tool call, private-state replay or fabricated zero-charge receipt occurs during recovery. A missing or corrupt journal is not proof of no handoff. Propagate failure and retain records.

Ordinary compare-and-swap calls only the owner `transition` helper, which permits Submitted→Working. Terminal writes use the settlement path exclusively. Old code paths that independently transition to terminal must disappear in the same cutover.

## One atomic Task settlement

`settle_execution` begins one short immediate transaction, loads Task plus its journal and verifies the current active fence for new settlement. Check the exact expected Task/journal revisions and call `settle_task_execution`. The resulting TaskRecord and receipt are the only new terminal values.

For `settlement: Some`, decode the existing ExpertSettlement with the admitted Task's agent ID and preserve the current exact checks for registry instance, installation, package, assignment, definition revision, invocation identity, expected private-state revision, incremented completed-invocation count, result and dependency coverage. Revalidate live dependency coverage using the existing same-transaction Vault primitive. Write assignment-local private state, its registry CAS, terminal Task and immutable Task receipt in this transaction. No callback, provider read, network call, model call or await outside database operations belongs here.

An identical terminal replay returns the stored immutable receipt without applying private state again, including after a lost acknowledgement. Changed terminal/journal/execution evidence conflicts. Once a Task is terminal, the old admission generation need not equal the newer active generation merely to read/rejoin its exact historical receipt. Never reopen it or alter parent Run identity.

The old `vault/registry.rs::settle_expert_task_checked` is an alternate terminal path and must be removed by the coordinator. Do not call it from the new adapter or keep a compatibility wrapper.

## Shared same-transaction receipt read

Provide in `vault/tasks.rs`:

```rust
pub(super) async fn read_execution_receipt_on(
    &self,
    connection: &turso::Connection,
    reference: &floe_agent_contract::TaskExecutionReceiptRef,
) -> Result<floe_agent_contract::TaskExecutionReceipt, AgentFailure>
```

The caller can pass an existing Transaction through the same connection convention used by `task_on`. The helper starts no independent transaction and writes nothing. It checks Vault access, the Vault Person against stored Task principal, exact Task/execution/reference, canonical record validation, exact persisted journal and accounting, and terminal evidence using the pure settlement helper in identical-replay mode (`expected_task_revision = aggregate_revision - 1`, terminal = stored snapshot, settlement = None). Return the actual stored immutable receipt only after all comparisons. Do not require the original execution generation to equal the current global generation for this historical read.

Keep `task_on(connection, task_id) -> Option<TaskRecord>` available to sibling Vault modules. Conversation will call the receipt helper inside its blocked-publication transaction and compare it to the acknowledged parent DelegationResult, then enforce Person/device/session/original-parent/continuation and blockage linkage. Actions will use this helper plus the canonical TaskRecord in its transaction and call `validate_task_artifact`; it owns proposal/effect interpretation. Neither caller may trust an artifact or caller-provided receipt by shape alone. No Task→Conversation or Task→Actions dependency is introduced.

## Coordinator-applied adjacent changes

Return exact small diffs, without applying them, for:

- removing `VaultTaskRecord`, `VaultTaskAdmission`, `VaultTaskActivation` exports/imports in `vault.rs`, crate roots and old consumers; replace them with the canonical Experts types;
- deleting `vault/registry.rs::settle_expert_task_checked` and its now-unused imports/old completion surface;
- making the existing `registry_payload` and `update_registry` methods `pub(super)` so the new Task transaction can reuse them, while retaining `registry_on` and the existing transaction/access/coverage helpers;
- any necessary manifest or root module reference. Do not duplicate registry serialization/SQL policy to avoid requesting this bounded seam.

Final report: changed files, exact repository/journal/transaction methods, obsolete paths removed, requested coordinator diffs, remaining concrete blockers, and explicit confirmation that no checks/tests/format/build/live operations were run. The coordinator will integrate and verify the whole source snapshot at G2.
