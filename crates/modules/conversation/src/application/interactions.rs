//! Trusted publication and decision commands for durable interactions.
//!
//! Publication verifies the interaction against its admitted origin: the
//! Person against the Session, the Run against the Session, and the Tool
//! call, Delegation Task or Model attempt against the origin Run's durable
//! journal identity. A forged origin conflicts even when every id is
//! well-formed. Decisions bind the reviewed target digest through
//! compare-and-swap; an identical command id rejoins the recorded decision.

use floe_agent_contract::{AgentFailure, JournalEvent, UserInteractionKind};
use floe_kernel::{PersonId, RunId};
use uuid::Uuid;

use crate::{
    ConversationInteraction, ConversationRepository, DecisionAdmission, ExpireInteraction,
    ExpireOutcome, InteractionDecision, InteractionDecisionKind, InteractionOrigin,
    InteractionRepository, InteractionRequirement, InteractionResolution, InteractionState,
    PublishAdmission, ReviewedTarget, RunState, SupersedeInteraction,
    domain::INTERACTION_PENDING_LIFETIME_MS,
    domain::{canonical_requirement_digest, canonical_target_digest, interaction_publication_id},
};

#[derive(Clone, Debug)]
pub struct PublishInteractionRequest {
    pub principal: String,
    pub session_id: Uuid,
    pub origin_run_id: RunId,
    pub origin: InteractionOrigin,
    pub kind: UserInteractionKind,
    pub requirement: InteractionRequirement,
    pub target: ReviewedTarget,
}

impl PublishInteractionRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.principal.trim() != self.principal
            || self.principal.is_empty()
            || self.principal.len() > 256
            || self.principal.chars().any(char::is_control)
            || self.session_id.is_nil()
            || !self.origin_run_id.is_valid()
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.origin
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        self.requirement
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        self.target
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        if (self.kind == UserInteractionKind::ProcessingRecipient)
            != (self.requirement.kind
                == crate::InteractionRequirementKind::ApproveProcessingRecipient)
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecideInteractionCommand {
    pub command_id: Uuid,
    pub interaction_id: Uuid,
    pub principal: String,
    pub expected_revision: u64,
    pub kind: InteractionDecisionKind,
    pub target_digest: [u8; 32],
}

