use std::future::Future;

use floe_access::DependencyCoverage;
use floe_conversation::AgentMessage;
use floe_experts::AgentRegistry;
use turso::transaction::TransactionBehavior;

use super::*;
use floe_actions::{
    EXPERT_CALENDAR_PROPOSAL_MEDIA_TYPE, ExpertCalendarProposal, ExpertProposalReference,
};

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(crate) async fn expert_proposal_dependency(
        &self,
        reference: &ExpertProposalReference,
        evidence: &ExpertCalendarProposal,
    ) -> Result<floe_access::ContextDependency, AgentFailure> {
        if reference.person_id != self.person_id
            || evidence.person_id != self.person_id
            || evidence.task_id != reference.invocation_id
        {
            return Err(AgentFailure::NotFound);
        }
        let task_id = floe_agent_contract::TaskId::from_uuid(reference.invocation_id)
            .ok_or(AgentFailure::InvalidInput)?;
        let task = self.task(task_id).await?.ok_or(AgentFailure::NotFound)?;
        if task.snapshot.state != floe_agent_contract::TaskState::Completed
            || task.snapshot.agent_id != evidence.package.id
        {
            return Err(AgentFailure::Conflict);
        }
        let DependencyCoverage::Dependent { dependencies } = &task.snapshot.coverage else {
            return Err(AgentFailure::PolicyDenied);
        };
        let matches: Vec<_> = dependencies
            .iter()
            .filter(|dependency| dependency.observation_id() == evidence.evidence_id)
            .collect();
        let [dependency] = matches.as_slice() else {
            return Err(AgentFailure::PolicyDenied);
        };
        let dependency = (*dependency).clone();
        if dependency.person_id() != self.person_id
            || dependency.source().person_id() != self.person_id
            || dependency.consumer().identifier() != evidence.package.id
            || dependency.operation() != floe_access::GrantOperation::Read
            || dependency.purpose() != floe_access::GrantPurpose::Assistant
            || dependency.expires_at() <= chrono::Utc::now()
            || dependency.resources().is_empty()
            || !matches!(
                dependency.source().connector().as_str(),
                "calendar.event_kit"
                    | "calendar.android"
                    | "calendar.google"
                    | "calendar.microsoft"
            )
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let grant = self.get_data_access_grant(dependency.grant_id()).await?;
        floe_access::validate_grant_dependency(&grant, &dependency)?;
        let policy = self
            .calendar_grant_policy(dependency.grant_id())
            .await
            .map_err(|error| match error {
                AgentFailure::AccessReviewRequired => AgentFailure::PolicyDenied,
                other => other,
            })?;
        if policy.consumer_policy != dependency.consumer_policy() {
            return Err(AgentFailure::PolicyDenied);
        }
        Ok(dependency)
    }

    pub(crate) async fn with_expert_proposal<ResultValue, Publish>(
        &self,
        reference: &ExpertProposalReference,
        publish: impl FnOnce(ExpertCalendarProposal) -> Publish,
    ) -> Result<ResultValue, AgentFailure>
    where
        Publish: Future<Output = Result<ResultValue, AgentFailure>>,
    {
        self.with_proposal_evidence(reference, true, publish).await
    }

    pub(crate) async fn with_recorded_expert_proposal<ResultValue, Inspect>(
        &self,
        reference: &ExpertProposalReference,
        inspect: impl FnOnce(ExpertCalendarProposal) -> Inspect,
    ) -> Result<ResultValue, AgentFailure>
    where
        Inspect: Future<Output = Result<ResultValue, AgentFailure>>,
    {
        self.with_proposal_evidence(reference, false, inspect).await
    }

    async fn with_proposal_evidence<ResultValue, Operation>(
        &self,
        reference: &ExpertProposalReference,
        require_active: bool,
        operation: impl FnOnce(ExpertCalendarProposal) -> Operation,
    ) -> Result<ResultValue, AgentFailure>
    where
        Operation: Future<Output = Result<ResultValue, AgentFailure>>,
    {
        if reference.person_id != self.person_id {
            return Err(AgentFailure::NotFound);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let session = self.session_on(&transaction, reference.session_id).await?;
            let snapshot = self
                .registry_on(&transaction)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            let task_id = floe_agent_contract::TaskId::from_uuid(reference.invocation_id)
                .ok_or(AgentFailure::InvalidInput)?;
            let mut binding = transaction
                .query(
                    "SELECT session_id, turn_id FROM agent_task_delegations WHERE task_id = ?",
                    [reference.invocation_id.to_string()],
                )
                .await
                .map_err(storage)?;
            let bound_turn = binding
                .next()
                .await
                .map_err(storage)?
                .ok_or(AgentFailure::NotFound)?;
            let recorded_session = bound_turn.get::<String>(0).map_err(storage)?;
            let bound_turn = bound_turn.get::<String>(1).map_err(storage)?;
            if recorded_session != reference.session_id.to_string() {
                return Err(AgentFailure::Conflict);
            }
            if binding.next().await.map_err(storage)?.is_some() {
                return Err(AgentFailure::Conflict);
            }
            drop(binding);
            let recorded = self
                .task_on(&transaction, task_id)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            let task_snapshot = &recorded.snapshot;
            if task_snapshot.state != floe_agent_contract::TaskState::Completed
                || task_snapshot.task_id != task_id
                || task_snapshot.principal != reference.person_id.to_string()
            {
                return Err(AgentFailure::Conflict);
            }
            let outputs: Vec<_> = session
                .messages
                .iter()
                .filter_map(|message| match message {
                    AgentMessage::Delegation { turn_id, task }
                        if task.id == reference.invocation_id
                            && turn_id.to_string() == bound_turn
                            && task.state == floe_experts::A2ATaskState::Completed =>
                    {
                        Some(task)
                    }
                    _ => None,
                })
                .flat_map(|task| &task.artifacts)
                .flat_map(|artifact| {
                    artifact.parts.iter().filter_map(move |part| match part {
                        floe_experts::A2APart::Data { media_type, data }
                            if media_type == EXPERT_CALENDAR_PROPOSAL_MEDIA_TYPE =>
                        {
                            Some((artifact.artifact_id, data.as_str()))
                        }
                        _ => None,
                    })
                })
                .collect();
            let [(artifact_id, output)] = outputs.as_slice() else {
                return Err(AgentFailure::NotFound);
            };
            if output.len() > 16_384 {
                return Err(AgentFailure::BudgetExceeded);
            }
            let evidence: ExpertCalendarProposal =
                serde_json::from_str(output).map_err(|_| AgentFailure::InvalidInput)?;
            evidence.validate()?;
            if evidence.task_id != reference.invocation_id
                || recorded.invocation_key.as_uuid() != evidence.invocation_id
                || evidence.person_id != reference.person_id
                || task_snapshot.agent_id != evidence.package.id
                || !session.data_classes.contains(&evidence.data_class)
            {
                return Err(AgentFailure::InvalidInput);
            }
            let trusted: Vec<_> = task_snapshot
                .artifacts
                .iter()
                .filter(|artifact| artifact.artifact_id == *artifact_id)
                .collect();
            let [trusted] = trusted.as_slice() else {
                return Err(AgentFailure::Conflict);
            };
            let trusted_parts: Vec<_> = trusted
                .parts
                .iter()
                .filter_map(|part| match part {
                    floe_agent_contract::ArtifactPart::Data { media_type, data }
                        if media_type == EXPERT_CALENDAR_PROPOSAL_MEDIA_TYPE =>
                    {
                        Some(data)
                    }
                    _ => None,
                })
                .collect();
            if trusted_parts.as_slice() != [output] {
                return Err(AgentFailure::Conflict);
            }
            let floe_agent_contract::DependencyCoverage::Dependent { dependencies } =
                &trusted.coverage
            else {
                return Err(AgentFailure::PolicyDenied);
            };
            let [contributor] = dependencies.as_slice() else {
                return Err(AgentFailure::PolicyDenied);
            };
            if contributor.observation_id() != evidence.evidence_id {
                return Err(AgentFailure::PolicyDenied);
            }
            let floe_agent_contract::DependencyCoverage::Dependent {
                dependencies: report_dependencies,
            } = &task_snapshot.coverage
            else {
                return Err(AgentFailure::PolicyDenied);
            };
            if !report_dependencies.contains(contributor) {
                return Err(AgentFailure::PolicyDenied);
            }
            let registry = AgentRegistry::restore(snapshot, self.vault_id)?;
            registry.validate_settled_invocation(
                evidence.instance_id,
                evidence.person_id,
                evidence.assignment_id,
                &evidence.package,
                evidence.state_revision,
                evidence.data_class,
            )?;
            if require_active {
                registry.validate_active_assignment(evidence.person_id, evidence.assignment_id)?;
                registry.validate_current_execution_selection(
                    evidence.person_id,
                    &recorded.admission,
                    &recorded.selection,
                    true,
                )?;
            }
            Ok(evidence)
        }
        .await;
        let evidence = self
            .finish_registry_transaction(transaction, result)
            .await?;
        self.check_access()?;
        let value = operation(evidence).await?;
        self.check_access()?;
        Ok(value)
    }
}
