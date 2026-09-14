use std::collections::HashSet;

use chrono::{DateTime, Utc};
use floe_agent::{AgentFailure, AgentOutcome, DataClass};
use floe_knowledge::{
    ContextMemory, KNOWLEDGE_VERSION, KnowledgeActor, KnowledgeCandidate, KnowledgeCandidateState,
    KnowledgeDecisionKind, KnowledgeDecisionResult, KnowledgeKind, KnowledgeMutation,
    KnowledgeOperation, KnowledgePayload, KnowledgeRevision, KnowledgeRevisionState,
    LearnerJobClaim, LearnerJobLifecycle, LearnerJobSettlement, LearnerJobState,
    LearnerReviewInput, LearnerReviewJob, LearningEvidenceRef, LearningEvidenceSnapshot,
    LearningObservation, LearningObservationKind, LearningOutcome, MAX_CONTEXT_MEMORIES,
    MAX_CONTEXT_MEMORY_BYTES, MemoryOverviewSnapshot, MemoryReviewSnapshot, StageMemoryCandidate,
    admit_learning_evidence, claim_learner_job, project_memory_summary, reject_learner_claim,
    settle_learner_job, validate_learner_input, validate_learner_job_lifecycle,
    validate_memory_overview_limit, validate_memory_review_candidate, validate_stage_request,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use turso::transaction::TransactionBehavior;
use uuid::Uuid;

use super::*;

const MAX_LEARNER_DISCOVERY_JOBS: usize = 8;
const MAX_LEARNER_DISCOVERY_SESSIONS: i64 = 64;
const MAX_LEARNER_DIGEST_TEXT_BYTES: usize = 1536;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn initialize_learning_store(&self) -> Result<(), AgentFailure> {
        let connection = self.connection()?;
        connection.execute(
            "CREATE TABLE IF NOT EXISTS learning_observations (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, content_hash TEXT NOT NULL, payload TEXT NOT NULL, UNIQUE(person_id, content_hash))",
            (),
        ).await.map_err(storage)?;
        connection.execute(
            "CREATE TABLE IF NOT EXISTS knowledge_candidates (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, idempotency_key TEXT NOT NULL, state TEXT NOT NULL, target_id TEXT, created_at TEXT NOT NULL, payload TEXT NOT NULL, UNIQUE(person_id, idempotency_key))",
            (),
        ).await.map_err(storage)?;
        connection.execute(
            "CREATE INDEX IF NOT EXISTS knowledge_candidates_review ON knowledge_candidates(person_id, state, created_at)",
            (),
        ).await.map_err(storage)?;
        connection.execute(
            "CREATE TABLE IF NOT EXISTS knowledge_candidate_decisions (id TEXT PRIMARY KEY, candidate_id TEXT NOT NULL UNIQUE, payload TEXT NOT NULL)",
            (),
        ).await.map_err(storage)?;
        connection.execute(
            "CREATE TABLE IF NOT EXISTS knowledge_revisions (target_id TEXT NOT NULL, revision INTEGER NOT NULL, person_id TEXT NOT NULL, kind TEXT NOT NULL, state TEXT NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(target_id, revision))",
            (),
        ).await.map_err(storage)?;
        connection.execute(
            "CREATE INDEX IF NOT EXISTS knowledge_revisions_active ON knowledge_revisions(person_id, kind, state)",
            (),
        ).await.map_err(storage)?;
        connection.execute(
            "CREATE TABLE IF NOT EXISTS knowledge_mutations (id TEXT PRIMARY KEY, candidate_id TEXT NOT NULL UNIQUE, target_id TEXT NOT NULL, created_at TEXT NOT NULL, payload TEXT NOT NULL)",
            (),
        ).await.map_err(storage)?;
        connection.execute(
            "CREATE TABLE IF NOT EXISTS learner_review_jobs (id TEXT PRIMARY KEY, person_id TEXT NOT NULL, idempotency_key TEXT NOT NULL, state TEXT NOT NULL, attempts INTEGER NOT NULL, available_at TEXT NOT NULL, payload TEXT NOT NULL, UNIQUE(person_id, idempotency_key))",
            (),
        ).await.map_err(storage)?;
        connection.execute(
            "CREATE INDEX IF NOT EXISTS learner_review_jobs_ready ON learner_review_jobs(person_id, state, available_at)",
            (),
        ).await.map_err(storage)?;
        Ok(())
    }

    pub async fn stage_memory_candidate(
        &self,
        request: StageMemoryCandidate,
    ) -> Result<KnowledgeCandidate, AgentFailure> {
        validate_stage_request(&request)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let evidence_reader = TransactionLearningEvidence {
                vault: self,
                transaction: &transaction,
            };
            let snapshot = admit_learning_evidence(
                &evidence_reader,
                self.person_id,
                request.session_id,
                request.expected_session_revision,
                &request.turn_ids,
            )
            .await?;

            let source_refs = request
                .turn_ids
                .iter()
                .map(|turn_id| LearningEvidenceRef {
                    session_id: request.session_id,
                    turn_id: *turn_id,
                })
                .collect::<Vec<_>>();
            let observation_hash = hash(&(
                self.person_id,
                request.session_id,
                &source_refs,
                request.observation_kind,
                request.digest.trim(),
                snapshot.outcome,
            ))?;
            let observation = if let Some(existing) = observation_by_hash(
                &transaction,
                self.person_id,
                &observation_hash,
            )
            .await?
            {
                existing
            } else {
                let observation = LearningObservation {
                    schema_version: KNOWLEDGE_VERSION,
                    id: Uuid::new_v4(),
                    person_id: self.person_id,
                    session_id: request.session_id,
                    evidence: source_refs.clone(),
                    outcome: LearningOutcome::Completed,
                    kind: request.observation_kind,
                    digest: request.digest.trim().to_owned(),
                    observed_at: request.created_at,
                    content_hash: observation_hash,
                };
                transaction.execute(
                    "INSERT INTO learning_observations (id, person_id, content_hash, payload) VALUES (?, ?, ?, ?)",
                    (
                        observation.id.to_string(),
                        self.person_id.to_string(),
                        observation.content_hash.clone(),
                        payload(&observation)?,
                    ),
                ).await.map_err(storage)?;
                observation
            };

            let (operation, before_hash) = match (request.target_id, request.base_revision) {
                (None, None) => (KnowledgeOperation::Create, None),
                (Some(target_id), Some(base_revision)) => {
                    let current = active_revision_on(&transaction, self.person_id, target_id).await?;
                    if current.revision != base_revision || current.kind != KnowledgeKind::Memory {
                        return Err(AgentFailure::Conflict);
                    }
                    (KnowledgeOperation::Revise, Some(hash(&current.payload)?))
                }
                _ => return Err(AgentFailure::InvalidInput),
            };
            let candidate_payload = KnowledgePayload::Memory {
                value: request.value,
            };
            let after_hash = hash(&candidate_payload)?;
            let idempotency_key = hash(&(
                &observation.content_hash,
                request.extractor_version.trim(),
                request.prompt_version.trim(),
                KnowledgeKind::Memory,
                request.target_id,
            ))?;
            if let Some(existing) = candidate_by_key(
                &transaction,
                self.person_id,
                &idempotency_key,
            )
            .await?
            {
                self.check_access()?;
                return Ok(existing);
            }
            let candidate = KnowledgeCandidate {
                schema_version: KNOWLEDGE_VERSION,
                id: Uuid::new_v4(),
                person_id: self.person_id,
                observation_id: observation.id,
                idempotency_key,
                kind: KnowledgeKind::Memory,
                operation,
                target_id: request.target_id,
                base_revision: request.base_revision,
                payload: candidate_payload,
                before_hash,
                after_hash,
                source_refs,
                extractor_version: request.extractor_version.trim().to_owned(),
                prompt_version: request.prompt_version.trim().to_owned(),
                actor: request.actor,
                state: KnowledgeCandidateState::Pending,
                created_at: request.created_at,
            };
            transaction.execute(
                "INSERT INTO knowledge_candidates (id, person_id, idempotency_key, state, target_id, created_at, payload) VALUES (?, ?, ?, ?, ?, ?, ?)",
                (
                    candidate.id.to_string(),
                    self.person_id.to_string(),
                    candidate.idempotency_key.clone(),
                    state_name(candidate.state),
                    candidate.target_id.map(|id| id.to_string()),
                    candidate.created_at.to_rfc3339(),
                    payload(&candidate)?,
                ),
            ).await.map_err(storage)?;
            self.check_access()?;
            Ok(candidate)
        }
        .await;
        finish_transaction(transaction, result).await
    }

    pub async fn pending_knowledge_candidates(
        &self,
    ) -> Result<Vec<KnowledgeCandidate>, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection.query(
            "SELECT payload FROM knowledge_candidates WHERE person_id = ? AND state = 'pending' ORDER BY created_at, id",
            [self.person_id.to_string()],
        ).await.map_err(storage)?;
        let mut candidates = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let candidate: KnowledgeCandidate = decode(&row.get::<String>(0).map_err(storage)?)?;
            validate_candidate_owner(&candidate, self.person_id)?;
            candidates.push(candidate);
        }
        self.check_access()?;
        Ok(candidates)
    }

    pub async fn pending_memory_candidate_count(&self) -> Result<usize, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT COUNT(*) FROM knowledge_candidates WHERE person_id = ? AND state = 'pending' AND json_extract(payload, '$.kind') = 'memory'",
                [self.person_id.to_string()],
            )
            .await
            .map_err(storage)?;
        let count = rows
            .next()
            .await
            .map_err(storage)?
            .ok_or(AgentFailure::VaultUnavailable)?
            .get::<i64>(0)
            .map_err(storage)?;
        self.check_access()?;
        usize::try_from(count).map_err(|_| AgentFailure::VaultUnavailable)
    }

    pub async fn memory_review_snapshot(&self) -> Result<MemoryReviewSnapshot, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(storage)?;
        let result = async {
            let mut rows = transaction
                .query(
                    "SELECT payload FROM knowledge_candidates WHERE person_id = ? AND state = 'pending' AND json_extract(payload, '$.kind') = 'memory' ORDER BY created_at, id",
                    [self.person_id.to_string()],
                )
                .await
                .map_err(storage)?;
            let mut candidates = Vec::new();
            while let Some(row) = rows.next().await.map_err(storage)? {
                let candidate: KnowledgeCandidate =
                    decode(&row.get::<String>(0).map_err(storage)?)?;
                validate_memory_review_candidate(&candidate, self.person_id)?;
                candidates.push(candidate);
            }
            drop(rows);
            self.check_access()?;
            Ok(MemoryReviewSnapshot {
                person_id: self.person_id,
                candidates,
            })
        }
        .await;
        finish_transaction(transaction, result).await
    }

    pub async fn memory_overview_snapshot(
        &self,
        limit: usize,
    ) -> Result<MemoryOverviewSnapshot, AgentFailure> {
        validate_memory_overview_limit(limit)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(storage)?;
        let result = async {
            let mut saved_rows = transaction
                .query(
                    "SELECT COUNT(*) FROM knowledge_revisions WHERE person_id = ? AND kind = 'memory' AND state = 'active'",
                    [self.person_id.to_string()],
                )
                .await
                .map_err(storage)?;
            let saved_count = saved_rows
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::VaultUnavailable)?
                .get::<i64>(0)
                .map_err(storage)?;
            drop(saved_rows);
            let mut pending_rows = transaction
                .query(
                    "SELECT COUNT(*) FROM knowledge_candidates WHERE person_id = ? AND state = 'pending' AND json_extract(payload, '$.kind') = 'memory'",
                    [self.person_id.to_string()],
                )
                .await
                .map_err(storage)?;
            let pending_count = pending_rows
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::VaultUnavailable)?
                .get::<i64>(0)
                .map_err(storage)?;
            drop(pending_rows);
            let mut rows = transaction
                .query(
                    "SELECT payload FROM knowledge_revisions WHERE person_id = ? AND kind = 'memory' AND state = 'active' ORDER BY json_extract(payload, '$.created_at') DESC, target_id LIMIT ?",
                    (
                        self.person_id.to_string(),
                        i64::try_from(limit).map_err(|_| AgentFailure::InvalidInput)?,
                    ),
                )
                .await
                .map_err(storage)?;
            let mut memories = Vec::new();
            while let Some(row) = rows.next().await.map_err(storage)? {
                let revision: KnowledgeRevision =
                    decode(&row.get::<String>(0).map_err(storage)?)?;
                memories.push(project_memory_summary(&revision, self.person_id)?);
            }
            drop(rows);
            let saved_count = usize::try_from(saved_count)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            let pending_count = usize::try_from(pending_count)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            self.check_access()?;
            Ok(MemoryOverviewSnapshot {
                person_id: self.person_id,
                saved_count,
                pending_count,
                memories,
            })
        }
        .await;
        finish_transaction(transaction, result).await
    }

    pub async fn decide_knowledge_candidate(
        &self,
        candidate_id: Uuid,
        decision_kind: KnowledgeDecisionKind,
        actor: KnowledgeActor,
        decided_at: DateTime<Utc>,
    ) -> Result<KnowledgeDecisionResult, AgentFailure> {
        floe_knowledge::validate_review_actor(&actor)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let candidate = candidate_by_id(&transaction, self.person_id, candidate_id).await?;
            floe_knowledge::validate_review_candidate(&candidate)?;
            let admission = if decision_kind == KnowledgeDecisionKind::Approve {
                let independent = !candidate.source_refs.is_empty()
                    && evidence_is_independent(
                        &transaction,
                        self.person_id,
                        &candidate.source_refs,
                    )
                    .await?;
                if !independent {
                    return Err(AgentFailure::PolicyDenied);
                }
                floe_knowledge::validate_approval_candidate(&candidate)?;
                let target_id = candidate.target_id.unwrap_or_else(Uuid::new_v4);
                let current_revision = if candidate.operation == KnowledgeOperation::Revise {
                    Some(active_revision_on(&transaction, self.person_id, target_id).await?)
                } else {
                    None
                };
                let current_payload_hash = current_revision
                    .as_ref()
                    .map(|revision| hash(&revision.payload))
                    .transpose()?;
                let target_exists = if candidate.operation == KnowledgeOperation::Create {
                    revision_exists_on(&transaction, target_id).await?
                } else {
                    current_revision.is_some()
                };
                Some(floe_knowledge::ReviewAdmission {
                    target_id,
                    target_exists,
                    current_revision,
                    current_payload_hash,
                    evidence_independent: independent,
                })
            } else {
                None
            };
            let plan = floe_knowledge::plan_review(
                candidate,
                decision_kind,
                actor,
                decided_at,
                admission,
            )?;
            if let Some(superseded) = &plan.superseded {
                let changed = transaction.execute(
                    "UPDATE knowledge_revisions SET state = 'superseded', payload = ? WHERE target_id = ? AND revision = ? AND state = 'active'",
                    (
                        payload(superseded)?,
                        superseded.target_id.to_string(),
                        integer(superseded.revision)?,
                    ),
                ).await.map_err(storage)?;
                if changed != 1 {
                    return Err(AgentFailure::Conflict);
                }
            }
            let KnowledgeDecisionResult { candidate, decision, revision, mutation } = plan.result;
            if let Some(revision) = &revision {
                transaction.execute(
                    "INSERT INTO knowledge_revisions (target_id, revision, person_id, kind, state, payload) VALUES (?, ?, ?, ?, 'active', ?)",
                    (
                        revision.target_id.to_string(),
                        integer(revision.revision)?,
                        self.person_id.to_string(),
                        kind_name(revision.kind),
                        payload(revision)?,
                    ),
                ).await.map_err(storage)?;
            }
            if let Some(mutation) = &mutation {
                transaction.execute(
                    "INSERT INTO knowledge_mutations (id, candidate_id, target_id, created_at, payload) VALUES (?, ?, ?, ?, ?)",
                    (
                        mutation.id.to_string(),
                        candidate_id.to_string(),
                        mutation.target_id.to_string(),
                        decided_at.to_rfc3339(),
                        payload(mutation)?,
                    ),
                ).await.map_err(storage)?;
            }
            let changed = transaction.execute(
                "UPDATE knowledge_candidates SET state = ?, target_id = ?, payload = ? WHERE id = ? AND person_id = ? AND state = 'pending'",
                (
                    state_name(candidate.state),
                    candidate.target_id.map(|id| id.to_string()),
                    payload(&candidate)?,
                    candidate_id.to_string(),
                    self.person_id.to_string(),
                ),
            ).await.map_err(storage)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            transaction.execute(
                "INSERT INTO knowledge_candidate_decisions (id, candidate_id, payload) VALUES (?, ?, ?)",
                (
                    decision.id.to_string(),
                    candidate_id.to_string(),
                    payload(&decision)?,
                ),
            ).await.map_err(storage)?;
            self.check_access()?;
            Ok(KnowledgeDecisionResult {
                candidate,
                decision,
                revision,
                mutation,
            })
        }
        .await;
        finish_transaction(transaction, result).await
    }

    pub async fn active_personal_memories(&self) -> Result<Vec<KnowledgeRevision>, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection.query(
            "SELECT payload FROM knowledge_revisions WHERE person_id = ? AND kind = 'memory' AND state = 'active' ORDER BY target_id",
            [self.person_id.to_string()],
        ).await.map_err(storage)?;
        let mut revisions = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let revision: KnowledgeRevision = decode(&row.get::<String>(0).map_err(storage)?)?;
            validate_revision(&revision, self.person_id)?;
            revisions.push(revision);
        }
        self.check_access()?;
        Ok(revisions)
    }
    pub async fn personal_memory_context(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<ContextMemory>, AgentFailure> {
        let mut context = Vec::new();
        let mut total_bytes = 0usize;
        let mut budget_exceeded = false;
        for revision in self.active_personal_memories().await? {
            project_memory_summary(&revision, self.person_id)?;
            if !self.evidence_is_independent(&revision.source_refs).await? {
                continue;
            }
            let KnowledgePayload::Memory { value } = revision.payload else {
                return Err(AgentFailure::VaultUnavailable);
            };
            if value.valid_from.is_some_and(|valid_from| valid_from > now)
                || value
                    .valid_until
                    .is_some_and(|valid_until| valid_until <= now)
            {
                continue;
            }
            total_bytes = total_bytes.saturating_add(value.statement.len());
            if context.len() >= MAX_CONTEXT_MEMORIES || total_bytes > MAX_CONTEXT_MEMORY_BYTES {
                budget_exceeded = true;
                continue;
            }
            context.push(ContextMemory {
                target_id: revision.target_id,
                revision: revision.revision,
                kind: value.kind,
                statement: value.statement,
                epistemic_status: value.epistemic_status,
                confidence_millis: value.confidence_millis,
                observed_at_unix_ms: value.observed_at.timestamp_millis(),
                valid_from_unix_ms: value.valid_from.map(|time| time.timestamp_millis()),
                valid_until_unix_ms: value.valid_until.map(|time| time.timestamp_millis()),
                source_refs: revision.source_refs,
            });
        }
        self.check_access()?;
        if budget_exceeded {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(context)
    }

    async fn evidence_is_independent(
        &self,
        evidence: &[LearningEvidenceRef],
    ) -> Result<bool, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = evidence_is_independent(&transaction, self.person_id, evidence).await;
        finish_transaction(transaction, result).await
    }

    pub async fn knowledge_mutations(
        &self,
        target_id: Uuid,
    ) -> Result<Vec<KnowledgeMutation>, AgentFailure> {
        let connection = self.connection()?;
        let mut rows = connection.query(
            "SELECT payload FROM knowledge_mutations WHERE target_id = ? ORDER BY created_at, id",
            [target_id.to_string()],
        ).await.map_err(storage)?;
        let mut mutations = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let mutation: KnowledgeMutation = decode(&row.get::<String>(0).map_err(storage)?)?;
            mutations.push(mutation);
        }
        self.check_access()?;
        Ok(mutations)
    }

    pub async fn enqueue_learner_review(
        &self,
        mut input: LearnerReviewInput,
        available_at: DateTime<Utc>,
    ) -> Result<LearnerReviewJob, AgentFailure> {
        validate_learner_input(&input, self.person_id)?;
        input.digest = input.digest.trim().to_owned();
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let idempotency_key = hash(&(
                self.person_id,
                input.session_id,
                input.session_revision,
                &input.turn_ids,
                input.outcome,
                &input.digest,
            ))?;
            if let Some(job) = learner_job_by_key(&transaction, self.person_id, &idempotency_key).await? {
                return Ok(job);
            }
            validate_learner_source(self, &transaction, &input).await?;
            let job_id = Uuid::new_v4();
            input.run_id = job_id;
            let job = LearnerReviewJob {
                schema_version: KNOWLEDGE_VERSION,
                id: job_id,
                idempotency_key,
                input,
                state: LearnerJobState::Queued,
                attempts: 0,
                available_at,
                claimed_at: None,
                finished_at: None,
                candidate_id: None,
                last_failure: None,
            };
            transaction.execute(
                "INSERT INTO learner_review_jobs (id, person_id, idempotency_key, state, attempts, available_at, payload) VALUES (?, ?, ?, 'queued', 0, ?, ?)",
                (
                    job.id.to_string(),
                    self.person_id.to_string(),
                    job.idempotency_key.clone(),
                    timestamp(job.available_at),
                    payload(&job)?,
                ),
            ).await.map_err(storage)?;
            self.check_access()?;
            Ok(job)
        }.await;
        finish_transaction(transaction, result).await
    }

    pub async fn discover_explicit_learner_reviews(
        &self,
        now: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<LearnerReviewJob>, AgentFailure> {
        if limit == 0 || limit > MAX_LEARNER_DISCOVERY_JOBS {
            return Err(AgentFailure::InvalidInput);
        }
        let connection = self.connection()?;
        let mut rows = connection
            .query(
                "SELECT payload FROM agent_sessions ORDER BY rowid DESC LIMIT ?",
                [MAX_LEARNER_DISCOVERY_SESSIONS],
            )
            .await
            .map_err(storage)?;
        let mut eligible = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let session: floe_agent::AgentSession =
                decode(&row.get::<String>(0).map_err(storage)?)?;
            if session.person_id != self.person_id {
                return Err(AgentFailure::VaultUnavailable);
            }
            if session.scope.is_some()
                || session.data_classes != [DataClass::Personal]
                || session.active_turn.is_some()
                || session.pending_output.is_some()
                || session.last_outcome != Some(AgentOutcome::Completed)
            {
                continue;
            }
            let Some((turn_id, user_text, assistant_text, signal)) =
                explicit_learning_turn(&session)
            else {
                continue;
            };
            eligible.push((session, turn_id, user_text, assistant_text, signal));
            if eligible.len() == limit {
                break;
            }
        }
        drop(rows);
        self.check_access()?;
        if eligible.is_empty() {
            return Ok(vec![]);
        }
        let memories = self.personal_memory_context(now).await?;
        let mut jobs = Vec::with_capacity(eligible.len());
        for (session, turn_id, user_text, assistant_text, signal) in eligible {
            let digest = format!(
                "signal: {}\nuser evidence:\n{}\nassistant outcome:\n{}",
                learning_signal_name(signal),
                bounded_digest_text(&user_text),
                bounded_digest_text(&assistant_text),
            );
            let input = LearnerReviewInput {
                schema_version: KNOWLEDGE_VERSION,
                run_id: Uuid::nil(),
                person_id: self.person_id,
                session_id: session.id,
                session_revision: session.revision,
                turn_ids: vec![turn_id],
                outcome: LearningOutcome::Completed,
                digest,
                current_memories: memories.clone(),
                observed_at: now,
            };
            match self.enqueue_learner_review(input, now).await {
                Ok(job) => jobs.push(job),
                Err(AgentFailure::StaleContext | AgentFailure::Conflict) => {}
                Err(failure) => return Err(failure),
            }
        }
        Ok(jobs)
    }

    pub async fn claim_learner_review(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Option<LearnerReviewJob>, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let mut rows = transaction.query(
                "SELECT state, attempts, available_at, payload FROM learner_review_jobs WHERE person_id = ? AND ((state IN ('queued', 'deferred') AND available_at <= ?) OR (state = 'running' AND available_at <= ?)) ORDER BY available_at, id LIMIT 1",
                (
                    self.person_id.to_string(),
                    timestamp(now),
                    timestamp(now),
                ),
            ).await.map_err(storage)?;
            let Some(row) = rows.next().await.map_err(storage)? else {
                self.check_access()?;
                return Ok(None);
            };
            let stored_state = row.get::<String>(0).map_err(storage)?;
            let stored_attempts = row.get::<i64>(1).map_err(storage)?;
            let stored_available_at = row.get::<String>(2).map_err(storage)?;
            let mut job: LearnerReviewJob = decode(&row.get::<String>(3).map_err(storage)?)?;
            drop(rows);
            validate_learner_job(&job, self.person_id)?;
            if stored_state != learner_job_state(job.state)
                || stored_attempts != i64::from(job.attempts)
                || stored_available_at != timestamp(job.available_at)
            {
                return Err(AgentFailure::VaultUnavailable);
            }
            let previous_state = learner_job_state(job.state).to_owned();
            let previous_attempts = job.attempts;
            let previous_available_at = timestamp(job.available_at);
            let lifecycle = LearnerJobLifecycle {
                state: job.state,
                attempts: job.attempts,
                available_at: job.available_at,
                claimed_at: job.claimed_at,
                finished_at: job.finished_at,
                candidate_id: job.candidate_id,
                last_failure: job.last_failure,
            };
            let claim = claim_learner_job(&lifecycle, now)?;
            let claimed = match claim {
                LearnerJobClaim::Exhausted(exhausted) => {
                    job.state = exhausted.state;
                    job.finished_at = exhausted.finished_at;
                    job.last_failure = exhausted.last_failure;
                    let changed = transaction.execute(
                        "UPDATE learner_review_jobs SET state = ?, payload = ? WHERE id = ? AND state = ? AND attempts = ? AND available_at = ?",
                        (
                            learner_job_state(job.state),
                            payload(&job)?,
                            job.id.to_string(),
                            previous_state,
                            i64::from(previous_attempts),
                            previous_available_at,
                        ),
                    ).await.map_err(storage)?;
                    if changed != 1 {
                        return Err(AgentFailure::Conflict);
                    }
                    self.check_access()?;
                    return Ok(None);
                }
                LearnerJobClaim::Claimed(claimed) => claimed,
            };
            if let Err(failure) = validate_learner_source(self, &transaction, &job.input).await {
                let rejected = reject_learner_claim(&lifecycle, failure, now)?;
                job.state = rejected.state;
                job.attempts = rejected.attempts;
                job.available_at = rejected.available_at;
                job.claimed_at = rejected.claimed_at;
                job.finished_at = rejected.finished_at;
                job.candidate_id = rejected.candidate_id;
                job.last_failure = rejected.last_failure;
                let changed = transaction.execute(
                    "UPDATE learner_review_jobs SET state = 'failed', payload = ? WHERE id = ? AND state = ? AND attempts = ? AND available_at = ?",
                    (
                        payload(&job)?,
                        job.id.to_string(),
                        previous_state,
                        i64::from(previous_attempts),
                        previous_available_at,
                    ),
                ).await.map_err(storage)?;
                if changed != 1 {
                    return Err(AgentFailure::Conflict);
                }
                self.check_access()?;
                return Ok(None);
            }
            job.state = claimed.state;
            job.attempts = claimed.attempts;
            job.available_at = claimed.available_at;
            job.claimed_at = claimed.claimed_at;
            job.finished_at = claimed.finished_at;
            job.candidate_id = claimed.candidate_id;
            job.last_failure = claimed.last_failure;
            let changed = transaction.execute(
                "UPDATE learner_review_jobs SET state = 'running', attempts = ?, available_at = ?, payload = ? WHERE id = ? AND state = ? AND attempts = ? AND available_at = ?",
                (
                    i64::from(job.attempts),
                    timestamp(job.available_at),
                    payload(&job)?,
                    job.id.to_string(),
                    previous_state,
                    i64::from(previous_attempts),
                    previous_available_at,
                ),
            ).await.map_err(storage)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            self.check_access()?;
            Ok(Some(job))
        }.await;
        finish_transaction(transaction, result).await
    }

    pub async fn settle_learner_review(
        &self,
        job_id: Uuid,
        expected_attempt: u8,
        settlement: LearnerJobSettlement,
        settled_at: DateTime<Utc>,
    ) -> Result<LearnerReviewJob, AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let mut job = learner_job_by_id(&transaction, self.person_id, job_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if job.state != LearnerJobState::Running || job.attempts != expected_attempt {
                return Err(AgentFailure::Conflict);
            }
            if let LearnerJobSettlement::Completed { candidate_id: Some(candidate_id) } = settlement {
                let candidate = candidate_by_id(&transaction, self.person_id, candidate_id).await?;
                let expected_sources = job
                    .input
                    .turn_ids
                    .iter()
                    .map(|turn_id| LearningEvidenceRef {
                        session_id: job.input.session_id,
                        turn_id: *turn_id,
                    })
                    .collect::<Vec<_>>();
                if candidate.actor
                    != (KnowledgeActor::Learner {
                        run_id: job.input.run_id,
                    })
                    || candidate.source_refs != expected_sources
                {
                    return Err(AgentFailure::PolicyDenied);
                }
                if !evidence_is_independent(
                    &transaction,
                    self.person_id,
                    &candidate.source_refs,
                )
                .await?
                {
                    return Err(AgentFailure::PolicyDenied);
                }
            }
            let lifecycle = LearnerJobLifecycle {
                state: job.state,
                attempts: job.attempts,
                available_at: job.available_at,
                claimed_at: job.claimed_at,
                finished_at: job.finished_at,
                candidate_id: job.candidate_id,
                last_failure: job.last_failure,
            };
            let settled = settle_learner_job(&lifecycle, expected_attempt, settlement, settled_at)?;
            job.state = settled.state;
            job.attempts = settled.attempts;
            job.available_at = settled.available_at;
            job.claimed_at = settled.claimed_at;
            job.finished_at = settled.finished_at;
            job.candidate_id = settled.candidate_id;
            job.last_failure = settled.last_failure;
            let changed = transaction.execute(
                "UPDATE learner_review_jobs SET state = ?, available_at = ?, payload = ? WHERE id = ? AND state = 'running' AND attempts = ?",
                (
                    learner_job_state(job.state),
                    timestamp(job.available_at),
                    payload(&job)?,
                    job.id.to_string(),
                    i64::from(expected_attempt),
                ),
            ).await.map_err(storage)?;
            if changed != 1 {
                return Err(AgentFailure::Conflict);
            }
            self.check_access()?;
            Ok(job)
        }.await;
        finish_transaction(transaction, result).await
    }
}

