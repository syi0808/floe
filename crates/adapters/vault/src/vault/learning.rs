use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;

use chrono::{DateTime, Utc};
use floe_access::DependencyCoverage;
use floe_agent_contract::{
    AgentFailure, CommandId, DataClass, ExecutionScope, JournalEntry, JournalEvent, OwnerActor,
    RunId,
};
use floe_conversation::{AgentOutcome, AgentSession};
use floe_knowledge::{
    KnowledgeActor, KnowledgeCandidate, KnowledgeCandidateState, KnowledgeDecisionKind,
    KnowledgeDecisionResult, KnowledgeKind, KnowledgeOperation,
    KnowledgeRevision, KnowledgeRevisionState, LearnerBudget,
    LearnerClaimJournal, LearnerClaimRef, LearnerJobClaim, LearnerJobSettlement,
    LearnerJobState, LearnerJournalHead, LearnerReviewInput, LearnerReviewJob,
    MAX_LEARNER_JOB_ATTEMPTS,
    LearningEvidenceRef, LearningEvidenceSnapshot, LearningObservation, LearningOutcome,
    LearningSessionSnapshot, LearningTranscriptMessage, MemoryContextFact, MemoryDecisionRequest, MemoryOverviewSnapshot,
    MemoryReviewSnapshot, MemoryStageOrigin, MemoryStageReceipt, MemoryStageRequest, StageMemoryCandidate,
    admit_learning_evidence, advance_learner_journal, claim_learner_job, learner_job_key,
    memory_stage_identity, memory_stage_receipt, new_learner_job, plan_memory_review, plan_memory_stage,
    project_memory_context, project_memory_summary, recover_learner_claim,
    reject_learner_claim, settle_learner_job, validate_learner_budget,
    validate_learner_input, validate_learner_journal, validate_learner_settlement,
    validate_learner_stage,
    validate_memory_overview_limit, validate_memory_review_candidate, validate_memory_stage_replay, validate_review_actor,
    validate_review_candidate, validate_stage_request, EvidenceReader,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use turso::transaction::TransactionBehavior;
use uuid::Uuid;

use super::*;

const MAX_LEARNER_JOB_BYTES: usize = 128 * 1024;
const MAX_LEARNER_EVENT_BYTES: usize = 128 * 1024;
const MAX_KNOWLEDGE_COMMAND_RECEIPTS: i64 = 4096;
const MAX_KNOWLEDGE_STAGE_RECEIPTS: i64 = 4096;
const MAX_MEMORY_REVIEW_ITEMS: usize = 100;

const KNOWLEDGE_STORAGE_SCHEMA: &[(&str, &str)] = &[
    ("knowledge_store_schema", "CREATE TABLE knowledge_store_schema (id INTEGER PRIMARY KEY CHECK(id = 1), version INTEGER NOT NULL CHECK(version = 1))"),
    ("learning_observations", "CREATE TABLE learning_observations (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, content_hash TEXT NOT NULL, payload TEXT NOT NULL, UNIQUE(person_id, content_hash))"),
    ("knowledge_candidates", "CREATE TABLE knowledge_candidates (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, idempotency_key TEXT NOT NULL, kind TEXT NOT NULL, state TEXT NOT NULL, target_id TEXT, created_at TEXT NOT NULL, payload TEXT NOT NULL, stage_payload TEXT NOT NULL, UNIQUE(person_id, idempotency_key))"),
    ("knowledge_candidates_review", "CREATE INDEX knowledge_candidates_review ON knowledge_candidates(person_id, kind, state, created_at)"),
    ("knowledge_stage_receipts", "CREATE TABLE knowledge_stage_receipts (person_id TEXT NOT NULL, candidate_key TEXT NOT NULL, candidate_id TEXT NOT NULL UNIQUE, observation_id TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(person_id, candidate_key))"),
    ("knowledge_candidate_decisions", "CREATE TABLE knowledge_candidate_decisions (id TEXT PRIMARY KEY, candidate_id TEXT NOT NULL UNIQUE, payload TEXT NOT NULL)"),
    ("knowledge_revisions", "CREATE TABLE knowledge_revisions (target_id TEXT NOT NULL, revision INTEGER NOT NULL, person_id TEXT NOT NULL, kind TEXT NOT NULL, state TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(target_id, revision))"),
    ("knowledge_revisions_active", "CREATE INDEX knowledge_revisions_active ON knowledge_revisions(person_id, kind, state)"),
    ("knowledge_mutations", "CREATE TABLE knowledge_mutations (id TEXT PRIMARY KEY, candidate_id TEXT NOT NULL UNIQUE, target_id TEXT NOT NULL, created_at TEXT NOT NULL, payload TEXT NOT NULL)"),
    ("knowledge_command_receipts", "CREATE TABLE knowledge_command_receipts (command_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, device_id TEXT NOT NULL, command_kind TEXT NOT NULL, candidate_id TEXT NOT NULL, decision_kind TEXT NOT NULL, payload TEXT NOT NULL)"),
    ("learner_review_jobs", "CREATE TABLE learner_review_jobs (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, idempotency_key TEXT NOT NULL, state TEXT NOT NULL, attempts INTEGER NOT NULL, available_at TEXT NOT NULL, payload TEXT NOT NULL, UNIQUE(person_id, idempotency_key))"),
    ("learner_review_jobs_ready", "CREATE INDEX learner_review_jobs_ready ON learner_review_jobs(person_id, state, available_at)"),
    ("learner_execution_journal", "CREATE TABLE learner_execution_journal (job_id TEXT NOT NULL, person_id TEXT NOT NULL, claim_attempt INTEGER NOT NULL, sequence INTEGER NOT NULL, event_key TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(job_id, person_id, claim_attempt, sequence), UNIQUE(job_id, person_id, claim_attempt, event_key))"),
    ("learner_journal_heads", "CREATE TABLE learner_journal_heads (job_id TEXT NOT NULL, person_id TEXT NOT NULL, claim_attempt INTEGER NOT NULL, device_id TEXT NOT NULL, journal_revision INTEGER NOT NULL, journal_digest TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(job_id, person_id, claim_attempt))"),
    ("learner_settlement_receipts", "CREATE TABLE learner_settlement_receipts (job_id TEXT NOT NULL, person_id TEXT NOT NULL, claim_attempt INTEGER NOT NULL, device_id TEXT NOT NULL, settlement TEXT NOT NULL, result TEXT NOT NULL, PRIMARY KEY(job_id, person_id, claim_attempt))"),
];

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn initialize_learning_store(&self, create: bool) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        if create {
            let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)
                .await.map_err(storage)?;
            let result = async {
                for (_, sql) in KNOWLEDGE_STORAGE_SCHEMA {
                    transaction.execute(sql, ()).await.map_err(storage)?;
                }
                transaction.execute("INSERT INTO knowledge_store_schema (id, version) VALUES (1, 1)", ())
                    .await.map_err(storage)?;
                self.check_access()
            }.await;
            finish_transaction(self, transaction, result).await?;
        }
        // Existing profiles must already carry the exact current schema. No
        // missing table is created during reopen and no old row is migrated.
        for (name, expected) in KNOWLEDGE_STORAGE_SCHEMA {
            let mut rows = connection.query(
                "SELECT sql FROM sqlite_master WHERE name = ? AND type IN ('table', 'index')", [*name],
            ).await.map_err(|_| AgentFailure::UnsupportedVersion)?;
            let row = rows.next().await.map_err(storage)?.ok_or(AgentFailure::UnsupportedVersion)?;
            let actual = row.get::<String>(0).map_err(storage)?;
            let normalize = |sql: &str| sql.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase();
            if rows.next().await.map_err(storage)?.is_some() || normalize(&actual) != normalize(expected) {
                return Err(AgentFailure::UnsupportedVersion);
            }
        }
        let mut rows = connection.query("SELECT id, version FROM knowledge_store_schema", ())
            .await.map_err(|_| AgentFailure::UnsupportedVersion)?;
        let row = rows.next().await.map_err(storage)?.ok_or(AgentFailure::UnsupportedVersion)?;
        if row.get::<i64>(0).map_err(storage)? != 1 || row.get::<i64>(1).map_err(storage)? != 1
            || rows.next().await.map_err(storage)?.is_some()
        { return Err(AgentFailure::UnsupportedVersion); }
        self.check_access()
    }

    fn validate_learning_actor(&self, actor: &OwnerActor) -> Result<(), AgentFailure> {
        self.check_access()?;
        actor.validate()?;
        if actor.person_id != self.person_id {
            return Err(AgentFailure::NotFound);
        }
        if actor.device_id.len() > 128 {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    fn check_learning_scope(&self, scope: &ExecutionScope) -> Result<(), AgentFailure> {
        self.check_access()?;
        if scope.cancellation().is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if tokio::time::Instant::now() >= scope.deadline() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        Ok(())
    }

    pub(crate) async fn knowledge_read_context(
        &self,
        actor: &OwnerActor,
        now: DateTime<Utc>,
        scope: &ExecutionScope,
    ) -> Result<floe_knowledge::MemoryContextSnapshot, AgentFailure> {
        self.validate_learning_actor(actor)?;
        self.check_learning_scope(scope)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(storage)?;
        let result = async {
            let mut rows = transaction.query(
                "SELECT target_id, revision, person_id, kind, state, payload FROM knowledge_revisions WHERE person_id = ? AND kind = 'memory' AND state = 'active' ORDER BY target_id, revision LIMIT 4097",
                [self.person_id.to_string()],
            ).await.map_err(storage)?;
            let mut facts = Vec::new();
            while let Some(row) = rows.next().await.map_err(storage)? {
                if facts.len() >= 4096 { return Err(AgentFailure::BudgetExceeded); }
                let revision = decode_revision_row(&row, self.person_id, KnowledgeRevisionState::Active)?;
                let independent = evidence_is_independent(
                    &transaction,
                    self.person_id,
                    &revision.source_refs,
                ).await?;
                facts.push(MemoryContextFact { revision, evidence_independent: independent });
            }
            drop(rows);
            self.check_learning_scope(scope)?;
            project_memory_context(self.person_id, facts, now)
        }.await;
        let loaded = finish_transaction(self, transaction, result).await;
        self.check_access()?;
        loaded
    }

    pub(crate) async fn knowledge_overview(
        &self,
        actor: &OwnerActor,
        limit: usize,
        scope: &ExecutionScope,
    ) -> Result<MemoryOverviewSnapshot, AgentFailure> {
        self.validate_learning_actor(actor)?;
        self.check_learning_scope(scope)?;
        validate_memory_overview_limit(limit)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(storage)?;
        let result = async {
            let saved_count = count_on(
                &transaction,
                "SELECT COUNT(*) FROM knowledge_revisions WHERE person_id = ? AND kind = 'memory' AND state = 'active'",
                &self.person_id.to_string(),
            ).await?;
            let pending_count = count_on(
                &transaction,
                "SELECT COUNT(*) FROM knowledge_candidates WHERE person_id = ? AND kind = 'memory' AND state = 'pending'",
                &self.person_id.to_string(),
            ).await?;
            let mut rows = transaction.query(
                "SELECT target_id, revision, person_id, kind, state, payload FROM knowledge_revisions WHERE person_id = ? AND kind = 'memory' AND state = 'active' ORDER BY json_extract(payload, '$.created_at') DESC, target_id LIMIT ?",
                (self.person_id.to_string(), i64::try_from(limit).map_err(|_| AgentFailure::InvalidInput)?),
            ).await.map_err(storage)?;
            let mut memories = Vec::new();
            while let Some(row) = rows.next().await.map_err(storage)? {
                let revision = decode_revision_row(&row, self.person_id, KnowledgeRevisionState::Active)?;
                memories.push(project_memory_summary(&revision, self.person_id)?);
            }
            drop(rows);
            self.check_learning_scope(scope)?;
            Ok(MemoryOverviewSnapshot {
                person_id: self.person_id,
                saved_count: usize::try_from(saved_count).map_err(|_| AgentFailure::StorageUnavailable)?,
                pending_count: usize::try_from(pending_count).map_err(|_| AgentFailure::StorageUnavailable)?,
                memories,
            })
        }.await;
        let acknowledged = finish_transaction(self, transaction, result).await;
        if acknowledged.is_ok() { self.check_access()?; }
        acknowledged
    }

    pub(crate) async fn knowledge_review(
        &self,
        actor: &OwnerActor,
        scope: &ExecutionScope,
    ) -> Result<MemoryReviewSnapshot, AgentFailure> {
        self.validate_learning_actor(actor)?;
        self.check_learning_scope(scope)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(storage)?;
        let result = async {
            let mut rows = transaction.query(
                "SELECT id, person_id, idempotency_key, kind, state, target_id, created_at, payload FROM knowledge_candidates WHERE person_id = ? AND kind = 'memory' AND state = 'pending' ORDER BY created_at, id LIMIT ?",
                (self.person_id.to_string(), i64::try_from(MAX_MEMORY_REVIEW_ITEMS + 1).map_err(|_| AgentFailure::InvalidInput)?),
            ).await.map_err(storage)?;
            let mut candidates = Vec::new();
            while let Some(row) = rows.next().await.map_err(storage)? {
                if candidates.len() == MAX_MEMORY_REVIEW_ITEMS {
                    return Err(AgentFailure::BudgetExceeded);
                }
                let candidate = decode_candidate_row(&row, self.person_id)?;
                validate_memory_review_candidate(&candidate, self.person_id)?;
                candidates.push(candidate);
            }
            drop(rows);
            self.check_learning_scope(scope)?;
            Ok(MemoryReviewSnapshot { person_id: self.person_id, candidates })
        }.await;
        finish_transaction(self, transaction, result).await
    }

    pub(crate) async fn knowledge_stage(
        &self,
        request: MemoryStageRequest,
        scope: &ExecutionScope,
    ) -> Result<KnowledgeCandidate, AgentFailure> {
        self.validate_learning_actor(&request.actor)?;
        self.check_learning_scope(scope)?;
        validate_stage_request(&request.request)?;
        validate_stage_origin_actor(&request)?;
        let identity = memory_stage_identity(self.person_id, &request.request)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            // Validate the learner claim against its canonical acknowledged Output first.
            // This is immutable journal evidence; no mutable Conversation revision is reread.
            let mut learner_input = None;
            if let MemoryStageOrigin::Learner { claim, journal_revision, journal_digest } = &request.origin {
                let job = learner_job_by_id(&transaction, self.person_id, claim.job_id)
                    .await?.ok_or(AgentFailure::NotFound)?;
                if job.state != LearnerJobState::Running || job.attempts != claim.claim_attempt
                    || job.claimed_device_id.as_deref() != Some(request.actor.device_id.as_str())
                { return Err(AgentFailure::Conflict); }
                let journals = load_all_learner_journals(&transaction, &job).await?;
                let journal = journals.get(&claim.claim_attempt).ok_or(AgentFailure::StorageUnavailable)?;
                if journal.head.device_id != request.actor.device_id {
                    return Err(AgentFailure::CapabilityDenied);
                }
                validate_learner_stage(
                    &request.actor,
                    &job,
                    journal,
                    *journal_revision,
                    *journal_digest,
                    &request.request,
                )?;
                learner_input = Some(job.input);
            }

            if let Some(candidate) = stage_candidate_by_key(
                &transaction,
                self.person_id,
                &identity.candidate_key,
                &request,
                &identity.source_refs,
            ).await? {
                self.check_learning_scope(scope)?;
                return Ok(candidate);
            }
            if count_all_on(&transaction, "SELECT COUNT(*) FROM knowledge_stage_receipts").await?
                >= MAX_KNOWLEDGE_STAGE_RECEIPTS
            {
                return Err(AgentFailure::BudgetExceeded);
            }

            if let Some(input) = &learner_input {
                floe_knowledge::validate_learner_memory_time(input, request.now)?;
                validate_learner_source(self, &transaction, input).await?;
            }
            let evidence_reader = TransactionLearningEvidence { vault: self, transaction: &transaction };
            let snapshot = admit_learning_evidence(
                &evidence_reader,
                self.person_id,
                request.request.session_id,
                request.request.expected_session_revision,
                &request.request.turn_ids,
            ).await?;
            let stored_observation = observation_by_hash(
                &transaction,
                self.person_id,
                &identity.observation_hash,
            ).await?;
            if let Some(observation) = &stored_observation {
                validate_observation_identity(
                    observation,
                    self.person_id,
                    &request.request,
                    &identity.observation_hash,
                    &identity.source_refs,
                )?;
            }
            let current_revision = if let (Some(target_id), Some(_)) = (request.request.target_id, request.request.base_revision) {
                active_revision_on(&transaction, self.person_id, target_id).await?
            } else {
                None
            };
            let plan = plan_memory_stage(
                self.person_id,
                &request.request,
                &snapshot,
                stored_observation,
                current_revision,
            )?;
            let stage = payload(&plan.candidate)?;
            let stage_receipt = memory_stage_receipt(&request, &plan)?;
            let encoded_receipt = payload(&stage_receipt)?;
            if stage.len() > MAX_LEARNER_JOB_BYTES || encoded_receipt.len() > MAX_LEARNER_JOB_BYTES {
                return Err(AgentFailure::BudgetExceeded);
            }
            if observation_by_hash(
                &transaction,
                self.person_id,
                &plan.observation.content_hash,
            ).await?.is_none() {
                transaction.execute(
                    "INSERT INTO learning_observations (id, person_id, content_hash, payload) VALUES (?, ?, ?, ?)",
                    (plan.observation.id.to_string(), self.person_id.to_string(), plan.observation.content_hash.clone(), payload(&plan.observation)?),
                ).await.map_err(storage)?;
            }
            transaction.execute(
                "INSERT INTO knowledge_candidates (id, person_id, idempotency_key, kind, state, target_id, created_at, payload, stage_payload) VALUES (?, ?, ?, 'memory', 'pending', ?, ?, ?, ?)",
                (plan.candidate.id.to_string(), self.person_id.to_string(), plan.candidate.idempotency_key.clone(), plan.candidate.target_id.map(|id| id.to_string()), timestamp(plan.candidate.created_at), stage.clone(), stage),
            ).await.map_err(storage)?;
            transaction.execute(
                "INSERT INTO knowledge_stage_receipts (person_id, candidate_key, candidate_id, observation_id, payload) VALUES (?, ?, ?, ?, ?)",
                (self.person_id.to_string(), identity.candidate_key, plan.candidate.id.to_string(), plan.observation.id.to_string(), encoded_receipt),
            ).await.map_err(storage)?;
            self.check_learning_scope(scope)?;
            Ok(plan.candidate)
        }.await;
        finish_transaction(self, transaction, result).await
    }

    pub(crate) async fn knowledge_decide(
        &self,
        actor: &OwnerActor,
        request: MemoryDecisionRequest,
        scope: &ExecutionScope,
    ) -> Result<KnowledgeDecisionResult, AgentFailure> {
        self.validate_learning_actor(actor)?;
        self.check_learning_scope(scope)?;
        if !request.command_id.is_valid() || request.candidate_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            if let Some(receipt) = knowledge_decision_receipt(
                &transaction,
                request.command_id,
                self.person_id,
                &actor.device_id,
            ).await? {
                if receipt.candidate_id != request.candidate_id || receipt.kind != request.kind {
                    return Err(AgentFailure::Conflict);
                }
                self.check_learning_scope(scope)?;
                return Ok(receipt.result);
            }
            let mut rows = transaction.query(
                "SELECT id, person_id, idempotency_key, kind, state, target_id, created_at, payload FROM knowledge_candidates WHERE id = ? AND person_id = ?",
                (request.candidate_id.to_string(), self.person_id.to_string()),
            ).await.map_err(storage)?;
            let row = rows.next().await.map_err(storage)?.ok_or(AgentFailure::NotFound)?;
            let candidate = decode_candidate_row(&row, self.person_id)?;
            drop(rows);
            validate_review_actor(&KnowledgeActor::User)?;
            validate_review_candidate(&candidate)?;
            validate_memory_review_candidate(&candidate, self.person_id)?;
            let admission = if request.kind == KnowledgeDecisionKind::Approve {
                let independent = evidence_is_independent(
                    &transaction,
                    self.person_id,
                    &candidate.source_refs,
                ).await?;
                let target_id = candidate.target_id.unwrap_or_else(Uuid::new_v4);
                let current_revision = if candidate.operation == KnowledgeOperation::Revise {
                    active_revision_on(&transaction, self.person_id, target_id).await?
                } else { None };
                let current_payload_hash = current_revision.as_ref()
                    .map(|revision| floe_knowledge::knowledge_content_hash(&revision.payload))
                    .transpose()?;
                let target_exists = revision_exists_on(&transaction, target_id).await?;
                Some(floe_knowledge::ReviewAdmission {
                    target_id,
                    target_exists,
                    current_revision,
                    current_payload_hash,
                    evidence_independent: independent,
                })
            } else { None };
            let plan = plan_memory_review(
                candidate,
                self.person_id,
                request.kind,
                request.decided_at,
                admission,
            )?;
            let KnowledgeDecisionResult { candidate, decision, revision, mutation } = plan.result;
            if let Some(superseded) = &plan.superseded {
                let changed = transaction.execute(
                    "UPDATE knowledge_revisions SET state = 'superseded', payload = ? WHERE target_id = ? AND revision = ? AND person_id = ? AND kind = 'memory' AND state = 'active'",
                    (payload(superseded)?, superseded.target_id.to_string(), integer(superseded.revision)?, self.person_id.to_string()),
                ).await.map_err(storage)?;
                if changed != 1 { return Err(AgentFailure::Conflict); }
            }
            if let Some(revision) = &revision {
                transaction.execute(
                    "INSERT INTO knowledge_revisions (target_id, revision, person_id, kind, state, payload) VALUES (?, ?, ?, 'memory', 'active', ?)",
                    (revision.target_id.to_string(), integer(revision.revision)?, self.person_id.to_string(), payload(revision)?),
                ).await.map_err(storage)?;
            }
            if let Some(mutation) = &mutation {
                transaction.execute(
                    "INSERT INTO knowledge_mutations (id, candidate_id, target_id, created_at, payload) VALUES (?, ?, ?, ?, ?)",
                    (mutation.id.to_string(), candidate.id.to_string(), mutation.target_id.to_string(), timestamp(mutation.created_at), payload(mutation)?),
                ).await.map_err(storage)?;
            }
            let changed = transaction.execute(
                "UPDATE knowledge_candidates SET state = ?, target_id = ?, payload = ? WHERE id = ? AND person_id = ? AND state = 'pending'",
                (state_name(candidate.state), candidate.target_id.map(|id| id.to_string()), payload(&candidate)?, candidate.id.to_string(), self.person_id.to_string()),
            ).await.map_err(storage)?;
            if changed != 1 { return Err(AgentFailure::Conflict); }
            transaction.execute(
                "INSERT INTO knowledge_candidate_decisions (id, candidate_id, payload) VALUES (?, ?, ?)",
                (decision.id.to_string(), candidate.id.to_string(), payload(&decision)?),
            ).await.map_err(storage)?;
            if count_all_on(&transaction, "SELECT COUNT(*) FROM knowledge_command_receipts").await? >= MAX_KNOWLEDGE_COMMAND_RECEIPTS {
                return Err(AgentFailure::BudgetExceeded);
            }
            let decision_result = KnowledgeDecisionResult { candidate, decision, revision, mutation };
            let encoded_result = payload(&decision_result)?;
            if encoded_result.len() > MAX_LEARNER_JOB_BYTES { return Err(AgentFailure::BudgetExceeded); }
            transaction.execute(
                "INSERT INTO knowledge_command_receipts (command_id, person_id, device_id, command_kind, candidate_id, decision_kind, payload) VALUES (?, ?, ?, 'memory_decision', ?, ?, ?)",
                (request.command_id.as_uuid().to_string(), self.person_id.to_string(), actor.device_id.clone(), request.candidate_id.to_string(), decision_kind_name(request.kind), encoded_result),
            ).await.map_err(storage)?;
            self.check_learning_scope(scope)?;
            Ok(decision_result)
        }.await;
        finish_transaction(self, transaction, result).await
    }

    pub(crate) async fn learner_discovery_sessions(
        &self,
        actor: &OwnerActor,
        limit: usize,
        scope: &ExecutionScope,
    ) -> Result<Vec<LearningSessionSnapshot>, AgentFailure> {
        self.validate_learning_actor(actor)?;
        self.check_learning_scope(scope)?;
        if limit == 0 || limit > 64 {
            return Err(AgentFailure::InvalidInput);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(storage)?;
        let result = async {
            let mut rows = transaction.query(
                "SELECT id, revision, payload FROM agent_sessions ORDER BY rowid DESC LIMIT ?",
                [i64::try_from(limit).map_err(|_| AgentFailure::InvalidInput)?],
            ).await.map_err(storage)?;
            let mut snapshots = Vec::new();
            while let Some(row) = rows.next().await.map_err(storage)? {
                let stored_id = row.get::<String>(0).map_err(storage)?;
                let stored_revision = row.get::<i64>(1).map_err(storage)?;
                let session: AgentSession = decode(&row.get::<String>(2).map_err(storage)?)?;
                if session.person_id != self.person_id || session.id.to_string() != stored_id
                    || i64::try_from(session.revision).map_err(|_| AgentFailure::StorageUnavailable)? != stored_revision
                { return Err(AgentFailure::VaultUnavailable); }
                self.payload(&session)?;
                let messages = learning_transcript_messages(self, &transaction, &session).await?;
                let actual_turns = floe_knowledge::explicit_learning_evidence_turns(&messages)?;
                let evidence_reader = TransactionLearningEvidence { vault: self, transaction: &transaction };
                let evidence = if actual_turns.is_empty() {
                    LearningEvidenceSnapshot {
                        person_id: session.person_id,
                        session_id: session.id,
                        revision: session.revision,
                        outcome: session.last_outcome.map(learning_outcome),
                        personal: session.scope.is_none() && session.data_classes == [DataClass::Personal],
                        active_turn: session.active_turn.is_some(),
                        pending_output: session.pending_output.is_some(),
                        turn_ids: Vec::new(),
                        coverage: DependencyCoverage::Unknown,
                        purpose: floe_knowledge::EvidenceProjectionPurpose::Learning,
                    }
                } else {
                    evidence_reader.read_learning_evidence(
                        self.person_id,
                        session.id,
                        &actual_turns,
                    ).await?
                };
                if evidence.session_id != session.id || evidence.revision != session.revision
                    || evidence.person_id != self.person_id
                { return Err(AgentFailure::StorageUnavailable); }
                let snapshot = LearningSessionSnapshot { evidence, messages };
                let bounded_bytes = snapshot.messages.iter().fold(0usize, |total, message| {
                    total.saturating_add(match message {
                        LearningTranscriptMessage::User { text, .. }
                        | LearningTranscriptMessage::Assistant { text, .. } => text.len().saturating_add(64),
                        LearningTranscriptMessage::Other => 8,
                    })
                });
                if bounded_bytes > 512 * 1024 {
                    return Err(AgentFailure::BudgetExceeded);
                }
                snapshots.push(snapshot);
            }
            drop(rows);
            self.check_learning_scope(scope)?;
            Ok(snapshots)
        }.await;
        finish_transaction(self, transaction, result).await
    }

    pub(crate) async fn learner_enqueue(
        &self,
        actor: &OwnerActor,
        mut input: LearnerReviewInput,
        available_at: DateTime<Utc>,
        scope: &ExecutionScope,
    ) -> Result<LearnerReviewJob, AgentFailure> {
        self.validate_learning_actor(actor)?;
        self.check_learning_scope(scope)?;
        validate_learner_input(&input, self.person_id)?;
        input.digest = input.digest.trim().to_owned();
        let idempotency_key = learner_job_key(&input)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            if let Some(existing) = learner_job_by_key(
                &transaction,
                self.person_id,
                &idempotency_key,
            ).await? {
                let mut replay_input = input.clone();
                replay_input.run_id = existing.id;
                if replay_input != existing.input {
                    return Err(AgentFailure::Conflict);
                }
                self.check_learning_scope(scope)?;
                return Ok(existing);
            }
            validate_learner_source(self, &transaction, &input).await?;
            let job = new_learner_job(input, available_at)?;
            let encoded = payload(&job)?;
            if encoded.len() > MAX_LEARNER_JOB_BYTES {
                return Err(AgentFailure::BudgetExceeded);
            }
            transaction.execute(
                "INSERT INTO learner_review_jobs (id, person_id, idempotency_key, state, attempts, available_at, payload) VALUES (?, ?, ?, 'queued', 0, ?, ?)",
                (job.id.to_string(), self.person_id.to_string(), job.idempotency_key.clone(), timestamp(job.available_at), encoded),
            ).await.map_err(storage)?;
            self.check_learning_scope(scope)?;
            Ok(job)
        }.await;
        finish_transaction(self, transaction, result).await
    }

    pub(crate) async fn learner_claim_review(
        &self,
        actor: &OwnerActor,
        budget: LearnerBudget,
        now: DateTime<Utc>,
        scope: &ExecutionScope,
    ) -> Result<Option<LearnerReviewJob>, AgentFailure> {
        self.validate_learning_actor(actor)?;
        self.check_learning_scope(scope)?;
        validate_learner_budget(&budget)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let mut rows = transaction.query(
                "SELECT id, person_id, idempotency_key, state, attempts, available_at, payload FROM learner_review_jobs WHERE person_id = ? AND ((state IN ('queued', 'deferred') AND available_at <= ?) OR (state = 'running' AND available_at <= ?)) ORDER BY available_at, id LIMIT 1",
                (self.person_id.to_string(), timestamp(now), timestamp(now)),
            ).await.map_err(storage)?;
            let Some(row) = rows.next().await.map_err(storage)? else {
                self.check_learning_scope(scope)?;
                return Ok(None);
            };
            let stored_payload = row.get::<String>(6).map_err(storage)?;
            let mut job = decode_job_row(&row, self.person_id)?;
            drop(rows);
            let journals = load_all_learner_journals(&transaction, &job).await?;
            if job.state == LearnerJobState::Running {
                let journal = journals.get(&job.attempts).ok_or(AgentFailure::StorageUnavailable)?;
                if journal.head.device_id != job.claimed_device_id.as_deref().unwrap_or("") {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let lifecycle = recover_learner_claim(&job, journal, &actor.device_id, now)?;
                job = job.apply_lifecycle(lifecycle)?;
                let changed = transaction.execute(
                    "UPDATE learner_review_jobs SET state = ?, attempts = ?, available_at = ?, payload = ? WHERE id = ? AND person_id = ? AND state = 'running' AND attempts = ? AND payload = ?",
                    (learner_job_state(job.state), i64::from(job.attempts), timestamp(job.available_at), payload(&job)?, job.id.to_string(), self.person_id.to_string(), i64::from(job.attempts), stored_payload),
                ).await.map_err(storage)?;
                if changed != 1 { return Err(AgentFailure::Conflict); }
                self.check_learning_scope(scope)?;
                return if job.state == LearnerJobState::Running { Ok(Some(job)) } else { Ok(None) };
            }
            let evidence_check = match floe_knowledge::validate_learner_memory_time(&job.input, now) {
                Ok(()) => validate_learner_source(self, &transaction, &job.input).await,
                Err(failure) => Err(failure),
            };
            match evidence_check {
                Ok(()) => {}
                Err(failure @ (AgentFailure::StaleContext | AgentFailure::NotFound | AgentFailure::PolicyDenied)) => {
                    let rejected = reject_learner_claim(&job.lifecycle(), failure, now)?;
                    job = job.apply_lifecycle(rejected)?;
                    let changed = update_job_cas(&transaction, &job, self.person_id, &stored_payload).await?;
                    if !changed { return Err(AgentFailure::Conflict); }
                    self.check_learning_scope(scope)?;
                    return Ok(None);
                }
                Err(failure) => return Err(failure),
            }
            let previous_state = learner_job_state(job.state).to_owned();
            let previous_attempts = job.attempts;
            let previous_available_at = timestamp(job.available_at);
            let claim = claim_learner_job(&job.lifecycle(), now, &actor.device_id)?;
            let lifecycle = match claim {
                LearnerJobClaim::Exhausted(lifecycle) => {
                    job = job.apply_lifecycle(lifecycle)?;
                    let changed = update_job_cas(&transaction, &job, self.person_id, &stored_payload).await?;
                    if !changed { return Err(AgentFailure::Conflict); }
                    self.check_learning_scope(scope)?;
                    return Ok(None);
                }
                LearnerJobClaim::Claimed(lifecycle) => lifecycle,
            };
            job = job.apply_lifecycle(lifecycle)?;
            let head = LearnerJournalHead::new(&job, budget)?;
            if let Some(_) = learner_journal_head(&transaction, job.id, self.person_id, job.attempts).await? {
                return Err(AgentFailure::StorageUnavailable);
            }
            transaction.execute(
                "INSERT INTO learner_journal_heads (job_id, person_id, claim_attempt, device_id, journal_revision, journal_digest, payload) VALUES (?, ?, ?, ?, 0, ?, ?)",
                (job.id.to_string(), self.person_id.to_string(), i64::from(job.attempts), head.device_id.clone(), digest_hex(&head.journal_digest), payload(&head)?),
            ).await.map_err(storage)?;
            let changed = transaction.execute(
                "UPDATE learner_review_jobs SET state = 'running', attempts = ?, available_at = ?, payload = ? WHERE id = ? AND person_id = ? AND state = ? AND attempts = ? AND available_at = ? AND payload = ?",
                (i64::from(job.attempts), timestamp(job.available_at), payload(&job)?, job.id.to_string(), self.person_id.to_string(), previous_state, i64::from(previous_attempts), previous_available_at, stored_payload),
            ).await.map_err(storage)?;
            if changed != 1 { return Err(AgentFailure::Conflict); }
            self.check_learning_scope(scope)?;
            Ok(Some(job))
        }.await;
        finish_transaction(self, transaction, result).await
    }

    pub(crate) async fn learner_settle_review(
        &self,
        actor: &OwnerActor,
        job_id: Uuid,
        expected_attempt: u8,
        settlement: LearnerJobSettlement,
        now: DateTime<Utc>,
    ) -> Result<(), AgentFailure> {
        self.validate_learning_actor(actor)?;
        let claim = LearnerClaimRef { job_id, claim_attempt: expected_attempt };
        claim.validate()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let encoded_settlement = payload(&StoredLearnerSettlement::from(&settlement))?;
            if encoded_settlement.len() > MAX_LEARNER_EVENT_BYTES {
                return Err(AgentFailure::BudgetExceeded);
            }
            if let Some((stored_device, stored_settlement, result)) = settlement_receipt(
                &transaction,
                job_id,
                self.person_id,
                expected_attempt,
            ).await? {
                if stored_device != actor.device_id || stored_settlement != encoded_settlement {
                    return Err(AgentFailure::Conflict);
                }
                let job: LearnerReviewJob = decode(&result)?;
                validate_learner_job_rowless(&job, self.person_id)?;
                if job.id != job_id || job.attempts != expected_attempt
                    || job.claimed_device_id.as_deref() != Some(actor.device_id.as_str())
                { return Err(AgentFailure::StorageUnavailable); }
                validate_settlement_result(&settlement, &job)?;
                let current = learner_job_by_id(&transaction, self.person_id, job_id)
                    .await?.ok_or(AgentFailure::StorageUnavailable)?;
                load_all_learner_journals(&transaction, &current).await?;
                self.check_access()?;
                return Ok(());
            }
            let mut job = learner_job_by_id(&transaction, self.person_id, job_id)
                .await?.ok_or(AgentFailure::NotFound)?;
            if job.state != LearnerJobState::Running || job.attempts != expected_attempt
                || job.claimed_device_id.as_deref() != Some(actor.device_id.as_str())
            { return Err(AgentFailure::Conflict); }
            let journals = load_all_learner_journals(&transaction, &job).await?;
            let journal = journals.get(&expected_attempt).ok_or(AgentFailure::StorageUnavailable)?;
            if journal.head.device_id != actor.device_id || journal.head.person_id != self.person_id {
                return Err(AgentFailure::CapabilityDenied);
            }
            let candidate = match &settlement {
                LearnerJobSettlement::Completed { candidate_id: Some(candidate_id) } => {
                    Some(candidate_by_id(&transaction, self.person_id, *candidate_id).await?)
                }
                _ => None,
            };
            validate_learner_settlement(&job, journal, &settlement, candidate.as_ref())?;
            let lifecycle = settle_learner_job(&job.lifecycle(), expected_attempt, settlement.clone(), now)?;
            job = job.apply_lifecycle(lifecycle)?;
            validate_settlement_result(&settlement, &job)?;
            let old_payload = payload(&learner_job_by_id(&transaction, self.person_id, job_id).await?.ok_or(AgentFailure::NotFound)?)?;
            let changed = update_running_job_cas(&transaction, &job, self.person_id, expected_attempt, &old_payload).await?;
            if !changed { return Err(AgentFailure::Conflict); }
            let encoded_job = payload(&job)?;
            if encoded_job.len() > MAX_LEARNER_JOB_BYTES { return Err(AgentFailure::BudgetExceeded); }
            transaction.execute(
                "INSERT INTO learner_settlement_receipts (job_id, person_id, claim_attempt, device_id, settlement, result) VALUES (?, ?, ?, ?, ?, ?)",
                (job_id.to_string(), self.person_id.to_string(), i64::from(expected_attempt), actor.device_id.clone(), encoded_settlement, encoded_job),
            ).await.map_err(storage)?;
            self.check_access()?;
            Ok(())
        }.await;
        finish_transaction(self, transaction, result).await
    }

    pub(crate) async fn learner_read_claim(
        &self,
        actor: &OwnerActor,
        claim: LearnerClaimRef,
        scope: &ExecutionScope,
    ) -> Result<LearnerReviewInput, AgentFailure> {
        self.validate_learning_actor(actor)?;
        self.check_learning_scope(scope)?;
        claim.validate()?;
        if scope.root_run_id() != RunId::from_uuid(claim.job_id) {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(storage)?;
        let result = async {
            let job = learner_job_by_id(&transaction, self.person_id, claim.job_id)
                .await?.ok_or(AgentFailure::NotFound)?;
            validate_running_claim(&job, claim, actor)?;
            let journals = load_all_learner_journals(&transaction, &job).await?;
            let journal = journals.get(&claim.claim_attempt).ok_or(AgentFailure::StorageUnavailable)?;
            if journal.head.device_id != actor.device_id { return Err(AgentFailure::CapabilityDenied); }
            validate_learner_source(self, &transaction, &job.input).await?;
            let confirmed = learner_job_by_id(&transaction, self.person_id, claim.job_id)
                .await?.ok_or(AgentFailure::NotFound)?;
            validate_running_claim(&confirmed, claim, actor)?;
            if confirmed != job { return Err(AgentFailure::Conflict); }
            self.check_learning_scope(scope)?;
            Ok(job.input)
        }.await;
        finish_transaction(self, transaction, result).await
    }

    pub(crate) async fn load_learner_journal(
        &self,
        actor: &OwnerActor,
        claim: LearnerClaimRef,
    ) -> Result<LearnerClaimJournal, AgentFailure> {
        self.validate_learning_actor(actor)?;
        claim.validate()?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(storage)?;
        let result = async {
            let job = learner_job_by_id(&transaction, self.person_id, claim.job_id)
                .await?.ok_or(AgentFailure::NotFound)?;
            if claim.claim_attempt > job.attempts { return Err(AgentFailure::NotFound); }
            let journals = load_all_learner_journals(&transaction, &job).await?;
            let journal = journals.get(&claim.claim_attempt).cloned().ok_or(AgentFailure::StorageUnavailable)?;
            if journal.head.person_id != actor.person_id || journal.head.device_id != actor.device_id {
                return Err(AgentFailure::CapabilityDenied);
            }
            self.check_access()?;
            Ok(journal)
        }.await;
        let loaded = finish_transaction(self, transaction, result).await;
        self.check_access()?;
        loaded
    }

    pub(crate) async fn append_learner_journal(
        &self,
        actor: &OwnerActor,
        claim: LearnerClaimRef,
        event: JournalEvent,
    ) -> Result<u64, AgentFailure> {
        self.validate_learning_actor(actor)?;
        claim.validate()?;
        let event_key = learner_event_key(claim, &event)?;
        let encoded_event = payload(&event)?;
        if encoded_event.len() > MAX_LEARNER_EVENT_BYTES { return Err(AgentFailure::BudgetExceeded); }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let job = learner_job_by_id(&transaction, self.person_id, claim.job_id)
                .await?.ok_or(AgentFailure::NotFound)?;
            validate_running_claim(&job, claim, actor)?;
            let journals = load_all_learner_journals(&transaction, &job).await?;
            let journal = journals.get(&claim.claim_attempt).cloned().ok_or(AgentFailure::StorageUnavailable)?;
            if journal.head.device_id != actor.device_id { return Err(AgentFailure::CapabilityDenied); }
            if let JournalEvent::ModelIntent { parent_task_id, attempt_id, reservation_ceiling, projection_ref, plan } = &event {
                plan.validate()?;
                reservation_ceiling.validate()?;
                if parent_task_id.is_some() || attempt_id.is_nil() || projection_ref.as_uuid().is_nil()
                    || plan.principal != self.person_id.to_string() || plan.device_id != actor.device_id
                    || plan.purpose != floe_knowledge::LEARNER_INFERENCE_PURPOSE
                    || plan.consumer != floe_knowledge::LEARNER_INFERENCE_CONSUMER
                { return Err(AgentFailure::PolicyDenied); }
            }
            let mut replay_revision = None;
            for entry in &journal.entries {
                if learner_event_key(claim, &entry.event)? == event_key {
                    if payload(&entry.event)? != encoded_event { return Err(AgentFailure::Conflict); }
                    replay_revision = Some(entry.revision);
                    break;
                }
            }
            if let Some(revision) = replay_revision {
                self.check_access()?;
                return Ok(revision);
            }
            if matches!(&event, JournalEvent::ModelIntent { .. }) {
                validate_learner_source(self, &transaction, &job.input).await?;
            }
            let next_revision = journal.head.journal_revision.checked_add(1).ok_or(AgentFailure::BudgetExceeded)?;
            let mut next_entries = journal.entries.clone();
            next_entries.push(JournalEntry { revision: next_revision, event: event.clone() });
            let next_head = advance_learner_journal(&journal.head, &next_entries)?;
            let changed = transaction.execute(
                "INSERT INTO learner_execution_journal (job_id, person_id, claim_attempt, sequence, event_key, payload) VALUES (?, ?, ?, ?, ?, ?)",
                (claim.job_id.to_string(), self.person_id.to_string(), i64::from(claim.claim_attempt), integer(next_revision)?, event_key, encoded_event),
            ).await.map_err(storage)?;
            if changed != 1 { return Err(AgentFailure::Conflict); }
            let updated = transaction.execute(
                "UPDATE learner_journal_heads SET journal_revision = ?, journal_digest = ?, payload = ? WHERE job_id = ? AND person_id = ? AND claim_attempt = ? AND device_id = ? AND journal_revision = ? AND journal_digest = ?",
                (integer(next_head.journal_revision)?, digest_hex(&next_head.journal_digest), payload(&next_head)?, claim.job_id.to_string(), self.person_id.to_string(), i64::from(claim.claim_attempt), actor.device_id.clone(), integer(journal.head.journal_revision)?, digest_hex(&journal.head.journal_digest)),
            ).await.map_err(storage)?;
            if updated != 1 { return Err(AgentFailure::Conflict); }
            self.check_access()?;
            Ok(next_head.journal_revision)
        }.await;
        let acknowledged = finish_transaction(self, transaction, result).await;
        if acknowledged.is_ok() { self.check_access()?; }
        acknowledged
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum StoredLearnerSettlement {
    Blocked { blockage: floe_knowledge::LearnerProjectionBlock },
    Completed { candidate_id: Option<Uuid> },
    Deferred { available_at: DateTime<Utc>, failure: AgentFailure },
    Failed { failure: AgentFailure },
}
impl From<&LearnerJobSettlement> for StoredLearnerSettlement {
    fn from(value: &LearnerJobSettlement) -> Self {
        match value {
            LearnerJobSettlement::Blocked { blockage } => Self::Blocked { blockage: blockage.clone() },
            LearnerJobSettlement::Completed { candidate_id } => Self::Completed { candidate_id: *candidate_id },
            LearnerJobSettlement::Deferred { available_at, failure } => Self::Deferred { available_at: *available_at, failure: *failure },
            LearnerJobSettlement::Failed { failure } => Self::Failed { failure: *failure },
        }
    }
}

fn validate_settlement_result(
    settlement: &LearnerJobSettlement,
    job: &LearnerReviewJob,
) -> Result<(), AgentFailure> {
    let matches = match settlement {
        LearnerJobSettlement::Blocked { blockage } => {
            job.state == LearnerJobState::Blocked && job.blocked.as_ref() == Some(blockage)
                && job.candidate_id.is_none() && job.last_failure.is_none()
        }
        LearnerJobSettlement::Completed { candidate_id } => {
            job.state == LearnerJobState::Completed && job.candidate_id == *candidate_id
                && job.blocked.is_none() && job.last_failure.is_none()
        }
        LearnerJobSettlement::Deferred { available_at, failure } => {
            if job.attempts >= MAX_LEARNER_JOB_ATTEMPTS {
                job.state == LearnerJobState::Failed && job.last_failure == Some(*failure)
                    && job.finished_at.is_some()
            } else {
                job.state == LearnerJobState::Deferred && job.available_at == *available_at
                    && job.last_failure == Some(*failure) && job.finished_at.is_none()
            }
        }
        LearnerJobSettlement::Failed { failure } => {
            job.state == LearnerJobState::Failed && job.last_failure == Some(*failure)
                && job.finished_at.is_some()
        }
    };
    if matches { Ok(()) } else { Err(AgentFailure::StorageUnavailable) }
}

async fn learning_transcript_messages<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    transaction: &turso::transaction::Transaction<'_>,
    session: &AgentSession,
) -> Result<Vec<LearningTranscriptMessage>, AgentFailure> {
    let mut output = Vec::with_capacity(session.messages.len());
    for message in &session.messages {
        match message {
            floe_conversation::AgentMessage::User { turn_id, message_id, text } => {
                if turn_id.is_nil() || message_id.is_nil() { return Err(AgentFailure::VaultUnavailable); }
                output.push(LearningTranscriptMessage::User {
                    message_id: *message_id,
                    turn_id: *turn_id,
                    text: text.clone(),
                });
            }
            floe_conversation::AgentMessage::Assistant { turn_id, text } => {
                let run_id = RunId::from_uuid(*turn_id).ok_or(AgentFailure::VaultUnavailable)?;
                let run = vault.conversation_run_on(transaction, run_id)
                    .await?.ok_or(AgentFailure::StorageUnavailable)?;
                run.validate(vault.person_id)?;
                if run.session_id != session.id || run.run_id != run_id || run.user_message_id.is_nil() {
                    return Err(AgentFailure::VaultUnavailable);
                }
                output.push(LearningTranscriptMessage::Assistant {
                    turn_id: *turn_id,
                    original_user_message_id: run.user_message_id,
                    text: text.clone(),
                });
            }
            _ => output.push(LearningTranscriptMessage::Other),
        }
    }
    Ok(output)
}

