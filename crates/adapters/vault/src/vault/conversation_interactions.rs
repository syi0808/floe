//! Durable Conversation interaction storage beside Conversation state.
//!
//! Interactions, decisions and publication linkage live in the same encrypted
//! Vault as Sessions and Runs, but no interaction row grants authority: it
//! records the admitted origin, the owner-produced requirement, the immutable
//! reviewed target and the decision intent that a later owner operation acts
//! on. Publication identity is deterministic (origin plus canonical digests),
//! so crash replay settles the same row instead of duplicating it. The
//! primary key on that deterministic id is the origin+digest uniqueness
//! constraint: every write revalidates that the id commits to the stored
//! origin and digests, so two distinct origins (for example two identical
//! tool calls) publish two rows while the same publication replays one.
//! Decision command identity is a second primary key with the same
//! replay-or-conflict contract.

use floe_agent_contract::{AgentFailure, JournalEvent};
use floe_conversation::{
    ConversationInteraction, DecisionAdmission, ExpireInteraction, ExpireOutcome,
    InteractionDecision, InteractionOrigin, InteractionResolutionCommit,
    InteractionResolutionReceipt, InteractionState, MAX_ACTIVE_INTERACTIONS_PER_RUN,
    MAX_STORED_INTERACTIONS_PER_RUN, ReviewAuditRecord, RunRecord, RunState,
    SupersedeInteraction, next_state_after_decision, state_after_resolution,
};
use floe_kernel::{PersonId, RunId};
use turso::transaction::{Transaction, TransactionBehavior};
use uuid::Uuid;

use super::*;