impl<Keys: VaultKeyProvider> floe_knowledge::MemoryContextReader for EncryptedAgentVault<Keys> {
    async fn read_memory_context(
        &self,
        now: DateTime<Utc>,
    ) -> Result<Vec<ContextMemory>, AgentFailure> {
        self.personal_memory_context(now).await
    }
}

impl<Keys: VaultKeyProvider> floe_knowledge::MemoryCandidateSink for EncryptedAgentVault<Keys> {
    fn person_id(&self) -> floe_domain::PersonId {
        self.person_id
    }

    async fn stage_memory_candidate(
        &self,
        request: StageMemoryCandidate,
    ) -> Result<KnowledgeCandidate, AgentFailure> {
        EncryptedAgentVault::stage_memory_candidate(self, request).await
    }
}

fn explicit_learning_turn(
    session: &floe_agent::AgentSession,
) -> Option<(Uuid, String, String, LearningObservationKind)> {
    let (user_index, turn_id, user_text) =
        session
            .messages
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, message)| match message {
                floe_agent::AgentMessage::User { turn_id, text } => {
                    Some((index, *turn_id, text.trim()))
                }
                _ => None,
            })?;
    let signal = floe_agent::explicit_learning_signal(user_text)?;
    let assistant_text =
        session.messages[user_index + 1..]
            .iter()
            .find_map(|message| match message {
                floe_agent::AgentMessage::Assistant {
                    turn_id: assistant_turn,
                    text,
                } if *assistant_turn == turn_id => Some(text.trim()),
                _ => None,
            })?;
    if assistant_text.is_empty() {
        return None;
    }
    Some((
        turn_id,
        user_text.to_owned(),
        assistant_text.to_owned(),
        signal,
    ))
}