fn validate_stage_origin_actor(request: &MemoryStageRequest) -> Result<(), AgentFailure> {
    match (&request.origin, &request.request.actor) {
        (MemoryStageOrigin::User, KnowledgeActor::User) => Ok(()),
        (MemoryStageOrigin::Learner { claim, .. }, KnowledgeActor::Learner { run_id })
            if *run_id == claim.job_id => claim.validate(),
        _ => Err(AgentFailure::PolicyDenied),
    }
}

fn validate_running_claim(
    job: &LearnerReviewJob,
    claim: LearnerClaimRef,
    actor: &OwnerActor,
) -> Result<(), AgentFailure> {
    if job.state != LearnerJobState::Running || job.id != claim.job_id
        || job.attempts != claim.claim_attempt || job.input.run_id != job.id
        || job.input.person_id != actor.person_id
        || job.claimed_device_id.as_deref() != Some(actor.device_id.as_str())
    { return Err(AgentFailure::Conflict); }
    Ok(())
}

fn decision_kind_name(kind: KnowledgeDecisionKind) -> &'static str {
    match kind {
        KnowledgeDecisionKind::Approve => "approve",
        KnowledgeDecisionKind::Reject => "reject",
    }
}

fn learner_job_state(state: LearnerJobState) -> &'static str {
    match state {
        LearnerJobState::Queued => "queued",
        LearnerJobState::Running => "running",
        LearnerJobState::Deferred => "deferred",
        LearnerJobState::Completed => "completed",
        LearnerJobState::Blocked => "blocked",
        LearnerJobState::Failed => "failed",
    }
}

