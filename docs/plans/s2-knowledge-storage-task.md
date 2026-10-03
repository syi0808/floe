# S2 Knowledge storage implementation

Implement the fixed owner ports against the encrypted Vault. This is mechanical storage work; do not redesign owner policy. Read root AGENTS.md and relevant local architecture skills, the accepted architecture/canonical/Vault/S2 Engine owner plans, and all files named below. No compiler, formatter, tests, checker, builds, dependency resolution, new tests, credential/user-data/cache changes, commits or publication. Return complete changed files plus a patch and a concise static review. Other workers own all shared module/export files.

## Exact exclusive paths

- crates/adapters/vault/src/vault/learning.rs
- crates/adapters/vault/src/repositories/knowledge.rs (new)
- crates/adapters/vault/src/repositories/learner_journal.rs
- crates/adapters/vault/src/repositories/memory_review.rs (delete)

Do not edit Vault mod.rs/lib.rs/engine.rs/repositories/mod.rs, Conversation, Context, Knowledge, manifests or tests. Report the exact module/export additions for the coordinator. No other file edits.

## Fixed constructors and ports

Export VaultKnowledgeRepository<Keys>::new(vault:Arc<EncryptedAgentVault<Keys>>,actor:OwnerActor)->Result<Self,AgentFailure>. It implements the object-safe KnowledgeRepository, LearnerJobRepository and LearnerEvidenceRepository in floe_knowledge. Export VaultLearnerJournalFactory<Keys>::new(vault:Arc<EncryptedAgentVault<Keys>>,actor:OwnerActor)->Result<Self,AgentFailure>, implementing LearnerJournalFactory. Inspect current port files; they are authoritative, no legacy wrappers. Both own real Arc handles. Validate captured actor and Vault Person/device at construction and exact supplied actor equality (including runtime epoch) on requests; check Vault access before/after physical work. Scope methods use actual ExecutionScope cancellation/deadline. Durable settlement/journal acknowledgements remain callable after execution cancellation but before Vault sealing.

Read owner helpers in Knowledge application/{storage_policy,memory,review,learner,learner_journal,discovery}. Pure semantics belong there. Delete the legacy direct trait implementations and discovery/model policy in learning.rs and memory_review.rs. Physical encrypted methods can remain pub(crate)/pub(super) for adapters and historical evidence readers, but remove obsolete public App workflows. Keep real existing schema creation and evidence/row decoding; never create a second model journal or guessed empty evidence. The owner is now the only caller.

## Knowledge transactions

Use one short encrypted transaction for stage and for each decision, with no provider/model I/O inside. Keep actual stored Conversation evidence checks and current independent provenance admission.

Stage:
- Validate MemoryStageRequest actor and origin. User requires request.actor == KnowledgeActor::User. Learner requires exact Running job and claim on this device, exact stored LearnerJournalHead and full canonical journal, matching MemoryStageOrigin revision/digest, then call validate_learner_stage(actor,job,journal,revision,digest,request). This proves the exact candidate is the already-validated journal Output; caller JSON is not authority.
- Call memory_stage_identity(person,request) using immutable request identity to obtain observation hash, candidate key and exact evidence refs before fresh mutable source/session observation. The helper uses the required Completed outcome in this key; the stored replay provides its actual prior admission.
- Exact stored candidate-key replay returns the original candidate before rechecking mutable target revision. Still validate stored ownership/key/observation/source refs/version identity. Preserve first acknowledged proposal for this observation/extractor/prompt/target identity, never create another candidate or autoapprove. Do not turn an unacknowledged changed proposal into a mutation.
- For a new candidate, acquire LearningEvidenceSnapshot through existing TransactionLearningEvidence, read matching observation and actual active target revision when needed, call plan_memory_stage, and insert its exact observation (if absent) and candidate atomically. All hash/content/operation/transition choices come from this owner helper. Preserve existing stored observation IDs on replay.

Read context:
- Read actual active memory revisions plus authoritative evidence independence in one consistent physical read/transaction. Produce MemoryContextFact for each and call project_memory_context(person,facts,now). This owner helper handles current validity intervals, provenance and bounds. Propagate errors; no fabricated empty success for unavailable storage.
- overview validates requested limit and calls project_memory_summary on each actual revision. review validates each pending memory candidate with validate_memory_review_candidate. Both are bounded; read limit+1 where a full collection is required and return BudgetExceeded on overflow (review maximum 100); overview's explicit limit intentionally returns that page alongside exact counts.

Decide:
- Persist immutable command receipt for MemoryDecisionRequest keyed command_id, bounded 4096 per store/generation-independent history. Immutable identity is Person/device/candidate_id/decision kind; decided_at is owner observation time, so replay retains the original stored time rather than requiring a later call's clock to equal it.
- Check command replay before mutation/current-candidate checks. Exact same command returns exact original KnowledgeDecisionResult; changed identity conflicts. Cross reuse among Knowledge command receipt kinds conflicts. No replay may approve a different candidate.
- Use existing actual ReviewAdmission facts, then plan_memory_review(candidate,actor.person_id,kind,decided_at,admission). This owner helper validates a pending Memory candidate, so product memory commands cannot mutate Playbook candidates. Apply exact returned candidate/decision/revision/mutation/superseded values atomically. Only User approval can create an active memory revision. Learner stages Pending only.

## Learner jobs and discovery

LearnerJobRepository now receives actor/scope. claim_review also receives the owner's LearnerBudget. Jobs retain typed Blocked with full LearnerProjectionBlock and claimed_device_id; no generic failure collapse. Preserve G1 LearningOutcome::Blocked exactly.