fn bounded_digest_text(value: &str) -> &str {
    if value.len() <= MAX_LEARNER_DIGEST_TEXT_BYTES {
        return value;
    }
    let mut end = MAX_LEARNER_DIGEST_TEXT_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn learning_signal_name(signal: LearningObservationKind) -> &'static str {
    match signal {
        LearningObservationKind::ExplicitRemember => "explicit_remember",
        LearningObservationKind::UserCorrection => "user_correction",
        LearningObservationKind::OutcomeConflict => "outcome_conflict",
        LearningObservationKind::ReusableProcedure => "reusable_procedure",
    }
}

async fn evidence_is_independent(
    transaction: &turso::transaction::Transaction<'_>,
    person_id: floe_domain::PersonId,
    evidence: &[LearningEvidenceRef],
) -> Result<bool, AgentFailure> {
    if evidence.is_empty() {
        return Ok(false);
    }
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
            )
            .await?,
            floe_domain::DependencyCoverage::Independent
        ) {
            return Ok(false);
        }
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
        person_id: floe_domain::PersonId,
        session_id: Uuid,
        turn_ids: &[Uuid],
    ) -> Result<LearningEvidenceSnapshot, AgentFailure> {
        if person_id != self.vault.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
        let session = self.vault.session_on(self.transaction, session_id).await?;
        let mut coverage = floe_domain::DependencyCoverage::Independent;
        for turn_id in turn_ids {
            let current = super::context_dependencies::read_context_dependency_coverage(
                self.transaction,
                person_id,
                session_id,
                *turn_id,
            )
            .await?;
            coverage = coverage
                .merge(&current)
                .map_err(|_| AgentFailure::VaultUnavailable)?;
        }
        let turn_ids = session
            .messages
            .iter()
            .filter(|message| !matches!(message, floe_agent::AgentMessage::Compaction { .. }))
            .map(floe_agent::AgentMessage::turn_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        self.vault.check_access()?;
        Ok(LearningEvidenceSnapshot {
            person_id: session.person_id,
            session_id: session.id,
            revision: session.revision,
            outcome: session.last_outcome.map(Into::into),
            personal: session.scope.is_none() && session.data_classes == [DataClass::Personal],
            active_turn: session.active_turn.is_some(),
            pending_output: session.pending_output.is_some(),
            turn_ids,
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
    admit_learning_evidence(
        &reader,
        input.person_id,
        input.session_id,
        input.session_revision,
        &input.turn_ids,
    )
    .await
    .map(|_| ())
    .map_err(|failure| match failure {
        AgentFailure::Conflict => AgentFailure::StaleContext,
        other => other,
    })
}

fn validate_learner_job(
    job: &LearnerReviewJob,
    person_id: floe_domain::PersonId,
) -> Result<(), AgentFailure> {
    if job.schema_version != KNOWLEDGE_VERSION
        || job.idempotency_key.is_empty()
        || job.input.run_id != job.id
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    validate_learner_job_lifecycle(&LearnerJobLifecycle {
        state: job.state,
        attempts: job.attempts,
        available_at: job.available_at,
        claimed_at: job.claimed_at,
        finished_at: job.finished_at,
        candidate_id: job.candidate_id,
        last_failure: job.last_failure,
    })?;
    validate_learner_input(&job.input, person_id).map_err(|_| AgentFailure::VaultUnavailable)
}

fn learner_job_state(state: LearnerJobState) -> &'static str {
    match state {
        LearnerJobState::Queued => "queued",
        LearnerJobState::Running => "running",
        LearnerJobState::Deferred => "deferred",
        LearnerJobState::Completed => "completed",
        LearnerJobState::Failed => "failed",
    }
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true)
}

async fn learner_job_by_key(
    transaction: &turso::transaction::Transaction<'_>,
    person_id: floe_domain::PersonId,
    idempotency_key: &str,
) -> Result<Option<LearnerReviewJob>, AgentFailure> {
    let mut rows = transaction
        .query(
            "SELECT payload FROM learner_review_jobs WHERE person_id = ? AND idempotency_key = ?",
            (person_id.to_string(), idempotency_key.to_owned()),
        )
        .await
        .map_err(storage)?;
    rows.next()
        .await
        .map_err(storage)?
        .map(|row| {
            let job: LearnerReviewJob = decode(&row.get::<String>(0).map_err(storage)?)?;
            validate_learner_job(&job, person_id)?;
            Ok(job)
        })
        .transpose()
}

async fn learner_job_by_id(
    transaction: &turso::transaction::Transaction<'_>,
    person_id: floe_domain::PersonId,
    job_id: Uuid,
) -> Result<Option<LearnerReviewJob>, AgentFailure> {
    let mut rows = transaction
        .query(
            "SELECT payload FROM learner_review_jobs WHERE person_id = ? AND id = ?",
            (person_id.to_string(), job_id.to_string()),
        )
        .await
        .map_err(storage)?;
    rows.next()
        .await
        .map_err(storage)?
        .map(|row| {
            let job: LearnerReviewJob = decode(&row.get::<String>(0).map_err(storage)?)?;
            validate_learner_job(&job, person_id)?;
            Ok(job)
        })
        .transpose()
}

async fn observation_by_hash(
    connection: &turso::transaction::Transaction<'_>,
    person_id: floe_domain::PersonId,
    content_hash: &str,
) -> Result<Option<LearningObservation>, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT payload FROM learning_observations WHERE person_id = ? AND content_hash = ?",
            (person_id.to_string(), content_hash.to_owned()),
        )
        .await
        .map_err(storage)?;
    rows.next()
        .await
        .map_err(storage)?
        .map(|row| {
            let observation: LearningObservation = decode(&row.get::<String>(0).map_err(storage)?)?;
            if observation.person_id != person_id || observation.content_hash != content_hash {
                return Err(AgentFailure::VaultUnavailable);
            }
            Ok(observation)
        })
        .transpose()
}