const SCHEMA_VERSION: i64 = 1;
const MAX_INTERACTION_PAYLOAD_BYTES: usize = 32 * 1024;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn conversation_interaction(
        &self,
        interaction_id: Uuid,
    ) -> Result<Option<ConversationInteraction>, AgentFailure> {
        if interaction_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        let record = read_interaction(&self.connection()?, self.person_id, interaction_id).await?;
        self.check_access()?;
        Ok(record)
    }

    pub async fn run_conversation_interactions(
        &self,
        origin_run_id: RunId,
    ) -> Result<Vec<ConversationInteraction>, AgentFailure> {
        if !origin_run_id.is_valid() {
            return Err(AgentFailure::InvalidInput);
        }
        let records = read_group_on(&self.connection()?, self.person_id, origin_run_id).await?;
        self.check_access()?;
        Ok(records)
    }

    pub async fn record_conversation_interaction_decision(
        &self,
        decision: InteractionDecision,
    ) -> Result<DecisionAdmission, AgentFailure> {
        decision
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if decision.principal != self.person_id.to_string() {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            if let Some(recorded) = read_decision(&transaction, decision.command_id).await? {
                if !decision.matches_recorded(&recorded) {
                    return Err(AgentFailure::Conflict);
                }
                let current =
                    read_interaction(&transaction, self.person_id, decision.interaction_id)
                        .await?
                        .ok_or(AgentFailure::VaultUnavailable)?;
                return Ok(DecisionAdmission::Rejoined(current));
            }
            if super::conversations::command_identity_used(&transaction, decision.command_id)
                .await?
            {
                return Err(AgentFailure::Conflict);
            }
            let current = read_interaction(&transaction, self.person_id, decision.interaction_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if current.revision != decision.interaction_revision
                || current.target_digest != decision.target_digest
                || decision.decided_at_unix_ms < current.created_at_unix_ms
                || decision.decided_at_unix_ms >= current.expires_at_unix_ms
            {
                return Err(AgentFailure::Conflict);
            }
            let next = next_state_after_decision(&current.state, &decision)?;
            let mut updated = current;
            updated.state = next;
            updated.revision = updated
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            updated
                .validate()
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            insert_decision(&transaction, &decision).await?;
            update_interaction(&transaction, &updated, updated.revision - 1).await?;
            let run = self
                .conversation_run_on(&transaction, updated.origin_run_id)
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            self.enqueue_resume_on(&transaction, &run).await?;
            self.check_access()?;
            Ok(DecisionAdmission::Applied(updated))
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn resolve_conversation_interaction_and_request_resume(
        &self,
        commit: InteractionResolutionCommit,
    ) -> Result<InteractionResolutionReceipt, AgentFailure> {
        let resolution = &commit.resolution;
        resolution.validate()?;
        commit.owner_receipt.validate()?;
        if resolution.person_id != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            let current = read_interaction(&transaction, self.person_id, resolution.interaction_id)
                .await?.ok_or(AgentFailure::NotFound)?;
            match &resolution.cause {
                floe_conversation::InteractionResolutionCause::Decision { command_id } => {
                    let recorded = read_decision(&transaction, *command_id).await?.ok_or(AgentFailure::Conflict)?;
                    if recorded.interaction_id != resolution.interaction_id
                        || recorded.target_digest != resolution.target_digest
                        || recorded.principal != self.person_id.to_string()
                        || resolution.resolved_at_unix_ms < recorded.decided_at_unix_ms
                        || !matches!(commit.owner_receipt, floe_conversation::OwnerResolutionReceipt::SourceProcessing { .. }) {
                        return Err(AgentFailure::Conflict);
                    }
                }
                floe_conversation::InteractionResolutionCause::Refresh { command_id } => {
                    if !matches!(commit.owner_receipt, floe_conversation::OwnerResolutionReceipt::ExpertBinding { .. }) {
                        return Err(AgentFailure::Conflict);
                    }
                    let mut rows = transaction.query("SELECT person_id, session_id, interaction_id, expected_revision FROM agent_conversation_interaction_refreshes WHERE command_id = ?", [command_id.to_string()]).await.map_err(storage)?;
                    let row = rows.next().await.map_err(storage)?.ok_or(AgentFailure::Conflict)?;
                    if row.get::<String>(0).map_err(storage)? != self.person_id.to_string()
                        || row.get::<String>(1).map_err(storage)? != current.session_id.to_string()
                        || row.get::<String>(2).map_err(storage)? != current.id.to_string()
                        || row.get::<i64>(3).map_err(storage)? != integer(resolution.expected_revision)?
                        || rows.next().await.map_err(storage)?.is_some() { return Err(AgentFailure::Conflict); }
                }
            }
            if current.target_digest != resolution.target_digest {
                return Err(AgentFailure::Conflict);
            }
            let run = self
                .conversation_run_on(&transaction, current.origin_run_id)
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            self.validate_owner_resolution_on(&transaction, &run, &current, &commit)
                .await?;
            if let InteractionState::Resolved { receipt } = &current.state {
                if current.revision
                    == resolution
                        .expected_revision
                        .checked_add(1)
                        .ok_or(AgentFailure::Conflict)?
                    && receipt.cause == resolution.cause
                    && receipt.owner_command_id == resolution.owner_command_id
                    && receipt.owner_operation_id == resolution.owner_operation_id
                    && receipt.owner_receipt == commit.owner_receipt
                    && receipt.resolved_at_unix_ms == resolution.resolved_at_unix_ms
                {
                    self.enqueue_resume_on(&transaction, &run).await?;
                    return Ok(receipt.clone());
                }
                return Err(AgentFailure::Conflict);
            }
            if current.revision != resolution.expected_revision {
                return Err(AgentFailure::Conflict);
            }
            let next = state_after_resolution(&current.state, resolution, &commit.owner_receipt)?;
            let InteractionState::Resolved { receipt } = &next else {
                return Err(AgentFailure::StorageUnavailable);
            };
            let receipt = receipt.clone();
            let mut updated = current;
            updated.state = next;
            updated.revision = updated
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            updated.validate()?;
            update_interaction(&transaction, &updated, resolution.expected_revision).await?;
            self.enqueue_resume_on(&transaction, &run).await?;
            self.check_access()?;
            Ok(receipt)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn supersede_conversation_interaction(
        &self,
        supersede: SupersedeInteraction,
    ) -> Result<ConversationInteraction, AgentFailure> {
        supersede
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if supersede.person_id != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            let current = read_interaction(&transaction, self.person_id, supersede.interaction_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if current.revision != supersede.expected_revision || current.state.is_terminal() {
                return Err(AgentFailure::Conflict);
            }
            let mut updated = current;
            updated.state = InteractionState::Superseded {
                superseded_by: supersede.superseded_by,
            };
            updated.revision = updated
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            updated
                .validate()
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            update_interaction(&transaction, &updated, updated.revision - 1).await?;
            let run = self
                .conversation_run_on(&transaction, updated.origin_run_id)
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            self.enqueue_resume_on(&transaction, &run).await?;
            self.check_access()?;
            Ok(updated)
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub async fn expire_conversation_interaction(
        &self,
        expire: ExpireInteraction,
    ) -> Result<ExpireOutcome, AgentFailure> {
        expire.validate().map_err(|_| AgentFailure::InvalidInput)?;
        if expire.person_id != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            let current = read_interaction(&transaction, self.person_id, expire.interaction_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            if current.state.is_terminal() {
                return Ok(ExpireOutcome::AlreadyTerminal(current));
            }
            if expire.now_unix_ms < current.expires_at_unix_ms {
                return Ok(ExpireOutcome::NotExpired(current));
            }
            let mut updated = current;
            updated.state = InteractionState::Expired;
            updated.revision = updated
                .revision
                .checked_add(1)
                .ok_or(AgentFailure::Conflict)?;
            updated
                .validate()
                .map_err(|_| AgentFailure::VaultUnavailable)?;
            update_interaction(&transaction, &updated, updated.revision - 1).await?;
            let run = self
                .conversation_run_on(&transaction, updated.origin_run_id)
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            self.enqueue_resume_on(&transaction, &run).await?;
            self.check_access()?;
            Ok(ExpireOutcome::Expired(updated))
        }
        .await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }

    pub(super) async fn check_interaction_origin_on(
        &self, transaction: &Transaction<'_>, record: &ConversationInteraction,
    ) -> Result<(), AgentFailure> {
        record.validate()?;
        let run = self.conversation_run_on(transaction, record.origin_run_id).await?.ok_or(AgentFailure::NotFound)?;
        if run.person_id != self.person_id || run.session_id != record.session_id || run.state != RunState::Working
            || record.revision != 1 || !matches!(record.state, InteractionState::Pending) {
            return Err(AgentFailure::Conflict);
        }
        let journal = self.conversation_journal_on(transaction, &run).await?;
        match &record.origin {
            InteractionOrigin::Projection { run_id, .. } if *run_id == run.run_id => {},
            InteractionOrigin::Task { execution, capability_call_id } => {
                let actual = self.read_execution_receipt_on(transaction, execution).await?;
                let task = self.task_on(transaction, execution.execution.task_id).await?.ok_or(AgentFailure::Conflict)?;
                self.validate_conversation_task_lineage_on(transaction, &run, &task).await?;
                if task.device_id != run.device_id || actual.snapshot.principal != self.person_id.to_string()
                    || !journal.iter().any(|entry| matches!(&entry.event, JournalEvent::DelegationIntent { request }
                        if request.task_id == execution.execution.task_id && request.parent_run_id == Some(run.run_id.as_uuid())
                        && request.principal == self.person_id.to_string() && request.execution_context.session_id == run.session_id
                        && request.execution_context.device_id == run.device_id))
                    || !journal.iter().any(|entry| matches!(&entry.event, JournalEvent::DelegationResult { receipt }
                        if matches!(&receipt.execution, floe_agent_contract::TaskExecutionEvidence::Admitted(stored) if stored == &actual))) {
                    return Err(AgentFailure::Conflict);
                }
                if let Some(call_id) = capability_call_id {
                    if !matches!(&actual.snapshot.blockage, Some(floe_agent_contract::TaskBlockage::SourceRead { tool_call_id, .. }) if tool_call_id == call_id) {
                        return Err(AgentFailure::Conflict);
                    }
                }
                if matches!(&record.audit.evidence, floe_conversation::BlockedReviewEvidence::ExpertBinding { .. })
                    && record.requirement.consumer != actual.snapshot.agent_id { return Err(AgentFailure::Conflict); }
            }
            _ => return Err(AgentFailure::Conflict),
        }
        self.store_review_audit_on(transaction, &run, &record.audit, false).await
    }

    pub(super) async fn validate_blocked_task_audits_on(
        &self, transaction: &Transaction<'_>, commit: &floe_conversation::BlockedRunCommit,
    ) -> Result<(), AgentFailure> {
        use floe_conversation::BlockedReviewEvidence as E;
        let mut groups: std::collections::HashMap<floe_agent_contract::TaskExecutionKey,
            (floe_agent_contract::TaskExecutionReceiptRef, Vec<&E>)> = std::collections::HashMap::new();
        for publication in &commit.publications {
            let execution = match &publication.record.evidence {
                E::ModelProjection { .. } => continue,
                E::TaskModelProjection { execution, .. } | E::SourceRead { execution, .. }
                | E::ExpertBinding { execution, .. } | E::Navigation { execution, .. } => execution,
            };
            let group = groups.entry(execution.execution).or_insert_with(|| (execution.clone(), Vec::new()));
            if group.0 != *execution { return Err(AgentFailure::Conflict); }
            group.1.push(&publication.record.evidence);
        }
        for (_, (reference, audits)) in groups {
            let actual = self.read_execution_receipt_on(transaction, &reference).await?;
            match actual.snapshot.blockage.as_ref().ok_or(AgentFailure::Conflict)? {
                floe_agent_contract::TaskBlockage::ModelProjection { plan, review } => {
                    if audits.len() != 1 || !matches!(audits[0], E::TaskModelProjection { plan: p, review: r, .. } if p == plan && r == review) {
                        return Err(AgentFailure::Conflict);
                    }
                }
                floe_agent_contract::TaskBlockage::SourceRead { tool_call_id, blockers } => {
                    let mut covered = std::collections::BTreeSet::new();
                    for audit in audits {
                        let requirements = match audit {
                            E::SourceRead { tool_call_id: call, blockers, .. } if call == tool_call_id => blockers.blockers().to_vec(),
                            E::Navigation { requirement, .. } => vec![requirement.clone()],
                            _ => return Err(AgentFailure::Conflict),
                        };
                        for requirement in requirements {
                            if !covered.insert(serde_json::to_vec(&requirement).map_err(storage)?) { return Err(AgentFailure::Conflict); }
                        }
                    }
                    let expected = blockers.blockers().iter().map(serde_json::to_vec).collect::<Result<std::collections::BTreeSet<_>, _>>().map_err(storage)?;
                    if covered != expected { return Err(AgentFailure::Conflict); }
                }
                floe_agent_contract::TaskBlockage::Binding { requirement_keys } => {
                    let mut covered = std::collections::BTreeSet::new();
                    for audit in audits {
                        let E::ExpertBinding { requirement_key, .. } = audit else { return Err(AgentFailure::Conflict); };
                        if !covered.insert(requirement_key.clone()) { return Err(AgentFailure::Conflict); }
                    }
                    if covered != requirement_keys.iter().cloned().collect() { return Err(AgentFailure::Conflict); }
                }
            }
        }
        Ok(())
    }

    pub(super) async fn store_review_audit_on(
        &self, transaction: &Transaction<'_>, run: &RunRecord,
        audit: &ReviewAuditRecord, existing_only: bool,
    ) -> Result<(), AgentFailure> {
        use floe_conversation::BlockedReviewEvidence as E;
        audit.validate()?;
        if audit.run_id != run.run_id || audit.person_id != run.person_id || audit.device_id != run.device_id
            || audit.session_id != run.session_id || audit.executor_generation != run.executor_generation {
            return Err(AgentFailure::Conflict);
        }
        let id = audit.operation_id.to_string();
        let mut rows = transaction.query("SELECT run_id,person_id,payload FROM agent_conversation_review_audits WHERE operation_id=? AND run_id=?", (id.clone(), run.run_id.as_uuid().to_string())).await.map_err(storage)?;
        if let Some(row) = rows.next().await.map_err(storage)? {
            let payload = row.get::<String>(2).map_err(storage)?;
            if payload.len() > 128 * 1024 { return Err(AgentFailure::StorageUnavailable); }
            let stored: ReviewAuditRecord = serde_json::from_str(&payload).map_err(unavailable)?;
            stored.validate()?;
            if stored != *audit || row.get::<String>(0).map_err(storage)? != run.run_id.as_uuid().to_string()
                || row.get::<String>(1).map_err(storage)? != run.person_id.to_string() || rows.next().await.map_err(storage)?.is_some() {
                return Err(AgentFailure::Conflict);
            }
            return Ok(());
        }
        drop(rows);
        if existing_only { return Err(AgentFailure::StorageUnavailable); }
        let source = match &audit.evidence {
            E::ModelProjection { review, access_reviews, .. } | E::TaskModelProjection { review, access_reviews, .. } => Some((&review.blockers, access_reviews, review.target_digest)),
            E::SourceRead { execution, tool_call_id, blockers, access_reviews } => {
                use sha2::Digest;
                let origin = InteractionOrigin::Task { execution: execution.clone(), capability_call_id: Some(*tool_call_id) };
                let digest: [u8;32] = sha2::Sha256::digest(serde_json::to_vec(&(run.run_id, &origin, blockers)).map_err(storage)?).into();
                Some((blockers, access_reviews, digest))
            }
            E::ExpertBinding { execution, requirement_key, review } => {
                let descriptor = self.read_binding_review_on(transaction, &run.device_id, review).await?;
                if descriptor.identity.person_id != run.person_id || descriptor.identity.device_id != run.device_id
                    || descriptor.identity.task_origin.as_ref() != Some(execution) || &descriptor.identity.requirement_key != requirement_key {
                    return Err(AgentFailure::Conflict);
                }
                None
            }
            E::Navigation { .. } => None,
        };
        if let Some((blockers, links, target_digest)) = source {
            for link in links {
                let connection_id = link.requirement.connection_id().ok_or(AgentFailure::Conflict)?.clone();
                let requirements = blockers.blockers().iter().filter(|b| b.connection_id() == Some(&connection_id)).cloned().collect::<Vec<_>>();
                let origin = floe_access::ProjectionReviewOrigin::for_requirements(run.run_id, audit.operation_id, target_digest, connection_id, &requirements)?;
                let review = access_review_on(transaction, self.person_id, &link.reference).await?;
                origin.validate_review_binding(run.person_id, &run.device_id, &review)?;
            }
        }
        let payload = serde_json::to_string(audit).map_err(storage)?;
        if payload.len() > 128 * 1024 { return Err(AgentFailure::BudgetExceeded); }
        transaction.execute("INSERT INTO agent_conversation_review_audits(operation_id,run_id,person_id,payload) VALUES(?,?,?,?)",
            (id,run.run_id.as_uuid().to_string(),self.person_id.to_string(),payload)).await.map_err(storage)?;
        Ok(())
    }

    async fn validate_owner_resolution_on(
        &self,
        transaction: &Transaction<'_>,
        run: &RunRecord,
        interaction: &ConversationInteraction,
        commit: &InteractionResolutionCommit,
    ) -> Result<(), AgentFailure> {
        use floe_conversation::{OwnerResolutionReceipt, ReviewedTarget};
        if commit.owner_receipt.command_id() != commit.resolution.owner_command_id
            || commit.owner_receipt.operation_id() != commit.resolution.owner_operation_id
        {
            return Err(AgentFailure::Conflict);
        }
        match (&interaction.target, &commit.owner_receipt) {
            (
                ReviewedTarget::SourceReview(reference),
                OwnerResolutionReceipt::SourceProcessing { receipt },
            ) => {
                receipt.validate()?;
                let review = access_review_on(transaction, self.person_id, reference).await?;
                if review.device_id != run.device_id
                    || receipt.reservation.source != review.source
                    || !matches!(&receipt.kind, floe_access::GrantCommitKind::Reviewed { review } if review == reference)
                {
                    return Err(AgentFailure::Conflict);
                }
                let mut rows = transaction.query("SELECT payload FROM access_grant_operations WHERE operation_id = ? AND person_id = ?",
                    (receipt.reservation.operation_id.to_string(), self.person_id.to_string())).await.map_err(storage)?;
                let row = rows
                    .next()
                    .await
                    .map_err(storage)?
                    .ok_or(AgentFailure::Conflict)?;
                let payload = row.get::<String>(0).map_err(storage)?;
                if payload.len() > 256 * 1024 {
                    return Err(AgentFailure::StorageUnavailable);
                }
                let stored: floe_access::GrantOperationReceipt =
                    serde_json::from_str(&payload).map_err(unavailable)?;
                stored.validate()?;
                if stored != floe_access::GrantOperationReceipt::Committed(receipt.clone())
                    || rows.next().await.map_err(storage)?.is_some()
                {
                    return Err(AgentFailure::Conflict);
                }
                Ok(())
            }
            (ReviewedTarget::ExpertBinding(reference), OwnerResolutionReceipt::ExpertBinding { receipt }) => {
                let descriptor = self.read_binding_review_on(transaction, &run.device_id, reference).await?;
                let stored = self.read_binding_review_receipt_on(transaction, &run.device_id, reference).await?
                    .ok_or(AgentFailure::Conflict)?;
                let actual = floe_experts::project_binding_mutation_receipt(&stored, &descriptor)?;
                if &actual != receipt || receipt.review_ref != *reference
                    || commit.resolution.resolved_at_unix_ms < receipt.committed_at_unix_ms {
                    return Err(AgentFailure::Conflict);
                }
                Ok(())
            }
            _ => Err(AgentFailure::Conflict),
        }
    }
}

async fn table_exists(connection: &turso::Connection, table: &str) -> Result<bool, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?",
            [table],
        )
        .await
        .map_err(storage)?;
    Ok(rows.next().await.map_err(storage)?.is_some())
}

pub(super) async fn initialize(transaction: &Transaction<'_>) -> Result<(), AgentFailure> {
    let mut tables = transaction
        .query(
            "SELECT name FROM sqlite_schema WHERE type = 'table' AND name IN ('agent_conversation_interaction_schema', 'agent_conversation_interactions', 'agent_conversation_interaction_decisions', 'agent_conversation_interaction_refreshes')",
            (),
        )
        .await
        .map_err(storage)?;
    let mut found = Vec::new();
    while let Some(row) = tables.next().await.map_err(storage)? {
        found.push(row.get::<String>(0).map_err(storage)?);
    }
    found.sort();
    if found.is_empty() {
        transaction
            .execute(
                "CREATE TABLE agent_conversation_interaction_schema (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL CHECK (version = 1))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE agent_conversation_interactions (interaction_id TEXT PRIMARY KEY, session_id TEXT NOT NULL, person_id TEXT NOT NULL, origin_run_id TEXT NOT NULL, requirement_digest TEXT NOT NULL, target_digest TEXT NOT NULL, state TEXT NOT NULL CHECK (state IN ('pending', 'resolving', 'resolved', 'denied', 'cancelled', 'superseded', 'expired')), revision INTEGER NOT NULL CHECK (revision > 0), created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL, payload TEXT NOT NULL CHECK (length(CAST(payload AS BLOB)) <= 32768))",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "CREATE TABLE agent_conversation_interaction_decisions (command_id TEXT PRIMARY KEY, interaction_id TEXT NOT NULL REFERENCES agent_conversation_interactions(interaction_id), kind TEXT NOT NULL CHECK (kind IN ('approve', 'deny', 'dismiss')), target_digest TEXT NOT NULL, principal TEXT NOT NULL, interaction_revision INTEGER NOT NULL CHECK (interaction_revision > 0), decided_at INTEGER NOT NULL)",
                (),
            )
            .await
            .map_err(storage)?;
        transaction.execute("CREATE TABLE agent_conversation_interaction_refreshes (command_id TEXT PRIMARY KEY, person_id TEXT NOT NULL, session_id TEXT NOT NULL, interaction_id TEXT NOT NULL REFERENCES agent_conversation_interactions(interaction_id), expected_revision INTEGER NOT NULL CHECK(expected_revision > 0))", ()).await.map_err(storage)?;
        transaction
            .execute(
                "CREATE INDEX agent_conversation_interactions_run ON agent_conversation_interactions (origin_run_id, state, interaction_id)",
                (),
            )
            .await
            .map_err(storage)?;
        transaction
            .execute(
                "INSERT INTO agent_conversation_interaction_schema (id, version) VALUES (1, 1)",
                (),
            )
            .await
            .map_err(storage)?;
        return Ok(());
    }
    if found
        != [
            "agent_conversation_interaction_decisions".to_owned(),
            "agent_conversation_interaction_refreshes".to_owned(),
            "agent_conversation_interaction_schema".to_owned(),
            "agent_conversation_interactions".to_owned(),
        ]
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut marker = transaction
        .query(
            "SELECT id, version FROM agent_conversation_interaction_schema",
            (),
        )
        .await
        .map_err(storage)?;
    let row = marker
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::VaultUnavailable)?;
    if row.get::<i64>(0).map_err(storage)? != 1
        || row.get::<i64>(1).map_err(storage)? != SCHEMA_VERSION
        || marker.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    transaction
        .query(
            "SELECT interaction_id, session_id, person_id, origin_run_id, requirement_digest, target_digest, state, revision, created_at, expires_at, payload FROM agent_conversation_interactions LIMIT 0",
            (),
        )
        .await
        .map_err(storage)?;
    transaction
        .query(
            "SELECT command_id, interaction_id, kind, target_digest, principal, interaction_revision, decided_at FROM agent_conversation_interaction_decisions LIMIT 0",
            (),
        )
        .await
        .map_err(storage)?;
    transaction.query("SELECT command_id, person_id, session_id, interaction_id, expected_revision FROM agent_conversation_interaction_refreshes LIMIT 0", ()).await.map_err(storage)?;
    let mut index = transaction
        .query(
            "SELECT name FROM sqlite_schema WHERE type = 'index' AND name = 'agent_conversation_interactions_run' AND tbl_name = 'agent_conversation_interactions'",
            (),
        )
        .await
        .map_err(storage)?;
    if index.next().await.map_err(storage)?.is_none()
        || index.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(())
}

pub(super) async fn read_interaction(
    connection: &turso::Connection,
    person_id: PersonId,
    interaction_id: Uuid,
) -> Result<Option<ConversationInteraction>, AgentFailure> {
    if !table_exists(connection, "agent_conversation_interactions").await? {
        return Ok(None);
    }
    let mut rows = connection
        .query(
            "SELECT interaction_id, session_id, person_id, origin_run_id, requirement_digest, target_digest, state, revision, created_at, expires_at, payload FROM agent_conversation_interactions WHERE interaction_id = ?",
            [interaction_id.to_string()],
        )
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Ok(None);
    };
    let record = parse_row(person_id, &row).await?;
    if rows.next().await.map_err(storage)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(Some(record))
}

async fn parse_row(
    person_id: PersonId,
    row: &turso::Row,
) -> Result<ConversationInteraction, AgentFailure> {
    let payload = row.get::<String>(10).map_err(storage)?;
    if payload.len() > MAX_INTERACTION_PAYLOAD_BYTES {
        return Err(AgentFailure::StorageUnavailable);
    }
    let record: ConversationInteraction = serde_json::from_str(&payload).map_err(unavailable)?;
    record
        .validate()
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    if record.person_id != person_id
        || row.get::<String>(0).map_err(storage)? != record.id.to_string()
        || row.get::<String>(1).map_err(storage)? != record.session_id.to_string()
        || row.get::<String>(2).map_err(storage)? != record.person_id.to_string()
        || row.get::<String>(3).map_err(storage)? != record.origin_run_id.as_uuid().to_string()
        || row.get::<String>(4).map_err(storage)? != hex_digest(&record.requirement_digest)
        || row.get::<String>(5).map_err(storage)? != hex_digest(&record.target_digest)
        || row.get::<String>(6).map_err(storage)? != state_name(&record.state)
        || row.get::<i64>(7).map_err(storage)? != integer(record.revision)?
        || row.get::<i64>(8).map_err(storage)? != record.created_at_unix_ms
        || row.get::<i64>(9).map_err(storage)? != record.expires_at_unix_ms
    {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(record)
}

pub(super) async fn count_interactions(
    transaction: &Transaction<'_>,
    origin_run_id: RunId,
    active_only: bool,
) -> Result<u64, AgentFailure> {
    let sql = if active_only {
        "SELECT count(*) FROM agent_conversation_interactions WHERE origin_run_id = ? AND state IN ('pending', 'resolving')"
    } else {
        "SELECT count(*) FROM agent_conversation_interactions WHERE origin_run_id = ?"
    };
    let mut rows = transaction
        .query(sql, [origin_run_id.as_uuid().to_string()])
        .await
        .map_err(storage)?;
    let count = rows
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::VaultUnavailable)?
        .get::<i64>(0)
        .map_err(storage)?;
    u64::try_from(count).map_err(|_| AgentFailure::VaultUnavailable)
}

pub(super) async fn insert_interaction(
    transaction: &Transaction<'_>,
    record: &ConversationInteraction,
) -> Result<(), AgentFailure> {
    let payload = encode_record(record)?;
    let parameters: Vec<turso::Value> = vec![
        record.id.to_string().into(),
        record.session_id.to_string().into(),
        record.person_id.to_string().into(),
        record.origin_run_id.as_uuid().to_string().into(),
        hex_digest(&record.requirement_digest).into(),
        hex_digest(&record.target_digest).into(),
        state_name(&record.state).to_owned().into(),
        integer(record.revision)?.into(),
        record.created_at_unix_ms.into(),
        record.expires_at_unix_ms.into(),
        payload.into(),
    ];
    transaction
        .execute(
            "INSERT INTO agent_conversation_interactions (interaction_id, session_id, person_id, origin_run_id, requirement_digest, target_digest, state, revision, created_at, expires_at, payload) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            parameters,
        )
        .await
        .map_err(storage)?;
    Ok(())
}

async fn update_interaction(
    transaction: &Transaction<'_>,
    record: &ConversationInteraction,
    expected_revision: u64,
) -> Result<(), AgentFailure> {
    let changed = transaction
        .execute(
            "UPDATE agent_conversation_interactions SET state = ?, revision = ?, payload = ? WHERE interaction_id = ? AND revision = ?",
            (
                state_name(&record.state).to_owned(),
                integer(record.revision)?,
                encode_record(record)?,
                record.id.to_string(),
                integer(expected_revision)?,
            ),
        )
        .await
        .map_err(storage)?;
    if changed != 1 {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}

async fn read_decision(
    connection: &turso::Connection,
    command_id: Uuid,
) -> Result<Option<InteractionDecision>, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT command_id, interaction_id, kind, target_digest, principal, interaction_revision, decided_at FROM agent_conversation_interaction_decisions WHERE command_id = ?",
            [command_id.to_string()],
        )
        .await
        .map_err(storage)?;
    let Some(row) = rows.next().await.map_err(storage)? else {
        return Ok(None);
    };
    let decision = InteractionDecision {
        command_id: Uuid::parse_str(&row.get::<String>(0).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?,
        interaction_id: Uuid::parse_str(&row.get::<String>(1).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?,
        kind: match row.get::<String>(2).map_err(storage)?.as_str() {
            "approve" => floe_conversation::InteractionDecisionKind::Approve,
            "deny" => floe_conversation::InteractionDecisionKind::Deny,
            "dismiss" => floe_conversation::InteractionDecisionKind::Dismiss,
            _ => return Err(AgentFailure::VaultUnavailable),
        },
        target_digest: parse_digest(&row.get::<String>(3).map_err(storage)?)?,
        principal: row.get::<String>(4).map_err(storage)?,
        interaction_revision: u64::try_from(row.get::<i64>(5).map_err(storage)?)
            .map_err(|_| AgentFailure::VaultUnavailable)?,
        decided_at_unix_ms: row.get::<i64>(6).map_err(storage)?,
    };
    decision
        .validate()
        .map_err(|_| AgentFailure::VaultUnavailable)?;
    if decision.command_id != command_id || rows.next().await.map_err(storage)?.is_some() {
        return Err(AgentFailure::VaultUnavailable);
    }
    Ok(Some(decision))
}

async fn insert_decision(
    transaction: &Transaction<'_>,
    decision: &InteractionDecision,
) -> Result<(), AgentFailure> {
    transaction
        .execute(
            "INSERT INTO agent_conversation_interaction_decisions (command_id, interaction_id, kind, target_digest, principal, interaction_revision, decided_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            (
                decision.command_id.to_string(),
                decision.interaction_id.to_string(),
                decision_kind_name(decision.kind).to_owned(),
                hex_digest(&decision.target_digest),
                decision.principal.clone(),
                integer(decision.interaction_revision)?,
                decision.decided_at_unix_ms,
            ),
        )
        .await
        .map_err(storage)?;
    Ok(())
}

fn encode_record(record: &ConversationInteraction) -> Result<String, AgentFailure> {
    let payload = serde_json::to_string(record).map_err(storage)?;
    if payload.len() > MAX_INTERACTION_PAYLOAD_BYTES {
        return Err(AgentFailure::BudgetExceeded);
    }
    Ok(payload)
}

fn integer(value: u64) -> Result<i64, AgentFailure> {
    i64::try_from(value).map_err(|_| AgentFailure::InvalidInput)
}

fn state_name(state: &InteractionState) -> &'static str {
    match state {
        InteractionState::Pending => "pending",
        InteractionState::Resolving { .. } => "resolving",
        InteractionState::Resolved { .. } => "resolved",
        InteractionState::Denied { .. } => "denied",
        InteractionState::Cancelled { .. } => "cancelled",
        InteractionState::Superseded { .. } => "superseded",
        InteractionState::Expired => "expired",
    }
}