impl DecideInteractionCommand {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.command_id.is_nil()
            || self.interaction_id.is_nil()
            || self.principal.trim() != self.principal
            || self.principal.is_empty()
            || self.principal.len() > 256
            || self.principal.chars().any(char::is_control)
            || self.expected_revision == 0
            || self.target_digest == [0; 32]
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

/// Publish the interaction for an admitted origin, or replay it.
///
/// A replay of the identical publication returns the same row without
/// re-verifying the origin journal: the stored row already binds the verified
/// origin, and its digests are rechecked on load. A new publication verifies
/// Person/Session/Run binding, rejects a cancelled origin Run, and requires
/// the origin call/task/attempt in the Run's durable journal.
pub async fn publish_interaction<Runs, Interactions>(
    runs: &Runs,
    interactions: &Interactions,
    request: PublishInteractionRequest,
    now_unix_ms: i64,
) -> Result<PublishAdmission, AgentFailure>
where
    Runs: ConversationRepository + ?Sized,
    Interactions: InteractionRepository + ?Sized,
{
    request.validate()?;
    if now_unix_ms < 0 {
        return Err(AgentFailure::InvalidInput);
    }
    let person_id = parse_principal(&request.principal)?;
    let requirement_digest = canonical_requirement_digest(&request.requirement)?;
    let target_digest = canonical_target_digest(&request.target)?;
    let id = interaction_publication_id(
        request.origin_run_id,
        &request.origin,
        &requirement_digest,
        &target_digest,
    )?;
    if let Some(existing) = interactions.get_interaction(person_id, id).await? {
        if existing.requirement_digest != requirement_digest
            || existing.target_digest != target_digest
            || existing.session_id != request.session_id
            || existing.origin_run_id != request.origin_run_id
            || existing.origin != request.origin
            || existing.kind != request.kind
        {
            return Err(AgentFailure::StorageUnavailable);
        }
        return Ok(PublishAdmission::Existing(existing));
    }
    let receipt = runs
        .load_receipt(request.origin_run_id)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    if receipt.principal != request.principal || receipt.session_id != request.session_id {
        return Err(AgentFailure::Conflict);
    }
    if receipt.state == RunState::Cancelled {
        return Err(AgentFailure::Conflict);
    }
    let journal = runs.load_journal(request.origin_run_id).await?;
    if !origin_admitted(&journal, &request.origin) {
        return Err(AgentFailure::Conflict);
    }
    let expires_at_unix_ms = now_unix_ms
        .checked_add(INTERACTION_PENDING_LIFETIME_MS)
        .ok_or(AgentFailure::InvalidInput)?;
    let record = ConversationInteraction {
        id,
        person_id,
        session_id: request.session_id,
        origin_run_id: request.origin_run_id,
        origin_turn_id: request.origin_run_id.as_uuid(),
        origin: request.origin,
        kind: request.kind,
        requirement: request.requirement,
        requirement_digest,
        target: request.target,
        target_digest,
        state: InteractionState::Pending,
        revision: 1,
        created_at_unix_ms: now_unix_ms,
        expires_at_unix_ms,
    };
    record.validate().map_err(|_| AgentFailure::InvalidInput)?;
    interactions.publish_interaction(record).await
}

/// Decide a Pending interaction, or rejoin an identical recorded decision.
///
/// Deciding a lapsed interaction persists Expired and conflicts; it never
/// approves stale review. Terminal states only rejoin through the identical
/// command id.
pub async fn decide_interaction<Interactions>(
    interactions: &Interactions,
    command: DecideInteractionCommand,
    now_unix_ms: i64,
) -> Result<DecisionAdmission, AgentFailure>
where
    Interactions: InteractionRepository,
{
    command.validate()?;
    if now_unix_ms < 0 {
        return Err(AgentFailure::InvalidInput);
    }
    let person_id = parse_principal(&command.principal)?;
    let current = interactions
        .get_interaction(person_id, command.interaction_id)
        .await?
        .ok_or(AgentFailure::NotFound)?;
    if current.projects_expired_at(now_unix_ms) {
        let expire = ExpireInteraction {
            interaction_id: command.interaction_id,
            person_id,
            now_unix_ms,
        };
        expire.validate()?;
        match interactions.mark_expired(expire).await {
            Ok(_) | Err(AgentFailure::Conflict) => return Err(AgentFailure::Conflict),
            Err(failure) => return Err(failure),
        }
    }
    let decision = InteractionDecision {
        command_id: command.command_id,
        interaction_id: command.interaction_id,
        interaction_revision: command.expected_revision,
        kind: command.kind,
        target_digest: command.target_digest,
        principal: command.principal,
        decided_at_unix_ms: now_unix_ms,
    };
    decision.validate()?;
    interactions.record_decision(decision).await
}

pub async fn resolve_interaction<Interactions>(
    interactions: &Interactions,
    resolution: InteractionResolution,
) -> Result<ConversationInteraction, AgentFailure>
where
    Interactions: InteractionRepository,
{
    resolution.validate()?;
    interactions.record_resolution(resolution).await
}

pub async fn supersede_interaction<Interactions>(
    interactions: &Interactions,
    supersede: SupersedeInteraction,
) -> Result<ConversationInteraction, AgentFailure>
where
    Interactions: InteractionRepository,
{
    supersede.validate()?;
    interactions.mark_superseded(supersede).await
}

pub async fn expire_interaction<Interactions>(
    interactions: &Interactions,
    expire: ExpireInteraction,
) -> Result<ExpireOutcome, AgentFailure>
where
    Interactions: InteractionRepository,
{
    expire.validate()?;
    interactions.mark_expired(expire).await
}

pub async fn load_interaction<Interactions>(
    interactions: &Interactions,
    principal: &str,
    interaction_id: Uuid,
) -> Result<ConversationInteraction, AgentFailure>
where
    Interactions: InteractionRepository,
{
    if interaction_id.is_nil() {
        return Err(AgentFailure::InvalidInput);
    }
    let person_id = parse_principal(principal)?;
    interactions
        .get_interaction(person_id, interaction_id)
        .await?
        .ok_or(AgentFailure::NotFound)
}

pub async fn list_run_interactions<Interactions>(
    interactions: &Interactions,
    principal: &str,
    origin_run_id: RunId,
) -> Result<Vec<ConversationInteraction>, AgentFailure>
where
    Interactions: InteractionRepository,
{
    if !origin_run_id.is_valid() {
        return Err(AgentFailure::InvalidInput);
    }
    let person_id = parse_principal(principal)?;
    interactions
        .list_run_interactions(person_id, origin_run_id)
        .await
}

fn origin_admitted(journal: &[crate::JournalEntry], origin: &InteractionOrigin) -> bool {
    journal.iter().any(|entry| match (&entry.event, origin) {
        (JournalEvent::ToolIntent { call }, InteractionOrigin::Tool { call_id }) => {
            call.call_id == *call_id
        }
        (JournalEvent::DelegationIntent { request }, InteractionOrigin::Task { task_id, .. }) => {
            request.task_id.as_uuid() == *task_id
        }
        (
            JournalEvent::ModelIntent { attempt_id, .. },
            InteractionOrigin::Model {
                attempt_id: expected,
            },
        ) => attempt_id == expected,
        _ => false,
    })
}

fn parse_principal(principal: &str) -> Result<PersonId, AgentFailure> {
    if principal.trim() != principal
        || principal.is_empty()
        || principal.len() > 256
        || principal.chars().any(char::is_control)
    {
        return Err(AgentFailure::InvalidInput);
    }
    let id = Uuid::parse_str(principal).map_err(|_| AgentFailure::InvalidInput)?;
    PersonId::from_uuid(id).ok_or(AgentFailure::InvalidInput)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use floe_agent_contract::{
        AgentContext, BoxFuture, DelegationExecutionContext, DelegationRequest, ExecutionJournal,
        ProjectionRef, TaskId, ToolCall,
    };
    use floe_kernel::CommandId;

    use crate::domain::{
        MAX_ACTIVE_INTERACTIONS_PER_RUN, MAX_STORED_INTERACTIONS_PER_RUN,
        next_state_after_decision, state_after_resolution,
    };
    use crate::{
        AdmittedTurn, CancelRunAdmission, CancelRunCommand, CommandQuery,
        InteractionRequirementKind, JournalEntry, RecoveryReceipt, RecoveryRequest, RunReceipt,
        RunTerminal, TurnAdmission, TurnAdmissionRequest,
    };

    use super::*;

    const NOW: i64 = 1_700_000_000_000;

    fn person() -> PersonId {
        PersonId::new()
    }

    fn requirement() -> InteractionRequirement {
        InteractionRequirement {
            kind: InteractionRequirementKind::EnableObserve,
            source_id: "floe.source.calendar".into(),
            connection_id: Some("calendar-connection".into()),
            consumer: "floe.builtin.schedule".into(),
            purpose: "scheduling".into(),
            inline: true,
        }
    }

    fn target() -> ReviewedTarget {
        ReviewedTarget::InlineObserve(crate::InlineObserveTarget {
            connection_id: "calendar-connection".into(),
            device_id: None,
            source_id: "floe.source.calendar".into(),
            connector_id: Some("floe.connector.calendar".into()),
            consumer: "floe.builtin.schedule".into(),
            purpose: "scheduling".into(),
            connection_revision: None,
            reviewed_producer_fingerprint: None,
            reviewed_native_subject: None,
            members: vec![crate::ReviewedBundleMember {
                member_id: "calendar.timeline".into(),
                resource: "personal".into(),
                source_revision: None,
                expected_grant: crate::ExpectedGrantState::Absent,
                policy_authority: None,
            }],
        })
    }

    fn receipt(person_id: PersonId, session_id: Uuid, run_id: RunId) -> RunReceipt {
        RunReceipt {
            run_id,
            command_id: CommandId::new(),
            session_id,
            principal: person_id.to_string(),
            request_digest: [1; 32],
            state: RunState::Working,
            output: None,
            coverage: floe_agent_contract::DependencyCoverage::Unknown,
            issue: None,
            session_revision: 1,
            aggregate_revision: 1,
            executor_generation: 1,
            continuation_of: None,
            continuation_executor_generation: None,
            continuation_level: 0,
            retry_of: None,
            profile: crate::ProfileSelection::Auto,
            attempt_refs: vec![],
            task_refs: vec![],
        }
    }

    fn tool_call(call_id: Uuid) -> ToolCall {
        ToolCall {
            call_id,
            invocation_key: floe_agent_contract::InvocationKey::from_uuid(Uuid::new_v4()).unwrap(),
            tool_id: "calendar.observe".into(),
            definition_revision: 1,
            input: "{}".into(),
        }
    }

    struct StubRuns {
        receipts: Mutex<HashMap<RunId, RunReceipt>>,
        journal: Mutex<HashMap<RunId, Vec<JournalEntry>>>,
    }

    impl StubRuns {
        fn new() -> Self {
            Self {
                receipts: Mutex::new(HashMap::new()),
                journal: Mutex::new(HashMap::new()),
            }
        }
    }

    struct MemoryInteractions {
        records: Mutex<HashMap<Uuid, ConversationInteraction>>,
        decisions: Mutex<HashMap<Uuid, InteractionDecision>>,
    }

    impl MemoryInteractions {
        fn new() -> Self {
            Self {
                records: Mutex::new(HashMap::new()),
                decisions: Mutex::new(HashMap::new()),
            }
        }
    }

    impl InteractionRepository for MemoryInteractions {
        fn publish_interaction<'a>(
            &'a self,
            record: ConversationInteraction,
        ) -> BoxFuture<'a, Result<PublishAdmission, AgentFailure>> {
            Box::pin(async move {
                record.validate().map_err(|_| AgentFailure::InvalidInput)?;
                let mut records = self.records.lock().unwrap();
                if let Some(existing) = records.get(&record.id) {
                    if existing.requirement_digest != record.requirement_digest
                        || existing.target_digest != record.target_digest
                        || existing.origin_run_id != record.origin_run_id
                        || existing.origin != record.origin
                    {
                        return Err(AgentFailure::StorageUnavailable);
                    }
                    if existing.person_id != record.person_id {
                        return Err(AgentFailure::CapabilityDenied);
                    }
                    return Ok(PublishAdmission::Existing(existing.clone()));
                }
                let run_records: Vec<_> = records
                    .values()
                    .filter(|candidate| candidate.origin_run_id == record.origin_run_id)
                    .collect();
                if run_records.len() >= MAX_STORED_INTERACTIONS_PER_RUN {
                    return Err(AgentFailure::BudgetExceeded);
                }
                if run_records
                    .iter()
                    .filter(|candidate| !candidate.state.is_terminal())
                    .count()
                    >= MAX_ACTIVE_INTERACTIONS_PER_RUN
                {
                    return Err(AgentFailure::BudgetExceeded);
                }
                records.insert(record.id, record.clone());
                Ok(PublishAdmission::Created(record))
            })
        }