async fn candidate_by_key(
    connection: &turso::transaction::Transaction<'_>,
    person_id: floe_domain::PersonId,
    idempotency_key: &str,
) -> Result<Option<KnowledgeCandidate>, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT payload FROM knowledge_candidates WHERE person_id = ? AND idempotency_key = ?",
            (person_id.to_string(), idempotency_key.to_owned()),
        )
        .await
        .map_err(storage)?;
    rows.next()
        .await
        .map_err(storage)?
        .map(|row| {
            let candidate: KnowledgeCandidate = decode(&row.get::<String>(0).map_err(storage)?)?;
            validate_candidate_owner(&candidate, person_id)?;
            if candidate.idempotency_key != idempotency_key {
                return Err(AgentFailure::VaultUnavailable);
            }
            Ok(candidate)
        })
        .transpose()
}

async fn candidate_by_id(
    connection: &turso::transaction::Transaction<'_>,
    person_id: floe_domain::PersonId,
    candidate_id: Uuid,
) -> Result<KnowledgeCandidate, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT payload FROM knowledge_candidates WHERE id = ? AND person_id = ?",
            (candidate_id.to_string(), person_id.to_string()),
        )
        .await
        .map_err(storage)?;
    let row = rows
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::NotFound)?;
    let candidate: KnowledgeCandidate = decode(&row.get::<String>(0).map_err(storage)?)?;
    validate_candidate_owner(&candidate, person_id)?;
    if candidate.id != candidate_id {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(candidate)
}