- discovery_sessions(actor,limit,scope) returns at most the requested 1..64 recent LearningSessionSnapshot values from actual stored sessions. Storage does not decide learning signals or create jobs. For each Assistant, resolve its actual admitted Conversation RunRecord and copy that Run's original_user_message_id into LearningTranscriptMessage::Assistant. A linked resumed answer must point to the original User identity; never infer linkage by copied text or assume the Assistant turn equals the User turn. User snapshots carry their actual stable message_id and turn_id. Evidence snapshot is read through the existing stored coverage authority, for the actual bounded source turns. Knowledge explicit_review_input performs eligibility and signal selection.
- enqueue validates actual source evidence/current session revision in the transaction, obtains learner_job_key for replay, and calls new_learner_job for a new row. Preserve exact immutable input and idempotency; no new observation snapshot on replay. Limit job envelopes to 128 KiB.
- Fresh claims are only Queued/Deferred and available. Use claim_learner_job(lifecycle,now,actor.device_id), job.apply_lifecycle(), and LearnerJournalHead::new(claimed_job,budget). Create head and claimed row atomically. This head pins the allowance; a new configuration cannot rewrite an old claim.
- Running lease recovery never increments attempts. Read the exact head and complete current-claim journal, then call recover_learner_claim(job,journal,actor.device_id,now). An empty journal or acknowledged known Output can retain the same claim/device/budget with an extended lease. Any nonempty incomplete or unresolved/unknown execution becomes Failed Interrupted. Apply the returned lifecycle atomically before returning either the retained claim or no work. Missing/corrupt/truncated journal is StorageUnavailable, never proof of no dispatch.
- Preserve exact claim source validation. On legitimate source stale/notfound/policy-denied use reject_learner_claim; no claim replacement after uncertainty. Other unavailable errors propagate.
- settle_review verifies exact Running claim/device, loads the optional actual candidate named by Completed, and calls validate_learner_settlement(job,journal,&settlement,candidate.as_ref()) for pure terminal/journal policy. LearnerProjectionBlock::validate recomputes its exact prepared-plan review digest. Completed with candidate_id proves stored candidate belongs to this exact job/input and canonical Output via validate_learner_stage; Completed without candidate requires an acknowledged Output parsing to proposal=None. Blocked requires valid actual plan Person/device/purpose/consumer and exact review digest SHA256 serde tuple(plan,projection_operation_id,blockers), with no unresolved/unknown attempts. Deferred never allowed with acknowledged Output, unresolved attempt or unknown usage; preserve actual known accounting. Failed preserves all journal/head evidence. Use settle_learner_job and apply_lifecycle. Persist immutable settlement replay so lost acknowledgements rejoin exact same settlement (including Blocked) and changed settlement conflicts; terminal rows cannot be overwritten. No automatic retry/reissue of model intent.
- LearnerEvidenceRepository::read_claim verifies actual Running claim/person/device, scope.root_run_id==job_id, actual immutable input and current source/evidence revision. Return the stored input only, with no caller prompt or selector. Check the same claim again on the actual reads; Context rechecks before publishing its projection.

## Sole canonical journal with an authenticated head

Keep learner_execution_journal as sole JournalEvent storage. Add a private per-claim head table keyed (job_id,claim_attempt), storing full LearnerJournalHead and Person/device. No duplicate model attempt ledger, inference journal or synthesized receipt.

Current schema uses sequence globally across a job; change new storage to per-claim sequence and event key uniqueness. Prelaunch schema is new, so no legacy compatibility path/migration fallback. Preserve actual stored code evidence, never run database migration against user data.

Append in one immediate transaction:
1. Load exact Running job, claimed device and current claim head; authenticate all previous current-claim entries with validate_learner_journal.
2. Enforce event method families (ModelIntent, ModelResult, Output, checkpoint/ValidatedBatch/BatchProgress); same event key with exact byte/value equality rejoins its original revision. Changed payload conflicts. Include claim identity in every physical key. Use stable keys (attempt ID, execution/batch/cursor identity, or exact canonical content digest).
3. Build one contiguous next JournalEntry and call advance_learner_journal(head,entire_next_prefix). The helper validates shared Engine ordering, role/catalog identity, no tools/delegations, cumulative pre-dispatch allowances, output parsing and bounds/reserved result capacity. Actual provider overrun or unknown terminal ModelResult remains recorded conservatively; never reject it simply because charged usage exceeds the prior ceiling.
4. On ModelIntent revalidate actual source evidence and exact plan Person/device/purpose/consumer. Never await provider I/O under the transaction.
5. Atomically insert event and CAS-update exact head revision/digest before acknowledging. ModelResult acknowledgement releases in-memory receipt payload only after this durable acknowledgement.

load_journal returns LearnerClaimJournal {head,entries}; verify row keys and contiguous sequence, then validate_learner_journal. A missing tail, altered head, missing head or malformed event returns failure. A new claim with head revision0 and canonical empty digest is the sole positive empty-journal case. Reads can include terminal claims for audit; appending remains Running-only. load_journal must use exact captured Person/device and job/claim binding, no caller-selected alternate authority.

## Acceptance and delivery

Storage never chooses a model, interprets model output outside owner helper validation, invents Task/Run IDs, copies resumed user text, autoapproves memory, publishes Conversation reviews, or changes provider policy. No compatibility wrappers for removed MemoryContextReader/MemoryCandidateSink/MemoryReviewRepository/LearnerModel APIs.

Report useful results promptly, then complete the bounded implementation and return changed files/patch, required exports and any concrete unresolved invariant. Do not run checks; this batch is statically reviewed natively before coordinated G2.