fn decision_kind_name(kind: floe_conversation::InteractionDecisionKind) -> &'static str {
    match kind {
        floe_conversation::InteractionDecisionKind::Approve => "approve",
        floe_conversation::InteractionDecisionKind::Deny => "deny",
        floe_conversation::InteractionDecisionKind::Dismiss => "dismiss",
    }
}

fn hex_digest(digest: &[u8; 32]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn parse_digest(value: &str) -> Result<[u8; 32], AgentFailure> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AgentFailure::VaultUnavailable);
    }
    let mut digest = [0; 32];
    for (index, chunk) in value.as_bytes().chunks(2).enumerate() {
        let text = std::str::from_utf8(chunk).map_err(|_| AgentFailure::VaultUnavailable)?;
        digest[index] = u8::from_str_radix(text, 16).map_err(|_| AgentFailure::VaultUnavailable)?;
    }
    Ok(digest)
}

pub(super) async fn read_group_on(
    connection: &turso::Connection,
    person_id: PersonId,
    origin_run_id: RunId,
) -> Result<Vec<ConversationInteraction>, AgentFailure> {
    if !table_exists(connection, "agent_conversation_interactions").await? {
        return Ok(Vec::new());
    }
    let mut rows = connection.query("SELECT interaction_id, session_id, person_id, origin_run_id, requirement_digest, target_digest, state, revision, created_at, expires_at, payload FROM agent_conversation_interactions WHERE origin_run_id = ? ORDER BY created_at ASC, interaction_id ASC LIMIT 65", [origin_run_id.as_uuid().to_string()]).await.map_err(storage)?;
    let mut records = Vec::new();
    while let Some(row) = rows.next().await.map_err(storage)? {
        records.push(parse_row(person_id, &row).await?);
    }
    if records.len() > MAX_STORED_INTERACTIONS_PER_RUN {
        return Err(AgentFailure::StorageUnavailable);
    }
    Ok(records)
}
async fn access_review_on(
    connection: &turso::Connection,
    person_id: PersonId,
    reference: &floe_access::ReviewRef,
) -> Result<floe_access::ConnectionReview, AgentFailure> {
    let mut rows = connection
        .query(
            "SELECT payload FROM access_connection_reviews WHERE review_id = ? AND person_id = ?",
            (reference.id.to_string(), person_id.to_string()),
        )
        .await
        .map_err(storage)?;
    let row = rows
        .next()
        .await
        .map_err(storage)?
        .ok_or(AgentFailure::Conflict)?;
    let payload = row.get::<String>(0).map_err(storage)?;
    if payload.len() > 256 * 1024 {
        return Err(AgentFailure::StorageUnavailable);
    }
    let review: floe_access::ConnectionReview =
        serde_json::from_str(&payload).map_err(unavailable)?;
    review.validate()?;
    if &review.reference != reference
        || review.person_id != person_id
        || rows.next().await.map_err(storage)?.is_some()
    {
        return Err(AgentFailure::Conflict);
    }
    Ok(review)
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn admit_conversation_interaction_refresh(
        &self,
        request: floe_conversation::InteractionRefresh,
    ) -> Result<ConversationInteraction, AgentFailure> {
        request.validate()?;
        if request.person_id != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| self.registry_transaction_start_error(error))?;
        let result = async {
            initialize(&transaction).await?;
            let mut rows = transaction.query("SELECT person_id, session_id, interaction_id, expected_revision FROM agent_conversation_interaction_refreshes WHERE command_id = ?", [request.command_id.to_string()]).await.map_err(storage)?;
            let replay = if let Some(row) = rows.next().await.map_err(storage)? {
                if row.get::<String>(0).map_err(storage)? != self.person_id.to_string() || row.get::<String>(1).map_err(storage)? != request.session_id.to_string()
                    || row.get::<String>(2).map_err(storage)? != request.interaction_id.to_string() || row.get::<i64>(3).map_err(storage)? != integer(request.expected_revision)?
                    || rows.next().await.map_err(storage)?.is_some() { return Err(AgentFailure::Conflict); }
                true
            } else { false };
            drop(rows);
            if !replay && super::conversations::command_identity_used(&transaction, request.command_id).await? { return Err(AgentFailure::Conflict); }
            let current = read_interaction(&transaction, self.person_id, request.interaction_id).await?.ok_or(AgentFailure::NotFound)?;
            let session = self.session_on(&transaction, request.session_id).await?;
            if current.session_id != request.session_id || session.person_id != self.person_id || session.scope.is_some()
                || session.data_classes != [DataClass::Personal] || (!replay && current.revision != request.expected_revision) { return Err(AgentFailure::Conflict); }
            if !replay {
                let mut count = transaction.query("SELECT count(*) FROM agent_conversation_interaction_refreshes WHERE person_id = ?", [self.person_id.to_string()]).await.map_err(storage)?;
                if count.next().await.map_err(storage)?.ok_or(AgentFailure::StorageUnavailable)?.get::<i64>(0).map_err(storage)? >= 4096 { return Err(AgentFailure::BudgetExceeded); }
                drop(count);
                transaction.execute("INSERT INTO agent_conversation_interaction_refreshes (command_id, person_id, session_id, interaction_id, expected_revision) VALUES (?, ?, ?, ?, ?)",
                    (request.command_id.to_string(), self.person_id.to_string(), request.session_id.to_string(), request.interaction_id.to_string(), integer(request.expected_revision)?)).await.map_err(storage)?;
            }
            self.check_access()?;
            Ok(current)
        }.await;
        self.finish_registry_transaction_checked(transaction, result)
            .await
    }
}

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub async fn resolving_conversation_interactions(
        &self,
        person_id: PersonId,
        limit: usize,
    ) -> Result<Vec<ConversationInteraction>, AgentFailure> {
        if person_id != self.person_id {
            return Err(AgentFailure::CapabilityDenied);
        }
        if limit == 0 || limit > 64 {
            return Err(AgentFailure::InvalidInput);
        }
        let connection = self.connection()?;
        if !table_exists(&connection, "agent_conversation_interactions").await? {
            self.check_access()?;
            return Ok(Vec::new());
        }
        let mut rows = connection.query("SELECT interaction_id FROM agent_conversation_interactions WHERE person_id = ? AND state = 'resolving' ORDER BY created_at, interaction_id LIMIT ?", (person_id.to_string(), integer(limit as u64 + 1)?)).await.map_err(storage)?;
        let mut result = Vec::new();
        while let Some(row) = rows.next().await.map_err(storage)? {
            let id =
                Uuid::parse_str(&row.get::<String>(0).map_err(storage)?).map_err(unavailable)?;
            let record = read_interaction(&connection, person_id, id)
                .await?
                .ok_or(AgentFailure::StorageUnavailable)?;
            if !matches!(record.state, InteractionState::Resolving { .. }) {
                return Err(AgentFailure::Conflict);
            }
            result.push(record);
            if result.len() > limit {
                return Err(AgentFailure::BudgetExceeded);
            }
        }
        self.check_access()?;
        Ok(result)
    }
}