fn state_name(state: KnowledgeCandidateState) -> &'static str {
    match state {
        KnowledgeCandidateState::Pending => "pending",
        KnowledgeCandidateState::Approved => "approved",
        KnowledgeCandidateState::Rejected => "rejected",
        KnowledgeCandidateState::Superseded => "superseded",
        KnowledgeCandidateState::Withdrawn => "withdrawn",
    }
}

fn kind_name(kind: KnowledgeKind) -> &'static str {
    match kind {
        KnowledgeKind::Memory => "memory",
        KnowledgeKind::Playbook => "playbook",
    }
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
}

fn digest_hex(value: &[u8; 32]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hash(value: &impl Serialize) -> Result<String, AgentFailure> {
    let bytes = serde_json::to_vec(value).map_err(storage)?;
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    Ok(digest_hex(&digest))
}

fn payload(value: &impl Serialize) -> Result<String, AgentFailure> {
    serde_json::to_string(value).map_err(storage)
}

fn decode<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, AgentFailure> {
    serde_json::from_str(value).map_err(unavailable)
}

fn integer(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::BudgetExceeded)
}

async fn finish_transaction<Keys: VaultKeyProvider, T>(
    vault: &EncryptedAgentVault<Keys>,
    transaction: turso::transaction::Transaction<'_>,
    result: Result<T, AgentFailure>,
) -> Result<T, AgentFailure> {
    match result {
        Ok(value) => {
            if let Err(failure) = vault.check_access() {
                if transaction.rollback().await.is_err() {
                    vault.unavailable.store(true, Ordering::Release);
                    return Err(AgentFailure::VaultUnavailable);
                }
                return Err(failure);
            }
            if transaction.commit().await.is_err() {
                vault.unavailable.store(true, Ordering::Release);
                return Err(AgentFailure::StorageUnavailable);
            }
            if let Err(failure) = vault.check_access() {
                vault.unavailable.store(true, Ordering::Release);
                return Err(failure);
            }
            Ok(value)
        }
        Err(failure) => {
            if transaction.rollback().await.is_err() {
                vault.unavailable.store(true, Ordering::Release);
                return Err(AgentFailure::VaultUnavailable);
            }
            if matches!(failure, AgentFailure::StorageUnavailable | AgentFailure::VaultUnavailable | AgentFailure::UnsupportedVersion) {
                vault.unavailable.store(true, Ordering::Release);
            }
            Err(failure)
        }
    }
}