async fn active_revision_on(
    connection: &turso::transaction::Transaction<'_>,
    person_id: floe_domain::PersonId,
    target_id: Uuid,
) -> Result<KnowledgeRevision, AgentFailure> {
    let mut rows = connection.query(
        "SELECT payload FROM knowledge_revisions WHERE target_id = ? AND person_id = ? AND state = 'active'",
        (target_id.to_string(), person_id.to_string()),
    ).await.map_err(storage)?;
    let row = rows
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::NotFound)?;
    let revision: KnowledgeRevision = decode(&row.get::<String>(0).map_err(storage)?)?;
    validate_revision(&revision, person_id)?;
    Ok(revision)
}

async fn revision_exists_on(
    connection: &turso::transaction::Transaction<'_>,
    target_id: Uuid,
) -> Result<bool, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT 1 FROM knowledge_revisions WHERE target_id = ? LIMIT 1",
            [target_id.to_string()],
        )
        .await
        .map_err(storage)?;
    Ok(rows.next().await.map_err(storage)?.is_some())
}

fn validate_candidate_owner(
    candidate: &KnowledgeCandidate,
    person_id: floe_domain::PersonId,
) -> Result<(), AgentFailure> {
    if candidate.schema_version != KNOWLEDGE_VERSION || candidate.person_id != person_id {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(())
}

fn validate_revision(
    revision: &KnowledgeRevision,
    person_id: floe_domain::PersonId,
) -> Result<(), AgentFailure> {
    if revision.schema_version != KNOWLEDGE_VERSION
        || revision.person_id != person_id
        || revision.kind != KnowledgeKind::Memory
        || revision.state != KnowledgeRevisionState::Active
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(())
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

fn hash(value: &impl Serialize) -> Result<String, AgentFailure> {
    let bytes = serde_json::to_vec(value).map_err(storage)?;
    let digest = Sha256::digest(bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
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

async fn finish_transaction<T>(
    transaction: turso::transaction::Transaction<'_>,
    result: Result<T, AgentFailure>,
) -> Result<T, AgentFailure> {
    match result {
        Ok(value) => {
            transaction.commit().await.map_err(storage)?;
            Ok(value)
        }
        Err(failure) => {
            let _ = transaction.rollback().await;
            Err(failure)
        }
    }
}
