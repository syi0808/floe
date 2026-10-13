//! Durable Knowledge discovery over completed Manager owner receipts.
//! This adapter stores reference-only scheduling state; Knowledge decides eligibility.
use super::{
    TransactionLearningEvidence, database_failure, finish_transaction, learner_job_by_id,
    learning_evidence_was_forgotten, learning_transcript_messages, storage, unavailable,
};
use crate::{EncryptedAgentVault, VaultKeyProvider};
use chrono::{DateTime, Utc};
use floe_access::DependencyCoverage;
use floe_agent_contract::{AgentFailure, ExecutionScope, OwnerActor, RunId};
use floe_knowledge::{
    EvidenceReader, LearningEvidenceRef, LearningEvidenceSnapshot, LearningOutcome,
    LearningSessionSnapshot,
};
use turso::transaction::TransactionBehavior;
use uuid::Uuid;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    /// Completed Manager source observations are committed with owner settlement.
    /// Only references and terminal proof are duplicated, never transcript text.
    pub(in crate::vault) async fn enqueue_learning_discovery_on(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        run: &floe_conversation::RunRecord,
    ) -> Result<(), AgentFailure> {
        run.validate(self.person_id)?;
        if run.state != floe_conversation::RunState::Completed {
            return Err(AgentFailure::Conflict);
        }
        let digest = crate::vault::conversations::terminal_receipt_on(transaction, run.run_id)
            .await?
            .ok_or(AgentFailure::StorageUnavailable)?;
        transaction.execute(
            "INSERT INTO learning_discovery_queue (sequence, run_id, person_id, session_id, source_revision, terminal_digest, state, available_at_ms) VALUES ((SELECT COALESCE(MAX(sequence), 0) + 1 FROM learning_discovery_queue), ?, ?, ?, ?, ?, 'pending', 0)",
            (run.run_id.as_uuid().to_string(), self.person_id.to_string(), run.session_id.to_string(),
                i64::try_from(run.session_revision).map_err(storage)?, digest),
        ).await.map_err(database_failure)?;
        Ok(())
    }

    pub(crate) async fn learner_discovery_sessions(
        &self,
        actor: &OwnerActor,
        limit: usize,
        now: DateTime<Utc>,
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
            .map_err(database_failure)?;
        let result = async {
            let mut rows = transaction.query(
                "SELECT run_id, session_id, source_revision, terminal_digest FROM learning_discovery_queue q WHERE person_id = ? AND state = 'pending' AND available_at_ms <= ? ORDER BY sequence LIMIT ?",
                (self.person_id.to_string(), now.timestamp_millis(), limit as i64),
            ).await.map_err(database_failure)?;
            let mut references = Vec::new();
            while let Some(row) = rows.next().await.map_err(database_failure)? {
                references.push((row.get::<String>(0).map_err(storage)?, row.get::<String>(1).map_err(storage)?,
                    row.get::<i64>(2).map_err(storage)?, row.get::<String>(3).map_err(storage)?));
            }
            drop(rows);
            let mut snapshots = Vec::new();
            for (run_id, session_id, source_revision, digest) in references {
                self.check_learning_scope(scope)?;
                let run_id = RunId::from_uuid(Uuid::parse_str(&run_id).map_err(unavailable)?)
                    .ok_or(AgentFailure::StorageUnavailable)?;
                let run = self.conversation_run_on(&transaction, run_id).await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                run.validate(self.person_id)?;
                if run.state != floe_conversation::RunState::Completed
                    || run.session_id.to_string() != session_id
                    || i64::try_from(run.session_revision).map_err(storage)? != source_revision
                    || crate::vault::conversations::terminal_receipt_on(&transaction, run_id).await?.as_deref() != Some(digest.as_str()) {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let history = self.read_manager_learning_turn_on(&transaction, &run).await
                    .map_err(crate::vault::conversation_core::owner_store_failure)?;
                let messages = learning_transcript_messages(self, &transaction, run.session_id, &history).await?;
                let mut turns = history.iter().map(|item| item.message.turn_id()).collect::<Vec<_>>();
                turns.dedup();
                let sources = turns.iter().map(|turn_id| LearningEvidenceRef { session_id: run.session_id, turn_id: *turn_id }).collect::<Vec<_>>();
                let discovery = floe_knowledge::LearningDiscoveryRef {
                    session_id: run.session_id, completed_run_id: run_id.as_uuid(), source_revision: run.session_revision,
                };
                if learning_evidence_was_forgotten(&transaction, self.person_id, &sources).await? {
                    snapshots.push(LearningSessionSnapshot { discovery, messages: Vec::new(),
                        evidence: LearningEvidenceSnapshot { person_id: self.person_id, session_id: run.session_id,
                            revision: run.session_revision, outcome: Some(LearningOutcome::Completed), personal: true,
                            active_turn: false, pending_output: false, turn_ids: turns,
                            coverage: DependencyCoverage::Unknown, purpose: floe_knowledge::EvidenceProjectionPurpose::Learning } });
                    continue;
                }
                let reader = TransactionLearningEvidence { vault: self, transaction: &transaction };
                let evidence = reader.read_learning_evidence(self.person_id, run.session_id, &turns).await?;
                snapshots.push(LearningSessionSnapshot { discovery, evidence, messages });
            }
            self.check_learning_scope(scope)?;
            Ok(snapshots)
        }.await;
        finish_transaction(self, transaction, result).await
    }

    pub(crate) async fn learner_settle_discovery(
        &self,
        actor: &OwnerActor,
        reference: floe_knowledge::LearningDiscoveryRef,
        disposition: floe_knowledge::LearningDiscoveryDisposition,
        now: DateTime<Utc>,
        scope: &ExecutionScope,
    ) -> Result<(), AgentFailure> {
        self.validate_learning_actor(actor)?;
        self.check_learning_scope(scope)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(database_failure)?;
        let result = async {
            let (run, state) = self.learning_discovery_run_on(&transaction, reference).await?;
            let (next, available) = match disposition {
                floe_knowledge::LearningDiscoveryDisposition::Deferred { until } => {
                    if state != "pending" { return Err(AgentFailure::Conflict); }
                    if until.timestamp_millis() < 0 || until > now + chrono::Duration::minutes(5) {
                        return Err(AgentFailure::InvalidInput);
                    }
                    ("pending", until.timestamp_millis())
                }
                floe_knowledge::LearningDiscoveryDisposition::Ignored => {
                    if state == "ignored" { return Ok(()); }
                    if state != "pending" { return Err(AgentFailure::Conflict); }
                    let history = self.read_manager_learning_turn_on(&transaction, &run).await
                        .map_err(crate::vault::conversation_core::owner_store_failure)?;
                    let sources = history.iter().map(|item| LearningEvidenceRef {
                        session_id: run.session_id, turn_id: item.message.turn_id(),
                    }).collect::<Vec<_>>();
                    if !learning_evidence_was_forgotten(&transaction, self.person_id, &sources).await? {
                        let messages = learning_transcript_messages(self, &transaction, run.session_id, &history).await?;
                        if !floe_knowledge::explicit_learning_evidence_turns(&messages)?.is_empty() {
                            return Err(AgentFailure::PolicyDenied);
                        }
                    }
                    ("ignored", 0)
                }
            };
            let changed = transaction.execute(
                "UPDATE learning_discovery_queue SET state = ?, available_at_ms = ? WHERE run_id = ? AND person_id = ? AND source_revision = ? AND state = 'pending'",
                (next, available, reference.completed_run_id.to_string(), self.person_id.to_string(), reference.source_revision as i64),
            ).await.map_err(database_failure)?;
            if changed != 1 { return Err(AgentFailure::Conflict); }
            self.check_learning_scope(scope)
        }.await;
        finish_transaction(self, transaction, result).await
    }

    pub(super) async fn learning_discovery_run_on(
        &self,
        transaction: &turso::transaction::Transaction<'_>,
        reference: floe_knowledge::LearningDiscoveryRef,
    ) -> Result<(floe_conversation::RunRecord, String), AgentFailure> {
        let run_id =
            RunId::from_uuid(reference.completed_run_id).ok_or(AgentFailure::InvalidInput)?;
        if reference.session_id.is_nil()
            || reference.source_revision == 0
            || reference.source_revision > i64::MAX as u64
        {
            return Err(AgentFailure::InvalidInput);
        }
        let run = self
            .conversation_run_on(transaction, run_id)
            .await?
            .ok_or(AgentFailure::NotFound)?;
        run.validate(self.person_id)?;
        if run.session_id != reference.session_id
            || run.session_revision != reference.source_revision
            || run.state != floe_conversation::RunState::Completed
        {
            return Err(AgentFailure::Conflict);
        }
        let mut rows = transaction.query(
            "SELECT session_id, source_revision, terminal_digest, state, job_id FROM learning_discovery_queue WHERE person_id = ? AND run_id = ?",
            (self.person_id.to_string(), reference.completed_run_id.to_string()),
        ).await.map_err(database_failure)?;
        let row = rows
            .next()
            .await
            .map_err(database_failure)?
            .ok_or(AgentFailure::NotFound)?;
        let state = row.get::<String>(3).map_err(storage)?;
        if row.get::<String>(0).map_err(storage)? != reference.session_id.to_string()
            || row.get::<i64>(1).map_err(storage)? != reference.source_revision as i64
            || !matches!(state.as_str(), "pending" | "ignored" | "enqueued")
            || crate::vault::conversations::terminal_receipt_on(transaction, run_id)
                .await?
                .as_deref()
                != Some(row.get::<String>(2).map_err(storage)?.as_str())
            || rows.next().await.map_err(database_failure)?.is_some()
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        let job_id = row.get::<Option<String>>(4).map_err(storage)?;
        drop(rows);
        match (state.as_str(), job_id) {
            ("enqueued", Some(job_id)) => {
                let id = Uuid::parse_str(&job_id).map_err(unavailable)?;
                let job = learner_job_by_id(transaction, self.person_id, id)
                    .await?
                    .ok_or(AgentFailure::StorageUnavailable)?;
                if job.input.session_id != reference.session_id
                    || job.input.session_revision != reference.source_revision
                    || job.input.turn_ids.last() != Some(&reference.completed_run_id)
                {
                    return Err(AgentFailure::StorageUnavailable);
                }
            }
            ("pending", None) => {}
            ("ignored", None) => {
                let history = self
                    .read_manager_learning_turn_on(transaction, &run)
                    .await
                    .map_err(crate::vault::conversation_core::owner_store_failure)?;
                let sources = history
                    .iter()
                    .map(|item| LearningEvidenceRef {
                        session_id: run.session_id,
                        turn_id: item.message.turn_id(),
                    })
                    .collect::<Vec<_>>();
                if !learning_evidence_was_forgotten(transaction, self.person_id, &sources).await? {
                    let messages =
                        learning_transcript_messages(self, transaction, run.session_id, &history)
                            .await?;
                    if !floe_knowledge::explicit_learning_evidence_turns(&messages)?.is_empty() {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                }
            }
            _ => return Err(AgentFailure::StorageUnavailable),
        }
        Ok((run, state))
    }

    pub(in crate::vault) async fn validate_learning_discovery_store(
        &self,
    ) -> Result<(), AgentFailure> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .await
            .map_err(database_failure)?;
        let result = async {
            let mut after = 0i64;
            loop {
                let mut rows = transaction.query(
                    "SELECT sequence, run_id, person_id, session_id, source_revision, available_at_ms FROM learning_discovery_queue WHERE sequence > ? ORDER BY sequence LIMIT 64",
                    [after],
                ).await.map_err(database_failure)?;
                let mut page = Vec::new();
                while let Some(row) = rows.next().await.map_err(database_failure)? {
                    let sequence = row.get::<i64>(0).map_err(storage)?;
                    let person = row.get::<String>(2).map_err(storage)?;
                    let revision = row.get::<i64>(4).map_err(storage)?;
                    if sequence <= after || person != self.person_id.to_string() || revision <= 0
                        || row.get::<i64>(5).map_err(storage)? < 0 { return Err(AgentFailure::StorageUnavailable); }
                    after = sequence;
                    page.push(floe_knowledge::LearningDiscoveryRef {
                        completed_run_id: Uuid::parse_str(&row.get::<String>(1).map_err(storage)?).map_err(unavailable)?,
                        session_id: Uuid::parse_str(&row.get::<String>(3).map_err(storage)?).map_err(unavailable)?,
                        source_revision: revision as u64,
                    });
                }
                drop(rows);
                let count = page.len();
                for reference in page { self.learning_discovery_run_on(&transaction, reference).await?; }
                if count < 64 { break; }
            }
            let mut missing = transaction.query(
                "SELECT 1 FROM agent_conversation_runs r LEFT JOIN learning_discovery_queue q ON q.run_id = r.run_id WHERE r.state = 'completed' AND q.run_id IS NULL LIMIT 1", (),
            ).await.map_err(database_failure)?;
            if missing.next().await.map_err(database_failure)?.is_some() { return Err(AgentFailure::StorageUnavailable); }
            self.check_access()
        }.await;
        finish_transaction(self, transaction, result).await
    }
}