async fn count_on(
    transaction: &turso::transaction::Transaction<'_>,
    sql: &str,
    parameter: &str,
) -> Result<i64, AgentFailure> {
    let mut rows = transaction.query(sql, [parameter.to_owned()]).await.map_err(storage)?;
    let row = rows.next().await.map_err(storage)?.ok_or(AgentFailure::StorageUnavailable)?;
    let count = row.get::<i64>(0).map_err(storage)?;
    if rows.next().await.map_err(storage)?.is_some() || count < 0 {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(count)
}

async fn count_all_on(
    transaction: &turso::transaction::Transaction<'_>,
    sql: &str,
) -> Result<i64, AgentFailure> {
    let mut rows = transaction.query(sql, ()).await.map_err(storage)?;
    let row = rows.next().await.map_err(storage)?.ok_or(AgentFailure::StorageUnavailable)?;
    let count = row.get::<i64>(0).map_err(storage)?;
    if rows.next().await.map_err(storage)?.is_some() || count < 0 {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(count)
}

fn decode_revision_row(
    row: &turso::Row,
    person_id: floe_kernel::PersonId,
    expected_state: KnowledgeRevisionState,
) -> Result<KnowledgeRevision, AgentFailure> {
    let target_id = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
        .map_err(unavailable)?;
    let revision_number = row.get::<i64>(1).map_err(storage)?;
    let stored_person = row.get::<String>(2).map_err(storage)?;
    let stored_kind = row.get::<String>(3).map_err(storage)?;
    let stored_state = row.get::<String>(4).map_err(storage)?;
    let encoded = row.get::<String>(5).map_err(storage)?;
    if encoded.len() > MAX_LEARNER_JOB_BYTES { return Err(AgentFailure::BudgetExceeded); }
    let revision: KnowledgeRevision = decode(&encoded)?;
    if revision.schema_version != floe_knowledge::KNOWLEDGE_VERSION
        || revision.target_id != target_id
        || i64::try_from(revision.revision).map_err(|_| AgentFailure::VaultUnavailable)? != revision_number
        || stored_person != person_id.to_string()
        || revision.person_id != person_id
        || stored_kind != kind_name(revision.kind)
        || stored_state != revision_state_name(revision.state)
        || revision.state != expected_state
        || revision.kind != KnowledgeKind::Memory
    { return Err(AgentFailure::VaultUnavailable); }
    project_memory_summary(&revision, person_id)?;
    Ok(revision)
}

fn revision_state_name(state: KnowledgeRevisionState) -> &'static str {
    match state {
        KnowledgeRevisionState::Active => "active",
        KnowledgeRevisionState::Superseded => "superseded",
        KnowledgeRevisionState::Stale => "stale",
        KnowledgeRevisionState::Archived => "archived",
        KnowledgeRevisionState::Tombstoned => "tombstoned",
    }
}

fn decode_candidate_row(
    row: &turso::Row,
    person_id: floe_kernel::PersonId,
) -> Result<KnowledgeCandidate, AgentFailure> {
    let stored_id = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
        .map_err(unavailable)?;
    let stored_person = row.get::<String>(1).map_err(storage)?;
    let stored_key = row.get::<String>(2).map_err(storage)?;
    let stored_kind = row.get::<String>(3).map_err(storage)?;
    let stored_state = row.get::<String>(4).map_err(storage)?;
    let stored_target = row.get::<Option<String>>(5).map_err(storage)?
        .map(|value| Uuid::parse_str(&value).map_err(unavailable)).transpose()?;
    let stored_created = row.get::<String>(6).map_err(storage)?;
    let encoded = row.get::<String>(7).map_err(storage)?;
    if encoded.len() > MAX_LEARNER_JOB_BYTES { return Err(AgentFailure::BudgetExceeded); }
    let candidate: KnowledgeCandidate = decode(&encoded)?;
    if candidate.schema_version != floe_knowledge::KNOWLEDGE_VERSION
        || candidate.id != stored_id || candidate.person_id != person_id
        || stored_person != person_id.to_string()
        || candidate.idempotency_key != stored_key || candidate.kind != KnowledgeKind::Memory
        || stored_kind != kind_name(candidate.kind)
        || stored_state != state_name(candidate.state)
        || candidate.target_id != stored_target
        || timestamp(candidate.created_at) != stored_created
    { return Err(AgentFailure::VaultUnavailable); }
    Ok(candidate)
}

async fn stage_candidate_by_key(
    transaction: &turso::transaction::Transaction<'_>,
    person_id: floe_kernel::PersonId,
    candidate_key: &str,
    request: &MemoryStageRequest,
    source_refs: &[LearningEvidenceRef],
) -> Result<Option<KnowledgeCandidate>, AgentFailure> {
    let mut rows = transaction.query(
        "SELECT id, person_id, idempotency_key, kind, state, target_id, created_at, payload, stage_payload FROM knowledge_candidates WHERE person_id = ? AND idempotency_key = ?",
        (person_id.to_string(), candidate_key.to_owned()),
    ).await.map_err(storage)?;
    let current = if let Some(row) = rows.next().await.map_err(storage)? {
        let current = decode_candidate_row(&row, person_id)?;
        let encoded_stage = row.get::<String>(8).map_err(storage)?;
        if encoded_stage.len() > MAX_LEARNER_JOB_BYTES { return Err(AgentFailure::BudgetExceeded); }
        Some((current, decode::<KnowledgeCandidate>(&encoded_stage)?))
    } else { None };
    if rows.next().await.map_err(storage)?.is_some() { return Err(AgentFailure::VaultUnavailable); }
    drop(rows);

    let receipt = memory_stage_receipt_by_key(transaction, person_id, candidate_key).await?;
    let (current, stage, receipt) = match (current, receipt) {
        (None, None) => return Ok(None),
        (Some(_), None) | (None, Some(_)) => return Err(AgentFailure::StorageUnavailable),
        (Some((current, stage)), Some(receipt)) => (current, stage, receipt),
    };
    validate_memory_review_candidate(&stage, person_id)?;
    if stage.id.is_nil() || stage.observation_id.is_nil() { return Err(AgentFailure::StorageUnavailable); }
    let target_matches = current.target_id == stage.target_id
        || (stage.operation == KnowledgeOperation::Create
            && stage.target_id.is_none()
            && current.state == KnowledgeCandidateState::Approved
            && current.target_id.is_some());
    if current.id != stage.id || current.person_id != stage.person_id
        || current.idempotency_key != stage.idempotency_key
        || current.observation_id != stage.observation_id || current.kind != stage.kind
        || current.operation != stage.operation || current.base_revision != stage.base_revision
        || current.payload != stage.payload || current.source_refs != stage.source_refs
        || current.before_hash != stage.before_hash || current.after_hash != stage.after_hash
        || current.extractor_version != stage.extractor_version
        || current.prompt_version != stage.prompt_version || current.actor != stage.actor
        || current.created_at != stage.created_at
        || !target_matches
        || stage.idempotency_key != candidate_key || stage.source_refs != source_refs
    { return Err(AgentFailure::StorageUnavailable); }
    let observation = observation_by_id(transaction, person_id, stage.observation_id)
        .await?.ok_or(AgentFailure::StorageUnavailable)?;
    validate_memory_stage_replay(request, &receipt, &observation, &stage)?;
    Ok(Some(stage))
}

async fn memory_stage_receipt_by_key(
    transaction: &turso::transaction::Transaction<'_>,
    person_id: floe_kernel::PersonId,
    candidate_key: &str,
) -> Result<Option<MemoryStageReceipt>, AgentFailure> {
    let mut rows = transaction.query(
        "SELECT person_id, candidate_key, candidate_id, observation_id, payload FROM knowledge_stage_receipts WHERE person_id = ? AND candidate_key = ?",
        (person_id.to_string(), candidate_key.to_owned()),
    ).await.map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else { return Ok(None); };
    let stored_person = row.get::<String>(0).map_err(storage)?;
    let stored_key = row.get::<String>(1).map_err(storage)?;
    let stored_candidate = row.get::<String>(2).map_err(storage)?;
    let stored_observation = row.get::<String>(3).map_err(storage)?;
    let encoded = row.get::<String>(4).map_err(storage)?;
    if rows.next().await.map_err(storage)?.is_some() { return Err(AgentFailure::VaultUnavailable); }
    drop(rows);
    if encoded.len() > MAX_LEARNER_JOB_BYTES { return Err(AgentFailure::BudgetExceeded); }
    let receipt: MemoryStageReceipt = decode(&encoded)?;
    if stored_person != person_id.to_string() || stored_key != candidate_key
        || stored_candidate != receipt.candidate_id.to_string()
        || stored_observation != receipt.observation_id.to_string()
        || receipt.person_id != person_id
    { return Err(AgentFailure::StorageUnavailable); }
    Ok(Some(receipt))
}

fn validate_observation_identity(
    observation: &LearningObservation,
    person_id: floe_kernel::PersonId,
    request: &StageMemoryCandidate,
    expected_hash: &str,
    source_refs: &[LearningEvidenceRef],
) -> Result<(), AgentFailure> {
    if observation.schema_version != floe_knowledge::KNOWLEDGE_VERSION
        || observation.id.is_nil() || observation.person_id != person_id
        || observation.session_id != request.session_id
        || observation.evidence != source_refs || observation.outcome != LearningOutcome::Completed
        || observation.kind != request.observation_kind
        || observation.digest != request.digest.trim()
        || observation.content_hash != expected_hash
    { return Err(AgentFailure::StorageUnavailable); }
    Ok(())
}

async fn observation_by_hash(
    transaction: &turso::transaction::Transaction<'_>,
    person_id: floe_kernel::PersonId,
    content_hash: &str,
) -> Result<Option<LearningObservation>, AgentFailure> {
    let mut rows = transaction.query(
        "SELECT id, person_id, content_hash, payload FROM learning_observations WHERE person_id = ? AND content_hash = ?",
        (person_id.to_string(), content_hash.to_owned()),
    ).await.map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else { return Ok(None); };
    let stored_id = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
        .map_err(unavailable)?;
    let stored_person = row.get::<String>(1).map_err(storage)?;
    let stored_hash = row.get::<String>(2).map_err(storage)?;
    let encoded = row.get::<String>(3).map_err(storage)?;
    if encoded.len() > MAX_LEARNER_JOB_BYTES { return Err(AgentFailure::StorageUnavailable); }
    let observation: LearningObservation = decode(&encoded)?;
    if rows.next().await.map_err(storage)?.is_some()
        || stored_id != observation.id || stored_person != person_id.to_string()
        || stored_hash != content_hash || observation.person_id != person_id
        || observation.content_hash != content_hash
    { return Err(AgentFailure::VaultUnavailable); }
    Ok(Some(observation))
}

async fn observation_by_id(
    transaction: &turso::transaction::Transaction<'_>,
    person_id: floe_kernel::PersonId,
    observation_id: Uuid,
) -> Result<Option<LearningObservation>, AgentFailure> {
    let mut rows = transaction.query(
        "SELECT id, person_id, content_hash, payload FROM learning_observations WHERE person_id = ? AND id = ?",
        (person_id.to_string(), observation_id.to_string()),
    ).await.map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else { return Ok(None); };
    let stored_id = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
        .map_err(unavailable)?;
    let stored_person = row.get::<String>(1).map_err(storage)?;
    let stored_hash = row.get::<String>(2).map_err(storage)?;
    let encoded = row.get::<String>(3).map_err(storage)?;
    if encoded.len() > MAX_LEARNER_JOB_BYTES { return Err(AgentFailure::StorageUnavailable); }
    let observation: LearningObservation = decode(&encoded)?;
    if rows.next().await.map_err(storage)?.is_some()
        || stored_id != observation_id || stored_person != person_id.to_string()
        || observation.id != observation_id || observation.person_id != person_id
        || observation.content_hash != stored_hash
    { return Err(AgentFailure::VaultUnavailable); }
    Ok(Some(observation))
}

async fn active_revision_on(
    transaction: &turso::transaction::Transaction<'_>,
    person_id: floe_kernel::PersonId,
    target_id: Uuid,
) -> Result<Option<KnowledgeRevision>, AgentFailure> {
    let mut rows = transaction.query(
        "SELECT target_id, revision, person_id, kind, state, payload FROM knowledge_revisions WHERE target_id = ? AND person_id = ? AND kind = 'memory' AND state = 'active'",
        (target_id.to_string(), person_id.to_string()),
    ).await.map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else { return Ok(None); };
    let revision = decode_revision_row(&row, person_id, KnowledgeRevisionState::Active)?;
    if rows.next().await.map_err(storage)?.is_some() { return Err(AgentFailure::VaultUnavailable); }
    Ok(Some(revision))
}

async fn revision_exists_on(
    transaction: &turso::transaction::Transaction<'_>,
    target_id: Uuid,
) -> Result<bool, AgentFailure> {
    let mut rows = transaction.query(
        "SELECT 1 FROM knowledge_revisions WHERE target_id = ? LIMIT 1",
        [target_id.to_string()],
    ).await.map_err(storage)?;
    Ok(rows.next().await.map_err(storage)?.is_some())
}

async fn evidence_is_independent(
    transaction: &turso::transaction::Transaction<'_>,
    person_id: floe_kernel::PersonId,
    evidence: &[LearningEvidenceRef],
) -> Result<bool, AgentFailure> {
    if evidence.is_empty() { return Ok(false); }
    for reference in evidence {
        if reference.session_id.is_nil() || reference.turn_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        if !matches!(
            super::context_dependencies::read_context_dependency_coverage(
                transaction,
                person_id,
                reference.session_id,
                reference.turn_id,
            ).await?,
            DependencyCoverage::Independent
        ) { return Ok(false); }
    }
    Ok(true)
}

struct TransactionLearningEvidence<'reader, 'connection, Keys: VaultKeyProvider> {
    vault: &'reader EncryptedAgentVault<Keys>,
    transaction: &'reader turso::transaction::Transaction<'connection>,
}

impl<Keys: VaultKeyProvider> floe_knowledge::EvidenceReader
    for TransactionLearningEvidence<'_, '_, Keys>
{
    async fn read_learning_evidence(
        &self,
        person_id: floe_kernel::PersonId,
        session_id: Uuid,
        turn_ids: &[Uuid],
    ) -> Result<LearningEvidenceSnapshot, AgentFailure> {
        self.vault.check_access()?;
        if person_id != self.vault.person_id { return Err(AgentFailure::PolicyDenied); }
        let session = self.vault.session_on(self.transaction, session_id).await?;
        let mut coverage = if turn_ids.is_empty() { DependencyCoverage::Unknown } else { DependencyCoverage::Independent };
        for turn_id in turn_ids {
            let current = super::context_dependencies::read_context_dependency_coverage(
                self.transaction,
                person_id,
                session_id,
                *turn_id,
            ).await?;
            coverage = match coverage.merge(&current) {
                Ok(merged) => merged,
                // Historical turns may name different epochs of one source.
                // Their union is ineligible for learning, not Vault corruption.
                Err(floe_context_contract::ContextDependencyError::Conflict) => DependencyCoverage::Unknown,
                Err(floe_context_contract::ContextDependencyError::TooLarge
                    | floe_context_contract::ContextDependencyError::DependencyCount) => return Err(AgentFailure::BudgetExceeded),
                Err(_) => return Err(AgentFailure::VaultUnavailable),
            };
        }
        let mut actual_turn_ids = session.messages.iter()
            .filter(|message| !matches!(message, floe_conversation::AgentMessage::Compaction { .. }))
            .map(floe_conversation::AgentMessage::turn_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        actual_turn_ids.sort_unstable();
        self.vault.check_access()?;
        Ok(LearningEvidenceSnapshot {
            person_id: session.person_id,
            session_id: session.id,
            revision: session.revision,
            outcome: session.last_outcome.map(learning_outcome),
            personal: session.scope.is_none() && session.data_classes == [DataClass::Personal],
            active_turn: session.active_turn.is_some(),
            pending_output: session.pending_output.is_some(),
            turn_ids: actual_turn_ids,
            coverage,
            purpose: floe_knowledge::EvidenceProjectionPurpose::Learning,
        })
    }
}

async fn validate_learner_source<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    transaction: &turso::transaction::Transaction<'_>,
    input: &LearnerReviewInput,
) -> Result<(), AgentFailure> {
    let reader = TransactionLearningEvidence { vault, transaction };
    floe_knowledge::admit_learning_evidence(
        &reader,
        input.person_id,
        input.session_id,
        input.session_revision,
        &input.turn_ids,
    ).await.map_err(|failure| match failure {
        AgentFailure::Conflict => AgentFailure::StaleContext,
        other => other,
    })?;
    for expected in &input.current_memories {
        let revision = active_revision_on(transaction, input.person_id, expected.target_id)
            .await?.ok_or(AgentFailure::StaleContext)?;
        if revision.revision != expected.revision { return Err(AgentFailure::StaleContext); }
        let independent = evidence_is_independent(transaction, input.person_id, &revision.source_refs).await?;
        let projected = project_memory_context(input.person_id,
            vec![MemoryContextFact { revision, evidence_independent: independent }], input.observed_at)?;
        if projected.memories.as_slice() != std::slice::from_ref(expected) {
            return Err(AgentFailure::StaleContext);
        }
    }
    Ok(())
}

fn learning_outcome(outcome: AgentOutcome) -> LearningOutcome {
    match outcome {
        AgentOutcome::Completed => LearningOutcome::Completed,
        AgentOutcome::Blocked { run_id, review_group_id } => LearningOutcome::Blocked { run_id, review_group_id },
        AgentOutcome::Halted { reason } => LearningOutcome::Halted { reason },
    }
}

struct MemoryDecisionReceipt {
    candidate_id: Uuid,
    kind: KnowledgeDecisionKind,
    result: KnowledgeDecisionResult,
}

async fn knowledge_decision_receipt(
    transaction: &turso::transaction::Transaction<'_>,
    command_id: CommandId,
    person_id: floe_kernel::PersonId,
    device_id: &str,
) -> Result<Option<MemoryDecisionReceipt>, AgentFailure> {
    let mut rows = transaction.query(
        "SELECT person_id, device_id, command_kind, candidate_id, decision_kind, payload FROM knowledge_command_receipts WHERE command_id = ?",
        [command_id.as_uuid().to_string()],
    ).await.map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else { return Ok(None); };
    let stored_person = row.get::<String>(0).map_err(storage)?;
    let stored_device = row.get::<String>(1).map_err(storage)?;
    let command_kind = row.get::<String>(2).map_err(storage)?;
    let candidate_id = Uuid::parse_str(&row.get::<String>(3).map_err(storage)?)
        .map_err(unavailable)?;
    let decision_kind = row.get::<String>(4).map_err(storage)?;
    let encoded = row.get::<String>(5).map_err(storage)?;
    if encoded.len() > MAX_LEARNER_JOB_BYTES { return Err(AgentFailure::StorageUnavailable); }
    let result: KnowledgeDecisionResult = decode(&encoded)?;
    if rows.next().await.map_err(storage)?.is_some() { return Err(AgentFailure::VaultUnavailable); }
    if command_kind != "memory_decision" || stored_person != person_id.to_string()
        || stored_device != device_id
    { return Err(AgentFailure::Conflict); }
    if result.candidate.person_id != person_id || result.candidate.id != candidate_id
        || result.decision.candidate_id != candidate_id
        || result.decision.decision != decision_kind_value(&decision_kind)?
        || result.decision.actor != KnowledgeActor::User
    { return Err(AgentFailure::StorageUnavailable); }
    floe_knowledge::project_memory_decision(command_id, &result)?;
    Ok(Some(MemoryDecisionReceipt {
        candidate_id,
        kind: decision_kind_value(&decision_kind)?,
        result,
    }))
}

fn decision_kind_value(value: &str) -> Result<KnowledgeDecisionKind, AgentFailure> {
    match value {
        "approve" => Ok(KnowledgeDecisionKind::Approve),
        "reject" => Ok(KnowledgeDecisionKind::Reject),
        _ => Err(AgentFailure::VaultUnavailable),
    }
}

async fn candidate_by_id(
    transaction: &turso::transaction::Transaction<'_>,
    person_id: floe_kernel::PersonId,
    candidate_id: Uuid,
) -> Result<KnowledgeCandidate, AgentFailure> {
    let mut rows = transaction.query(
        "SELECT id, person_id, idempotency_key, kind, state, target_id, created_at, payload FROM knowledge_candidates WHERE id = ? AND person_id = ?",
        (candidate_id.to_string(), person_id.to_string()),
    ).await.map_err(storage)?;
    let row = rows.next().await.map_err(storage)?.ok_or(AgentFailure::NotFound)?;
    let candidate = decode_candidate_row(&row, person_id)?;
    if rows.next().await.map_err(storage)?.is_some() { return Err(AgentFailure::VaultUnavailable); }
    if candidate.id != candidate_id { return Err(AgentFailure::VaultUnavailable); }
    Ok(candidate)
}

async fn learner_job_by_key(
    transaction: &turso::transaction::Transaction<'_>,
    person_id: floe_kernel::PersonId,
    idempotency_key: &str,
) -> Result<Option<LearnerReviewJob>, AgentFailure> {
    let mut rows = transaction.query(
        "SELECT id, person_id, idempotency_key, state, attempts, available_at, payload FROM learner_review_jobs WHERE person_id = ? AND idempotency_key = ?",
        (person_id.to_string(), idempotency_key.to_owned()),
    ).await.map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else { return Ok(None); };
    let job = decode_job_row(&row, person_id)?;
    if rows.next().await.map_err(storage)?.is_some() { return Err(AgentFailure::VaultUnavailable); }
    if job.idempotency_key != idempotency_key { return Err(AgentFailure::VaultUnavailable); }
    Ok(Some(job))
}

async fn learner_job_by_id(
    transaction: &turso::transaction::Transaction<'_>,
    person_id: floe_kernel::PersonId,
    job_id: Uuid,
) -> Result<Option<LearnerReviewJob>, AgentFailure> {
    let mut rows = transaction.query(
        "SELECT id, person_id, idempotency_key, state, attempts, available_at, payload FROM learner_review_jobs WHERE id = ? AND person_id = ?",
        (job_id.to_string(), person_id.to_string()),
    ).await.map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else { return Ok(None); };
    let job = decode_job_row(&row, person_id)?;
    if rows.next().await.map_err(storage)?.is_some() { return Err(AgentFailure::VaultUnavailable); }
    if job.id != job_id { return Err(AgentFailure::VaultUnavailable); }
    Ok(Some(job))
}

fn decode_job_row(
    row: &turso::Row,
    person_id: floe_kernel::PersonId,
) -> Result<LearnerReviewJob, AgentFailure> {
    let id = Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
        .map_err(unavailable)?;
    let stored_person = row.get::<String>(1).map_err(storage)?;
    let stored_key = row.get::<String>(2).map_err(storage)?;
    let stored_state = row.get::<String>(3).map_err(storage)?;
    let stored_attempts = row.get::<i64>(4).map_err(storage)?;
    let stored_available = row.get::<String>(5).map_err(storage)?;
    let encoded = row.get::<String>(6).map_err(storage)?;
    if encoded.len() > MAX_LEARNER_JOB_BYTES { return Err(AgentFailure::BudgetExceeded); }
    let job: LearnerReviewJob = decode(&encoded)?;
    validate_learner_job_rowless(&job, person_id)?;
    if stored_person != person_id.to_string() || job.id != id || job.idempotency_key != stored_key
        || learner_job_state(job.state) != stored_state
        || i64::from(job.attempts) != stored_attempts
        || timestamp(job.available_at) != stored_available
    { return Err(AgentFailure::VaultUnavailable); }
    Ok(job)
}

fn validate_learner_job_rowless(
    job: &LearnerReviewJob,
    person_id: floe_kernel::PersonId,
) -> Result<(), AgentFailure> {
    validate_learner_input(&job.input, person_id).map_err(|_| AgentFailure::VaultUnavailable)?;
    floe_knowledge::validate_learner_job_lifecycle(&job.lifecycle())?;
    if job.schema_version != floe_knowledge::KNOWLEDGE_VERSION || job.id.is_nil()
        || job.input.run_id != job.id || job.idempotency_key.is_empty()
        || learner_job_key(&job.input).map_err(|_| AgentFailure::VaultUnavailable)? != job.idempotency_key
    { return Err(AgentFailure::VaultUnavailable); }
    Ok(())
}

async fn update_job_cas(
    transaction: &turso::transaction::Transaction<'_>,
    job: &LearnerReviewJob,
    person_id: floe_kernel::PersonId,
    old_payload: &str,
) -> Result<bool, AgentFailure> {
    let changed = transaction.execute(
        "UPDATE learner_review_jobs SET state = ?, attempts = ?, available_at = ?, payload = ? WHERE id = ? AND person_id = ? AND payload = ?",
        (learner_job_state(job.state), i64::from(job.attempts), timestamp(job.available_at), payload(job)?, job.id.to_string(), person_id.to_string(), old_payload.to_owned()),
    ).await.map_err(storage)?;
    Ok(changed == 1)
}

async fn update_running_job_cas(
    transaction: &turso::transaction::Transaction<'_>,
    job: &LearnerReviewJob,
    person_id: floe_kernel::PersonId,
    expected_attempt: u8,
    old_payload: &str,
) -> Result<bool, AgentFailure> {
    let changed = transaction.execute(
        "UPDATE learner_review_jobs SET state = ?, attempts = ?, available_at = ?, payload = ? WHERE id = ? AND person_id = ? AND state = 'running' AND attempts = ? AND payload = ?",
        (learner_job_state(job.state), i64::from(job.attempts), timestamp(job.available_at), payload(job)?, job.id.to_string(), person_id.to_string(), i64::from(expected_attempt), old_payload.to_owned()),
    ).await.map_err(storage)?;
    Ok(changed == 1)
}

async fn settlement_receipt(
    transaction: &turso::transaction::Transaction<'_>,
    job_id: Uuid,
    person_id: floe_kernel::PersonId,
    attempt: u8,
) -> Result<Option<(String, String, String)>, AgentFailure> {
    let mut rows = transaction.query(
        "SELECT device_id, settlement, result FROM learner_settlement_receipts WHERE job_id = ? AND person_id = ? AND claim_attempt = ?",
        (job_id.to_string(), person_id.to_string(), i64::from(attempt)),
    ).await.map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else { return Ok(None); };
    let receipt = (
        row.get::<String>(0).map_err(storage)?,
        row.get::<String>(1).map_err(storage)?,
        row.get::<String>(2).map_err(storage)?,
    );
    if rows.next().await.map_err(storage)?.is_some() || receipt.0.len() > 128
        || receipt.1.len() > MAX_LEARNER_EVENT_BYTES || receipt.2.len() > MAX_LEARNER_JOB_BYTES
    { return Err(AgentFailure::VaultUnavailable); }
    Ok(Some(receipt))
}

async fn learner_journal_head(
    transaction: &turso::transaction::Transaction<'_>,
    job_id: Uuid,
    person_id: floe_kernel::PersonId,
    attempt: u8,
) -> Result<Option<LearnerJournalHead>, AgentFailure> {
    let mut rows = transaction.query(
        "SELECT job_id, person_id, claim_attempt, device_id, journal_revision, journal_digest, payload FROM learner_journal_heads WHERE job_id = ? AND person_id = ? AND claim_attempt = ?",
        (job_id.to_string(), person_id.to_string(), i64::from(attempt)),
    ).await.map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else { return Ok(None); };
    let stored_job = row.get::<String>(0).map_err(storage)?;
    let stored_person = row.get::<String>(1).map_err(storage)?;
    let stored_attempt = row.get::<i64>(2).map_err(storage)?;
    let stored_device = row.get::<String>(3).map_err(storage)?;
    let stored_revision = row.get::<i64>(4).map_err(storage)?;
    let stored_digest = row.get::<String>(5).map_err(storage)?;
    let encoded_head = row.get::<String>(6).map_err(storage)?;
    if encoded_head.len() > 4096 { return Err(AgentFailure::StorageUnavailable); }
    let head: LearnerJournalHead = decode(&encoded_head)?;
    if rows.next().await.map_err(storage)?.is_some()
        || stored_job != job_id.to_string() || stored_person != person_id.to_string()
        || stored_attempt != i64::from(attempt) || stored_device != head.device_id
        || stored_revision != i64::try_from(head.journal_revision).map_err(|_| AgentFailure::StorageUnavailable)?
        || stored_digest != digest_hex(&head.journal_digest)
        || head.claim.job_id != job_id || head.claim.claim_attempt != attempt
        || head.person_id != person_id
    { return Err(AgentFailure::StorageUnavailable); }
    Ok(Some(head))
}

async fn load_all_learner_journals(
    transaction: &turso::transaction::Transaction<'_>,
    job: &LearnerReviewJob,
) -> Result<HashMap<u8, LearnerClaimJournal>, AgentFailure> {
    let mut heads = HashMap::new();
    let mut head_rows = transaction.query(
        "SELECT job_id, person_id, claim_attempt, device_id, journal_revision, journal_digest, payload FROM learner_journal_heads WHERE job_id = ? ORDER BY claim_attempt",
        [job.id.to_string()],
    ).await.map_err(storage)?;
    while let Some(row) = head_rows.next().await.map_err(storage)? {
        let stored_job = row.get::<String>(0).map_err(storage)?;
        let stored_person = row.get::<String>(1).map_err(storage)?;
        let attempt = u8::try_from(row.get::<i64>(2).map_err(storage)?)
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let stored_device = row.get::<String>(3).map_err(storage)?;
        let stored_revision = row.get::<i64>(4).map_err(storage)?;
        let stored_digest = row.get::<String>(5).map_err(storage)?;
        let encoded_head = row.get::<String>(6).map_err(storage)?;
        if encoded_head.len() > 4096 { return Err(AgentFailure::StorageUnavailable); }
        let head: LearnerJournalHead = decode(&encoded_head)?;
        if stored_job != job.id.to_string() || stored_person != job.input.person_id.to_string()
            || attempt == 0 || attempt > job.attempts || stored_device != head.device_id
            || stored_revision != i64::try_from(head.journal_revision).map_err(|_| AgentFailure::StorageUnavailable)?
            || stored_digest != digest_hex(&head.journal_digest)
            || head.claim.job_id != job.id || head.claim.claim_attempt != attempt
            || head.person_id != job.input.person_id
            || heads.insert(attempt, head).is_some()
        { return Err(AgentFailure::StorageUnavailable); }
    }
    drop(head_rows);
    if heads.len() != usize::from(job.attempts) {
        return Err(AgentFailure::StorageUnavailable);
    }

    let mut entry_map: HashMap<u8, Vec<JournalEntry>> = HashMap::new();
    let mut entry_bytes: HashMap<u8, usize> = HashMap::new();
    let mut event_keys = HashSet::new();
    let mut event_rows = transaction.query(
        "SELECT person_id, claim_attempt, sequence, event_key, payload FROM learner_execution_journal WHERE job_id = ? ORDER BY claim_attempt, sequence",
        [job.id.to_string()],
    ).await.map_err(storage)?;
    while let Some(row) = event_rows.next().await.map_err(storage)? {
        let stored_person = row.get::<String>(0).map_err(storage)?;
        let attempt = u8::try_from(row.get::<i64>(1).map_err(storage)?)
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let sequence = u64::try_from(row.get::<i64>(2).map_err(storage)?)
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let event_key = row.get::<String>(3).map_err(storage)?;
        let encoded = row.get::<String>(4).map_err(storage)?;
        if attempt == 0 || attempt > job.attempts || event_key.len() > 256
            || encoded.len() > MAX_LEARNER_EVENT_BYTES
            || stored_person != job.input.person_id.to_string()
            || !event_keys.insert((attempt, event_key.clone()))
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let event: JournalEvent = decode(&encoded)?;
        let canonical_key = learner_event_key(
            LearnerClaimRef { job_id: job.id, claim_attempt: attempt },
            &event,
        ).map_err(|_| AgentFailure::StorageUnavailable)?;
        if event_key != canonical_key { return Err(AgentFailure::StorageUnavailable); }
        let entries = entry_map.entry(attempt).or_default();
        let total_bytes = entry_bytes.entry(attempt).or_default();
        if entries.len() >= 64 || (*total_bytes).saturating_add(encoded.len()) > 512 * 1024 {
            return Err(AgentFailure::StorageUnavailable);
        }
        *total_bytes += encoded.len();
        let expected = u64::try_from(entries.len()).map_err(|_| AgentFailure::StorageUnavailable)?
            .checked_add(1).ok_or(AgentFailure::StorageUnavailable)?;
        if sequence != expected { return Err(AgentFailure::StorageUnavailable); }
        entries.push(JournalEntry { revision: sequence, event });
    }
    drop(event_rows);

    let mut journals = HashMap::new();
    for attempt in 1..=job.attempts {
        let head = heads.remove(&attempt).ok_or(AgentFailure::StorageUnavailable)?;
        let entries = entry_map.remove(&attempt).unwrap_or_default();
        if usize::try_from(head.journal_revision).map_err(|_| AgentFailure::StorageUnavailable)? != entries.len() {
            return Err(AgentFailure::StorageUnavailable);
        }
        validate_learner_journal(&head, &entries)?;
        let journal = LearnerClaimJournal { head, entries };
        if attempt < job.attempts || (attempt == job.attempts && job.state == LearnerJobState::Deferred) {
            floe_knowledge::validate_learner_deferred_journal(&journal)?;
            let (device, encoded_settlement, encoded_result) = settlement_receipt(
                transaction, job.id, job.input.person_id, attempt,
            ).await?.ok_or(AgentFailure::StorageUnavailable)?;
            let StoredLearnerSettlement::Deferred { available_at, failure } = decode(&encoded_settlement)?
                else { return Err(AgentFailure::StorageUnavailable); };
            let settled: LearnerReviewJob = decode(&encoded_result)?;
            validate_learner_job_rowless(&settled, job.input.person_id)?;
            if device != journal.head.device_id || settled.id != job.id || settled.attempts != attempt
                || settled.input != job.input || settled.idempotency_key != job.idempotency_key
                || settled.state != LearnerJobState::Deferred
                || settled.claimed_device_id.as_deref() != Some(device.as_str())
                || (attempt == job.attempts && settled != *job)
            { return Err(AgentFailure::StorageUnavailable); }
            validate_settlement_result(&LearnerJobSettlement::Deferred { available_at, failure }, &settled)?;
        }
        journals.insert(attempt, journal);
    }
    if !entry_map.is_empty() || !heads.is_empty() { return Err(AgentFailure::StorageUnavailable); }
    if let Some(latest) = journals.get(&job.attempts) {
        if job.claimed_device_id.as_deref() != Some(latest.head.device_id.as_str()) {
            return Err(AgentFailure::StorageUnavailable);
        }
    }
    Ok(journals)
}

fn learner_event_key(claim: LearnerClaimRef, event: &JournalEvent) -> Result<String, AgentFailure> {
    let key = match event {
        JournalEvent::ModelIntent { attempt_id, .. } if !attempt_id.is_nil() => format!("model-intent:{attempt_id}"),
        JournalEvent::ModelResult { attempt_id, .. } if !attempt_id.is_nil() => format!("model-result:{attempt_id}"),
        JournalEvent::Output { .. } => format!("output:{}:{}", claim.claim_attempt, hash(event)?),
        JournalEvent::Checkpoint { iteration } => format!("checkpoint:{}:{iteration}", claim.claim_attempt),
        JournalEvent::ValidatedBatch { .. } => format!("validated-batch:{}:{}", claim.claim_attempt, hash(event)?),
        JournalEvent::BatchProgress { .. } => format!("batch-progress:{}:{}", claim.claim_attempt, hash(event)?),
        JournalEvent::ModelIntent { .. } | JournalEvent::ModelResult { .. } => return Err(AgentFailure::InvalidInput),
        _ => return Err(AgentFailure::CapabilityDenied),
    };
    Ok(key)
}
