use std::collections::HashSet;

use chrono::{DateTime, Utc};
use floe_agent::{
    AgentFailure, AgentOutcome, DataClass, EpistemicStatus, KNOWLEDGE_VERSION, KnowledgeActor,
    KnowledgeCandidate, KnowledgeCandidateState, KnowledgeDecision, KnowledgeDecisionKind,
    KnowledgeDecisionResult, KnowledgeKind, KnowledgeMutation, KnowledgeOperation,
    KnowledgePayload, KnowledgeRevision, KnowledgeRevisionState, LearningEvidenceRef,
    LearningObservation, PersonalMemoryKind, StageMemoryCandidate,
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