        fn get_interaction<'a>(
            &'a self,
            person_id: PersonId,
            interaction_id: Uuid,
        ) -> BoxFuture<'a, Result<Option<ConversationInteraction>, AgentFailure>> {
            Box::pin(async move {
                let records = self.records.lock().unwrap();
                let record = records.get(&interaction_id).cloned();
                if record
                    .as_ref()
                    .is_some_and(|record| record.person_id != person_id)
                {
                    return Ok(None);
                }
                if let Some(record) = &record {
                    record.validate()?;
                }
                Ok(record)
            })
        }

        fn list_run_interactions<'a>(
            &'a self,
            person_id: PersonId,
            origin_run_id: RunId,
        ) -> BoxFuture<'a, Result<Vec<ConversationInteraction>, AgentFailure>> {
            Box::pin(async move {
                let records = self.records.lock().unwrap();
                let mut listed: Vec<_> = records
                    .values()
                    .filter(|record| {
                        record.person_id == person_id && record.origin_run_id == origin_run_id
                    })
                    .cloned()
                    .collect();
                listed.sort_by_key(|record| record.created_at_unix_ms);
                for record in &listed {
                    record.validate()?;
                }
                Ok(listed)
            })
        }

        fn record_decision<'a>(
            &'a self,
            decision: InteractionDecision,
        ) -> BoxFuture<'a, Result<DecisionAdmission, AgentFailure>> {
            Box::pin(async move {
                decision.validate()?;
                let mut decisions = self.decisions.lock().unwrap();
                if let Some(recorded) = decisions.get(&decision.command_id) {
                    if !decision.matches_recorded(recorded) {
                        return Err(AgentFailure::Conflict);
                    }
                    let records = self.records.lock().unwrap();
                    let current = records
                        .get(&decision.interaction_id)
                        .ok_or(AgentFailure::StorageUnavailable)?;
                    current.validate()?;
                    return Ok(DecisionAdmission::Rejoined(current.clone()));
                }
                let mut records = self.records.lock().unwrap();
                let current = records
                    .get(&decision.interaction_id)
                    .cloned()
                    .ok_or(AgentFailure::NotFound)?;
                current.validate()?;
                if current.person_id.to_string() != decision.principal
                    || current.revision != decision.interaction_revision
                    || current.target_digest != decision.target_digest
                    || decision.decided_at_unix_ms < current.created_at_unix_ms
                    || decision.decided_at_unix_ms >= current.expires_at_unix_ms
                {
                    return Err(AgentFailure::Conflict);
                }
                let next = next_state_after_decision(&current.state, &decision)?;
                let mut updated = current;
                updated.state = next;
                updated.revision += 1;
                updated.validate()?;
                decisions.insert(decision.command_id, decision);
                records.insert(updated.id, updated.clone());
                Ok(DecisionAdmission::Applied(updated))
            })
        }

        fn record_resolution<'a>(
            &'a self,
            resolution: InteractionResolution,
        ) -> BoxFuture<'a, Result<ConversationInteraction, AgentFailure>> {
            Box::pin(async move {
                resolution.validate()?;
                let decisions = self.decisions.lock().unwrap();
                let recorded = decisions
                    .get(&resolution.decision_id)
                    .ok_or(AgentFailure::Conflict)?;
                if recorded.interaction_id != resolution.interaction_id
                    || resolution.resolved_at_unix_ms < recorded.decided_at_unix_ms
                {
                    return Err(AgentFailure::Conflict);
                }
                let mut records = self.records.lock().unwrap();
                let current = records
                    .get(&resolution.interaction_id)
                    .cloned()
                    .ok_or(AgentFailure::NotFound)?;
                current.validate()?;
                if current.person_id != resolution.person_id
                    || current.revision != resolution.expected_revision
                {
                    return Err(AgentFailure::Conflict);
                }
                let next = state_after_resolution(
                    &current.state,
                    resolution.decision_id,
                    resolution.owner_operation_id,
                    resolution.resolved_at_unix_ms,
                )?;
                let mut updated = current;
                updated.state = next;
                updated.revision += 1;
                updated.validate()?;
                records.insert(updated.id, updated.clone());
                Ok(updated)
            })
        }

        fn mark_superseded<'a>(
            &'a self,
            supersede: SupersedeInteraction,
        ) -> BoxFuture<'a, Result<ConversationInteraction, AgentFailure>> {
            Box::pin(async move {
                supersede.validate()?;
                let mut records = self.records.lock().unwrap();
                let current = records
                    .get(&supersede.interaction_id)
                    .cloned()
                    .ok_or(AgentFailure::NotFound)?;
                current.validate()?;
                if current.person_id != supersede.person_id
                    || current.revision != supersede.expected_revision
                    || current.state.is_terminal()
                {
                    return Err(AgentFailure::Conflict);
                }
                let mut updated = current;
                updated.state = InteractionState::Superseded {
                    superseded_by: supersede.superseded_by,
                };
                updated.revision += 1;
                updated.validate()?;
                records.insert(updated.id, updated.clone());
                Ok(updated)
            })
        }

        fn mark_expired<'a>(
            &'a self,
            expire: ExpireInteraction,
        ) -> BoxFuture<'a, Result<ExpireOutcome, AgentFailure>> {
            Box::pin(async move {
                expire.validate()?;
                let mut records = self.records.lock().unwrap();
                let current = records
                    .get(&expire.interaction_id)
                    .cloned()
                    .ok_or(AgentFailure::NotFound)?;
                current.validate()?;
                if current.person_id != expire.person_id {
                    return Err(AgentFailure::NotFound);
                }
                if current.state.is_terminal() {
                    return Ok(ExpireOutcome::AlreadyTerminal(current));
                }
                if expire.now_unix_ms < current.expires_at_unix_ms {
                    return Ok(ExpireOutcome::NotExpired(current));
                }
                let mut updated = current;
                updated.state = InteractionState::Expired;
                updated.revision += 1;
                updated.validate()?;
                records.insert(updated.id, updated.clone());
                Ok(ExpireOutcome::Expired(updated))
            })
        }
    }

    impl ConversationRepository for StubRuns {
        fn find_command<'a>(
            &'a self,
            _query: CommandQuery,
        ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>> {
            Box::pin(async move { Ok(None) })
        }

        fn admit_turn<'a>(
            &'a self,
            _request: TurnAdmissionRequest,
        ) -> BoxFuture<'a, Result<TurnAdmission, AgentFailure>> {
            Box::pin(async move { Err(AgentFailure::InvalidInput) })
        }

        fn admit_cancel<'a>(
            &'a self,
            _request: CancelRunCommand,
        ) -> BoxFuture<'a, Result<CancelRunAdmission, AgentFailure>> {
            Box::pin(async move { Err(AgentFailure::InvalidInput) })
        }

        fn journal(&self, _run_id: RunId) -> Result<Arc<dyn ExecutionJournal>, AgentFailure> {
            Err(AgentFailure::InvalidInput)
        }

        fn finish_run<'a>(
            &'a self,
            _run_id: RunId,
            _expected_aggregate_revision: u64,
            _terminal: RunTerminal,
        ) -> BoxFuture<'a, Result<RunReceipt, AgentFailure>> {
            Box::pin(async move { Err(AgentFailure::InvalidInput) })
        }

        fn load_run<'a>(
            &'a self,
            _run_id: RunId,
        ) -> BoxFuture<'a, Result<Option<AdmittedTurn>, AgentFailure>> {
            Box::pin(async move { Ok(None) })
        }

        fn load_receipt<'a>(
            &'a self,
            run_id: RunId,
        ) -> BoxFuture<'a, Result<Option<RunReceipt>, AgentFailure>> {
            Box::pin(async move { Ok(self.receipts.lock().unwrap().get(&run_id).cloned()) })
        }

        fn recover_session<'a>(
            &'a self,
            _request: RecoveryRequest,
        ) -> BoxFuture<'a, Result<RecoveryReceipt, AgentFailure>> {
            Box::pin(async move { Err(AgentFailure::InvalidInput) })
        }

        fn load_journal<'a>(
            &'a self,
            run_id: RunId,
        ) -> BoxFuture<'a, Result<Vec<JournalEntry>, AgentFailure>> {
            Box::pin(async move {
                Ok(self
                    .journal
                    .lock()
                    .unwrap()
                    .get(&run_id)
                    .cloned()
                    .unwrap_or_default())
            })
        }
    }

    fn publish_request(
        person_id: PersonId,
        session_id: Uuid,
        run_id: RunId,
        origin: InteractionOrigin,
    ) -> PublishInteractionRequest {
        PublishInteractionRequest {
            principal: person_id.to_string(),
            session_id,
            origin_run_id: run_id,
            origin,
            kind: UserInteractionKind::SourceAccess,
            requirement: requirement(),
            target: target(),
        }
    }

    fn working_run(
        runs: &StubRuns,
        person_id: PersonId,
        session_id: Uuid,
        run_id: RunId,
        origin: &InteractionOrigin,
    ) {
        let event = match origin {
            InteractionOrigin::Tool { call_id } => JournalEvent::ToolIntent {
                call: tool_call(*call_id),
            },
            InteractionOrigin::Task { task_id, .. } => JournalEvent::DelegationIntent {
                request: DelegationRequest {
                    task_id: TaskId::from_uuid(*task_id).unwrap(),
                    parent_run_id: Some(run_id.as_uuid()),
                    principal: person_id.to_string(),
                    invocation_key: floe_agent_contract::InvocationKey::new(),
                    selected_agent_id: "agent".into(),
                    selected_definition_revision: 1,
                    message: "task".into(),
                    context_refs: vec![],
                    execution_context: DelegationExecutionContext {
                        session_id,
                        device_id: "mac-local".into(),
                        agent_context: AgentContext {
                            projection_version: 1,
                            persona: None,
                            memories: vec![],
                            optional_context_issues: vec![],
                            evidence: vec![],
                        },
                        max_output_bytes: floe_agent_contract::MAX_OUTPUT_BYTES,
                    },
                },
            },
            InteractionOrigin::Model { attempt_id } => JournalEvent::ModelIntent {
                attempt_id: *attempt_id,
                projection_ref: ProjectionRef::new(),
            },
        };
        runs.receipts
            .lock()
            .unwrap()
            .insert(run_id, receipt(person_id, session_id, run_id));
        runs.journal
            .lock()
            .unwrap()
            .insert(run_id, vec![JournalEntry { revision: 1, event }]);
    }

    #[tokio::test]
    async fn publish_verifies_origin_and_replays_identically() {
        let runs = StubRuns::new();
        let interactions = MemoryInteractions::new();
        let person_id = person();
        let session_id = Uuid::new_v4();
        let run_id = RunId::new();
        let origin = InteractionOrigin::Tool {
            call_id: Uuid::new_v4(),
        };
        working_run(&runs, person_id, session_id, run_id, &origin);
        let request = publish_request(person_id, session_id, run_id, origin);

        let PublishAdmission::Created(first) =
            publish_interaction(&runs, &interactions, request.clone(), NOW)
                .await
                .unwrap()
        else {
            panic!("first publish must create");
        };
        assert_eq!(first.revision, 1);
        assert_eq!(first.state, InteractionState::Pending);

        let PublishAdmission::Existing(second) =
            publish_interaction(&runs, &interactions, request, NOW + 1)
                .await
                .unwrap()
        else {
            panic!("replay must rejoin");
        };
        assert_eq!(first, second);
    }

    #[tokio::test]
    async fn publish_rejects_foreign_and_forged_origins() {
        let runs = StubRuns::new();
        let interactions = MemoryInteractions::new();
        let person_id = person();
        let session_id = Uuid::new_v4();
        let run_id = RunId::new();
        let origin = InteractionOrigin::Tool {
            call_id: Uuid::new_v4(),
        };
        working_run(&runs, person_id, session_id, run_id, &origin);

        let mut foreign_person = publish_request(person_id, session_id, run_id, origin.clone());
        foreign_person.principal = person().to_string();
        assert_eq!(
            publish_interaction(&runs, &interactions, foreign_person, NOW).await,
            Err(AgentFailure::Conflict)
        );

        let mut foreign_session = publish_request(person_id, session_id, run_id, origin.clone());
        foreign_session.session_id = Uuid::new_v4();
        assert_eq!(
            publish_interaction(&runs, &interactions, foreign_session, NOW).await,
            Err(AgentFailure::Conflict)
        );

        let forged_call = publish_request(
            person_id,
            session_id,
            run_id,
            InteractionOrigin::Tool {
                call_id: Uuid::new_v4(),
            },
        );
        assert_eq!(
            publish_interaction(&runs, &interactions, forged_call, NOW).await,
            Err(AgentFailure::Conflict)
        );

        let forged_task = publish_request(
            person_id,
            session_id,
            run_id,
            InteractionOrigin::Task {
                task_id: Uuid::new_v4(),
                capability_call_id: None,
            },
        );
        assert_eq!(
            publish_interaction(&runs, &interactions, forged_task, NOW).await,
            Err(AgentFailure::Conflict)
        );

        let unknown_run = publish_request(person_id, session_id, RunId::new(), origin);
        assert_eq!(
            publish_interaction(&runs, &interactions, unknown_run, NOW).await,
            Err(AgentFailure::NotFound)
        );
    }

    #[tokio::test]
    async fn publish_rejects_cancelled_origin_run() {
        let runs = StubRuns::new();
        let interactions = MemoryInteractions::new();
        let person_id = person();
        let session_id = Uuid::new_v4();
        let run_id = RunId::new();
        let origin = InteractionOrigin::Tool {
            call_id: Uuid::new_v4(),
        };
        working_run(&runs, person_id, session_id, run_id, &origin);
        runs.receipts
            .lock()
            .unwrap()
            .get_mut(&run_id)
            .unwrap()
            .state = RunState::Cancelled;
        runs.receipts
            .lock()
            .unwrap()
            .get_mut(&run_id)
            .unwrap()
            .issue = Some(AgentFailure::Cancelled);

        let request = publish_request(person_id, session_id, run_id, origin);
        assert_eq!(
            publish_interaction(&runs, &interactions, request, NOW).await,
            Err(AgentFailure::Conflict)
        );
    }

    #[tokio::test]
    async fn decision_rejoins_identical_command_and_conflicts_on_digest() {
        let runs = StubRuns::new();
        let interactions = MemoryInteractions::new();
        let person_id = person();
        let session_id = Uuid::new_v4();
        let run_id = RunId::new();
        let origin = InteractionOrigin::Tool {
            call_id: Uuid::new_v4(),
        };
        working_run(&runs, person_id, session_id, run_id, &origin);
        let request = publish_request(person_id, session_id, run_id, origin);
        let PublishAdmission::Created(created) =
            publish_interaction(&runs, &interactions, request, NOW)
                .await
                .unwrap()
        else {
            panic!("publish must create");
        };

        let command = DecideInteractionCommand {
            command_id: Uuid::new_v4(),
            interaction_id: created.id,
            principal: person_id.to_string(),
            expected_revision: 1,
            kind: InteractionDecisionKind::Approve,
            target_digest: created.target_digest,
        };
        let DecisionAdmission::Applied(applied) =
            decide_interaction(&interactions, command.clone(), NOW + 1)
                .await
                .unwrap()
        else {
            panic!("decision must apply");
        };
        assert!(matches!(applied.state, InteractionState::Resolving { .. }));

        let DecisionAdmission::Rejoined(rejoined) =
            decide_interaction(&interactions, command.clone(), NOW + 2)
                .await
                .unwrap()
        else {
            panic!("identical retry must rejoin");
        };
        assert_eq!(rejoined, applied);

        let mut conflicted = command.clone();
        conflicted.target_digest = [9; 32];
        assert_eq!(
            decide_interaction(&interactions, conflicted, NOW + 3).await,
            Err(AgentFailure::Conflict)
        );

        let mut stale = command.clone();
        stale.command_id = Uuid::new_v4();
        stale.expected_revision = 1;
        assert_eq!(
            decide_interaction(&interactions, stale, NOW + 3).await,
            Err(AgentFailure::Conflict)
        );
    }

    #[tokio::test]
    async fn lapsed_decision_persists_expired_and_conflicts() {
        let runs = StubRuns::new();
        let interactions = MemoryInteractions::new();
        let person_id = person();
        let session_id = Uuid::new_v4();
        let run_id = RunId::new();
        let origin = InteractionOrigin::Tool {
            call_id: Uuid::new_v4(),
        };
        working_run(&runs, person_id, session_id, run_id, &origin);
        let request = publish_request(person_id, session_id, run_id, origin);
        let PublishAdmission::Created(created) =
            publish_interaction(&runs, &interactions, request, NOW)
                .await
                .unwrap()
        else {
            panic!("publish must create");
        };

        let command = DecideInteractionCommand {
            command_id: Uuid::new_v4(),
            interaction_id: created.id,
            principal: person_id.to_string(),
            expected_revision: 1,
            kind: InteractionDecisionKind::Approve,
            target_digest: created.target_digest,
        };
        assert_eq!(
            decide_interaction(&interactions, command, created.expires_at_unix_ms).await,
            Err(AgentFailure::Conflict)
        );
        let stored = load_interaction(&interactions, &person_id.to_string(), created.id)
            .await
            .unwrap();
        assert_eq!(stored.state, InteractionState::Expired);
    }

    #[tokio::test]
    async fn forged_reference_fails_trusted_lookup() {
        let interactions = MemoryInteractions::new();
        assert_eq!(
            load_interaction(&interactions, &person().to_string(), Uuid::new_v4()).await,
            Err(AgentFailure::NotFound)
        );
        assert_eq!(
            load_interaction(&interactions, &person().to_string(), Uuid::nil()).await,
            Err(AgentFailure::InvalidInput)
        );
    }

    #[tokio::test]
    async fn publish_rejects_nil_origin_references() {
        let runs = StubRuns::new();
        let interactions = MemoryInteractions::new();
        let person_id = person();
        let session_id = Uuid::new_v4();
        let run_id = RunId::new();
        for origin in [
            InteractionOrigin::Tool {
                call_id: Uuid::nil(),
            },
            InteractionOrigin::Task {
                task_id: Uuid::nil(),
                capability_call_id: None,
            },
            InteractionOrigin::Task {
                task_id: Uuid::new_v4(),
                capability_call_id: Some(Uuid::nil()),
            },
            InteractionOrigin::Model {
                attempt_id: Uuid::nil(),
            },
        ] {
            let request = publish_request(person_id, session_id, run_id, origin);
            assert_eq!(
                publish_interaction(&runs, &interactions, request, NOW).await,
                Err(AgentFailure::InvalidInput)
            );
        }
    }

    #[tokio::test]
    async fn publish_accepts_task_and_model_origins() {
        let runs = StubRuns::new();
        let interactions = MemoryInteractions::new();
        let person_id = person();
        let session_id = Uuid::new_v4();
        let run_id = RunId::new();
        let origins = [
            InteractionOrigin::Task {
                task_id: Uuid::new_v4(),
                capability_call_id: Some(Uuid::new_v4()),
            },
            InteractionOrigin::Model {
                attempt_id: Uuid::new_v4(),
            },
        ];
        for origin in origins {
            working_run(&runs, person_id, session_id, run_id, &origin);
            let request = publish_request(person_id, session_id, run_id, origin);
            assert!(matches!(
                publish_interaction(&runs, &interactions, request, NOW)
                    .await
                    .unwrap(),
                PublishAdmission::Created(_)
            ));
        }
        assert_eq!(
            list_run_interactions(&interactions, &person_id.to_string(), run_id)
                .await
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn resolution_requires_the_recorded_decision_binding() {
        let runs = StubRuns::new();
        let interactions = MemoryInteractions::new();
        let person_id = person();
        let session_id = Uuid::new_v4();
        let run_id = RunId::new();
        let origin = InteractionOrigin::Tool {
            call_id: Uuid::new_v4(),
        };
        working_run(&runs, person_id, session_id, run_id, &origin);
        let request = publish_request(person_id, session_id, run_id, origin);
        let PublishAdmission::Created(created) =
            publish_interaction(&runs, &interactions, request, NOW)
                .await
                .unwrap()
        else {
            panic!("publish must create");
        };
        let command = DecideInteractionCommand {
            command_id: Uuid::new_v4(),
            interaction_id: created.id,
            principal: person_id.to_string(),
            expected_revision: 1,
            kind: InteractionDecisionKind::Approve,
            target_digest: created.target_digest,
        };
        let DecisionAdmission::Applied(applied) =
            decide_interaction(&interactions, command.clone(), NOW + 1)
                .await
                .unwrap()
        else {
            panic!("decision must apply");
        };
        let InteractionState::Resolving {
            decision_id,
            owner_operation_id,
        } = applied.state
        else {
            panic!("approve must resolve");
        };

        let wrong_operation = InteractionResolution {
            interaction_id: created.id,
            person_id,
            expected_revision: applied.revision,
            decision_id,
            owner_operation_id: Uuid::new_v4(),
            resolved_at_unix_ms: NOW + 2,
        };
        assert_eq!(
            resolve_interaction(&interactions, wrong_operation).await,
            Err(AgentFailure::Conflict)
        );

        let resolution = InteractionResolution {
            interaction_id: created.id,
            person_id,
            expected_revision: applied.revision,
            decision_id,
            owner_operation_id,
            resolved_at_unix_ms: NOW + 2,
        };
        let resolved = resolve_interaction(&interactions, resolution)
            .await
            .unwrap();
        assert!(matches!(resolved.state, InteractionState::Resolved { .. }));
    }
}
