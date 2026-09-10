use std::collections::HashSet;

use chrono::{DateTime, Utc};
use floe_agent::{
    AgentFailure, AgentOutcome, ContextMemory, DataClass, EpistemicStatus, KNOWLEDGE_VERSION,
    KnowledgeActor, KnowledgeCandidate, KnowledgeCandidateState, KnowledgeDecision,
    KnowledgeDecisionKind, KnowledgeDecisionResult, KnowledgeKind, KnowledgeMutation,
    KnowledgeOperation, KnowledgePayload, KnowledgeRevision, KnowledgeRevisionState,
    LearnerJobSettlement, LearnerJobState, LearnerReviewInput, LearnerReviewJob,
    LearningEvidenceRef, LearningObservation, LearningObservationKind, MAX_CONTEXT_MEMORIES,
    MAX_CONTEXT_MEMORY_BYTES, PersonalMemoryKind, StageMemoryCandidate, retryable_learner_failure,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use turso::transaction::TransactionBehavior;
use uuid::Uuid;

use super::*;

const MAX_OBSERVATION_DIGEST_BYTES: usize = 4 * 1024;
const MAX_MEMORY_STATEMENT_BYTES: usize = 2 * 1024;
const MAX_EVIDENCE_REFS: usize = 32;
const MAX_VERSION_BYTES: usize = 128;
const LEARNER_JOB_LEASE_SECONDS: i64 = 30;
const MAX_LEARNER_JOB_ATTEMPTS: u8 = 3;
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
            let session = self.session_on(&transaction, request.session_id).await?;
            if session.revision != request.expected_session_revision {
                return Err(AgentFailure::Conflict);
            }
            if session.active_turn.is_some()
                || session.pending_output.is_some()
                || session.last_outcome != Some(AgentOutcome::Completed)
                || session.data_classes != [DataClass::Personal]
            {
                return Err(AgentFailure::PolicyDenied);
            }
            let message_turns: HashSet<_> = session
                .messages
                .iter()
                .map(floe_agent::AgentMessage::turn_id)
                .collect();
            if request
                .turn_ids
                .iter()
                .any(|turn_id| !message_turns.contains(turn_id))
            {
                return Err(AgentFailure::NotFound);
            }

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
                session.last_outcome,
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
                    outcome: AgentOutcome::Completed,
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

    pub async fn decide_knowledge_candidate(
        &self,
        candidate_id: Uuid,
        decision_kind: KnowledgeDecisionKind,
        actor: KnowledgeActor,
        decided_at: DateTime<Utc>,
    ) -> Result<KnowledgeDecisionResult, AgentFailure> {
        if actor != KnowledgeActor::User {
            return Err(AgentFailure::PolicyDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let mut candidate = candidate_by_id(&transaction, self.person_id, candidate_id).await?;
            if candidate.state != KnowledgeCandidateState::Pending {
                return Err(AgentFailure::Conflict);
            }
            let decision = KnowledgeDecision {
                schema_version: KNOWLEDGE_VERSION,
                id: Uuid::new_v4(),
                candidate_id,
                decision: decision_kind,
                actor: actor.clone(),
                decided_at,
            };
            let (revision, mutation) = match decision_kind {
                KnowledgeDecisionKind::Reject => {
                    candidate.state = KnowledgeCandidateState::Rejected;
                    (None, None)
                }
                KnowledgeDecisionKind::Approve => {
                    let target_id = candidate.target_id.unwrap_or_else(Uuid::new_v4);
                    let (from_revision, revision_number) = match candidate.operation {
                        KnowledgeOperation::Create => {
                            if candidate.target_id.is_some() || candidate.base_revision.is_some() {
                                return Err(AgentFailure::VaultUnavailable);
                            }
                            if revision_exists_on(&transaction, target_id).await? {
                                return Err(AgentFailure::Conflict);
                            }
                            (None, 1)
                        }
                        KnowledgeOperation::Revise => {
                            let current = active_revision_on(&transaction, self.person_id, target_id).await?;
                            if Some(current.revision) != candidate.base_revision
                                || candidate.before_hash.as_deref()
                                    != Some(hash(&current.payload)?.as_str())
                            {
                                return Err(AgentFailure::Conflict);
                            }
                            let mut superseded = current.clone();
                            superseded.state = KnowledgeRevisionState::Superseded;
                            let changed = transaction.execute(
                                "UPDATE knowledge_revisions SET state = 'superseded', payload = ? WHERE target_id = ? AND revision = ? AND state = 'active'",
                                (
                                    payload(&superseded)?,
                                    target_id.to_string(),
                                    integer(current.revision)?,
                                ),
                            ).await.map_err(storage)?;
                            if changed != 1 {
                                return Err(AgentFailure::Conflict);
                            }
                            (
                                Some(current.revision),
                                current.revision.checked_add(1).ok_or(AgentFailure::Conflict)?,
                            )
                        }
                        KnowledgeOperation::Retire => return Err(AgentFailure::InvalidInput),
                    };
                    let revision = KnowledgeRevision {
                        schema_version: KNOWLEDGE_VERSION,
                        target_id,
                        revision: revision_number,
                        person_id: self.person_id,
                        kind: candidate.kind,
                        payload: candidate.payload.clone(),
                        state: KnowledgeRevisionState::Active,
                        source_refs: candidate.source_refs.clone(),
                        created_by: actor.clone(),
                        created_at: decided_at,
                    };
                    transaction.execute(
                        "INSERT INTO knowledge_revisions (target_id, revision, person_id, kind, state, payload) VALUES (?, ?, ?, ?, 'active', ?)",
                        (
                            target_id.to_string(),
                            integer(revision_number)?,
                            self.person_id.to_string(),
                            kind_name(candidate.kind),
                            payload(&revision)?,
                        ),
                    ).await.map_err(storage)?;
                    let mutation = KnowledgeMutation {
                        schema_version: KNOWLEDGE_VERSION,
                        id: Uuid::new_v4(),
                        candidate_id,
                        target_id,
                        from_revision,
                        to_revision: revision_number,
                        actor: actor.clone(),
                        operation: candidate.operation,
                        before_hash: candidate.before_hash.clone(),
                        after_hash: candidate.after_hash.clone(),
                        rollback_revision: from_revision,
                        created_at: decided_at,
                    };
                    transaction.execute(
                        "INSERT INTO knowledge_mutations (id, candidate_id, target_id, created_at, payload) VALUES (?, ?, ?, ?, ?)",
                        (
                            mutation.id.to_string(),
                            candidate_id.to_string(),
                            target_id.to_string(),
                            decided_at.to_rfc3339(),
                            payload(&mutation)?,
                        ),
                    ).await.map_err(storage)?;
                    candidate.target_id = Some(target_id);
                    candidate.state = KnowledgeCandidateState::Approved;
                    (Some(revision), Some(mutation))
                }
            };
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
        for revision in self.active_personal_memories().await? {
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
            total_bytes = total_bytes
                .checked_add(value.statement.len())
                .ok_or(AgentFailure::BudgetExceeded)?;
            if context.len() >= MAX_CONTEXT_MEMORIES || total_bytes > MAX_CONTEXT_MEMORY_BYTES {
                return Err(AgentFailure::BudgetExceeded);
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
        Ok(context)
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
            validate_learner_source(&transaction, &input).await?;
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
                outcome: AgentOutcome::Completed,
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
            if job.attempts >= MAX_LEARNER_JOB_ATTEMPTS {
                job.state = LearnerJobState::Failed;
                job.finished_at = Some(now);
                job.last_failure = Some(AgentFailure::Stalled);
                transaction.execute(
                    "UPDATE learner_review_jobs SET state = 'failed', payload = ? WHERE id = ?",
                    (payload(&job)?, job.id.to_string()),
                ).await.map_err(storage)?;
                self.check_access()?;
                return Ok(None);
            }
            if let Err(failure) = validate_learner_source(&transaction, &job.input).await {
                if !matches!(
                    failure,
                    AgentFailure::StaleContext
                        | AgentFailure::NotFound
                        | AgentFailure::PolicyDenied
                ) {
                    return Err(failure);
                }
                job.state = LearnerJobState::Failed;
                job.finished_at = Some(now);
                job.last_failure = Some(failure);
                transaction.execute(
                    "UPDATE learner_review_jobs SET state = 'failed', payload = ? WHERE id = ?",
                    (payload(&job)?, job.id.to_string()),
                ).await.map_err(storage)?;
                self.check_access()?;
                return Ok(None);
            }
            job.state = LearnerJobState::Running;
            job.attempts = job.attempts.checked_add(1).ok_or(AgentFailure::VaultUnavailable)?;
            job.claimed_at = Some(now);
            job.available_at = now + chrono::Duration::seconds(LEARNER_JOB_LEASE_SECONDS);
            job.finished_at = None;
            job.last_failure = None;
            let changed = transaction.execute(
                "UPDATE learner_review_jobs SET state = 'running', attempts = ?, available_at = ?, payload = ? WHERE id = ?",
                (
                    i64::from(job.attempts),
                    timestamp(job.available_at),
                    payload(&job)?,
                    job.id.to_string(),
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
            match settlement {
                LearnerJobSettlement::Completed { candidate_id } => {
                    if let Some(candidate_id) = candidate_id {
                        let candidate =
                            candidate_by_id(&transaction, self.person_id, candidate_id).await?;
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
                    }
                    job.state = LearnerJobState::Completed;
                    job.finished_at = Some(settled_at);
                    job.candidate_id = candidate_id;
                    job.last_failure = None;
                }
                LearnerJobSettlement::Deferred {
                    available_at,
                    failure,
                } => {
                    if available_at <= settled_at
                        || !retryable_learner_failure(failure)
                    {
                        return Err(AgentFailure::InvalidInput);
                    }
                    if job.attempts >= MAX_LEARNER_JOB_ATTEMPTS {
                        job.state = LearnerJobState::Failed;
                        job.finished_at = Some(settled_at);
                        job.last_failure = Some(failure);
                    } else {
                        job.state = LearnerJobState::Deferred;
                        job.available_at = available_at;
                        job.claimed_at = None;
                        job.last_failure = Some(failure);
                    }
                }
                LearnerJobSettlement::Failed { failure } => {
                    job.state = LearnerJobState::Failed;
                    job.finished_at = Some(settled_at);
                    job.last_failure = Some(failure);
                }
            }
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

impl<Keys: VaultKeyProvider> floe_agent::MemoryCandidateSink for EncryptedAgentVault<Keys> {
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

fn validate_stage_request(request: &StageMemoryCandidate) -> Result<(), AgentFailure> {
    let digest = request.digest.trim();
    let statement = request.value.statement.trim();
    if digest.is_empty()
        || digest.len() > MAX_OBSERVATION_DIGEST_BYTES
        || statement.is_empty()
        || statement.len() > MAX_MEMORY_STATEMENT_BYTES
        || request.turn_ids.is_empty()
        || request.turn_ids.len() > MAX_EVIDENCE_REFS
        || request.turn_ids.iter().collect::<HashSet<_>>().len() != request.turn_ids.len()
        || !valid_version(&request.extractor_version)
        || !valid_version(&request.prompt_version)
        || request.value.confidence_millis > 1000
        || request
            .value
            .valid_until
            .zip(request.value.valid_from)
            .is_some_and(|(until, from)| until <= from)
    {
        return Err(AgentFailure::InvalidInput);
    }
    if matches!(
        request.actor,
        KnowledgeActor::Curator | KnowledgeActor::System
    ) {
        return Err(AgentFailure::PolicyDenied);
    }
    if matches!(request.value.kind, PersonalMemoryKind::Inference)
        != matches!(request.value.epistemic_status, EpistemicStatus::Inference)
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

fn validate_learner_input(
    input: &LearnerReviewInput,
    person_id: floe_domain::PersonId,
) -> Result<(), AgentFailure> {
    let unique_turns = input.turn_ids.iter().collect::<HashSet<_>>();
    let unique_memories = input
        .current_memories
        .iter()
        .map(|memory| memory.target_id)
        .collect::<HashSet<_>>();
    if input.schema_version != KNOWLEDGE_VERSION
        || input.person_id != person_id
        || input.outcome != AgentOutcome::Completed
        || input.session_revision == 0
        || input.turn_ids.is_empty()
        || input.turn_ids.len() > MAX_EVIDENCE_REFS
        || unique_turns.len() != input.turn_ids.len()
        || input.digest.trim().is_empty()
        || input.digest.len() > MAX_OBSERVATION_DIGEST_BYTES
        || input.current_memories.len() > MAX_CONTEXT_MEMORIES
        || unique_memories.len() != input.current_memories.len()
        || input.current_memories.iter().any(|memory| {
            memory.revision == 0
                || memory.statement.trim().is_empty()
                || memory.confidence_millis > 1000
                || memory.source_refs.is_empty()
        })
        || serde_json::to_vec(input)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > 16 * 1024
    {
        return Err(AgentFailure::InvalidInput);
    }
    Ok(())
}

async fn validate_learner_source(
    transaction: &turso::transaction::Transaction<'_>,
    input: &LearnerReviewInput,
) -> Result<(), AgentFailure> {
    let mut rows = transaction
        .query(
            "SELECT revision, payload FROM agent_sessions WHERE id = ?",
            [input.session_id.to_string()],
        )
        .await
        .map_err(storage)?;
    let row = rows
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::NotFound)?;
    let expected_revision =
        i64::try_from(input.session_revision).map_err(|_| AgentFailure::InvalidInput)?;
    if row.get::<i64>(0).map_err(storage)? != expected_revision {
        return Err(AgentFailure::StaleContext);
    }
    let session: floe_agent::AgentSession = decode(&row.get::<String>(1).map_err(storage)?)?;
    if session.id != input.session_id
        || session.person_id != input.person_id
        || session.revision != input.session_revision
        || session.scope.is_some()
        || session.data_classes != [DataClass::Personal]
        || session.active_turn.is_some()
        || session.pending_output.is_some()
        || session.last_outcome != Some(input.outcome)
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let evidence = session
        .messages
        .iter()
        .map(floe_agent::AgentMessage::turn_id)
        .collect::<HashSet<_>>();
    if input
        .turn_ids
        .iter()
        .any(|turn_id| !evidence.contains(turn_id))
    {
        return Err(AgentFailure::NotFound);
    }
    Ok(())
}

fn validate_learner_job(
    job: &LearnerReviewJob,
    person_id: floe_domain::PersonId,
) -> Result<(), AgentFailure> {
    let valid_lifecycle = match job.state {
        LearnerJobState::Queued => {
            job.attempts == 0
                && job.claimed_at.is_none()
                && job.finished_at.is_none()
                && job.candidate_id.is_none()
                && job.last_failure.is_none()
        }
        LearnerJobState::Running => {
            (1..=MAX_LEARNER_JOB_ATTEMPTS).contains(&job.attempts)
                && job.claimed_at.is_some()
                && job.finished_at.is_none()
                && job.candidate_id.is_none()
                && job.last_failure.is_none()
        }
        LearnerJobState::Deferred => {
            (1..MAX_LEARNER_JOB_ATTEMPTS).contains(&job.attempts)
                && job.claimed_at.is_none()
                && job.finished_at.is_none()
                && job.candidate_id.is_none()
                && job.last_failure.is_some_and(retryable_learner_failure)
        }
        LearnerJobState::Completed => {
            job.attempts > 0 && job.finished_at.is_some() && job.last_failure.is_none()
        }
        LearnerJobState::Failed => {
            job.attempts <= MAX_LEARNER_JOB_ATTEMPTS
                && job.finished_at.is_some()
                && job.candidate_id.is_none()
                && job.last_failure.is_some()
        }
    };
    if job.schema_version != KNOWLEDGE_VERSION
        || job.idempotency_key.is_empty()
        || job.attempts > MAX_LEARNER_JOB_ATTEMPTS
        || job.input.run_id != job.id
        || !valid_lifecycle
    {
        return Err(AgentFailure::VaultUnavailable);
    }
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

fn valid_version(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_VERSION_BYTES
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
