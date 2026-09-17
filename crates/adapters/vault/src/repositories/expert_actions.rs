//! Durable pre-dispatch intent and settlement for expert-originated actions.

use std::future::Future;

use chrono::{DateTime, Utc};
use floe_actions::{
    ActionAuthorityMode, AgentActionAdmission, AgentActionEnvelope, CalendarAction,
    CalendarActionState, ExpertActionStore, ExpertProposalReference,
};
use floe_agent_contract::AgentFailure;
use floe_access::ContextDependency;
use floe_execution::Cancellation;
use floe_experts::ExpertResult;
use uuid::Uuid;

use crate::{EncryptedAgentVault, VaultKeyProvider};

impl<Keys: VaultKeyProvider> ExpertActionStore for EncryptedAgentVault<Keys> {
    async fn agent_action_policy(&self) -> Result<ActionAuthorityMode, AgentFailure> {
        EncryptedAgentVault::agent_action_policy(self).await
    }

    async fn expert_proposal_dependency(
        &self,
        reference: &ExpertProposalReference,
    ) -> Result<ContextDependency, AgentFailure> {
        EncryptedAgentVault::expert_proposal_dependency(self, reference).await
    }

    async fn store_agent_action_envelope(
        &self,
        envelope: AgentActionEnvelope,
    ) -> Result<AgentActionAdmission, AgentFailure> {
        EncryptedAgentVault::store_agent_action_envelope(self, envelope).await
    }

    async fn agent_action_admission(
        &self,
        execution_id: Uuid,
    ) -> Result<AgentActionAdmission, AgentFailure> {
        EncryptedAgentVault::agent_action_admission(self, execution_id).await
    }

    async fn agent_calendar_action(
        &self,
        invocation_id: Uuid,
    ) -> Result<CalendarAction, AgentFailure> {
        EncryptedAgentVault::agent_calendar_action(self, invocation_id).await
    }

    async fn admit_agent_action_dispatch_with_cancellation_and_fence(
        &self,
        execution_id: Uuid,
        expected_digest: &str,
        now: DateTime<Utc>,
        cancellation: Cancellation,
        fence: impl Fn() -> Result<(), AgentFailure> + Send + Sync,
    ) -> Result<AgentActionAdmission, AgentFailure> {
        EncryptedAgentVault::admit_agent_action_dispatch_with_cancellation_and_fence(
            self,
            execution_id,
            expected_digest,
            now,
            cancellation,
            fence,
        )
        .await
    }

    async fn settle_agent_action(
        &self,
        admission: &AgentActionAdmission,
        state: CalendarActionState,
    ) -> Result<CalendarAction, AgentFailure> {
        EncryptedAgentVault::settle_agent_action(self, admission, state).await
    }

    async fn decide_agent_action(
        &self,
        execution_id: Uuid,
        expected_digest: &str,
        approve: bool,
        now: DateTime<Utc>,
    ) -> Result<AgentActionAdmission, AgentFailure> {
        EncryptedAgentVault::decide_agent_action(self, execution_id, expected_digest, approve, now)
            .await
    }

    async fn cancel_agent_action(
        &self,
        execution_id: Uuid,
        expected_digest: &str,
    ) -> Result<AgentActionAdmission, AgentFailure> {
        EncryptedAgentVault::cancel_agent_action(self, execution_id, expected_digest).await
    }

    async fn with_expert_proposal<ResultValue, Publish>(
        &self,
        reference: &ExpertProposalReference,
        publish: impl FnOnce(ExpertResult) -> Publish,
    ) -> Result<ResultValue, AgentFailure>
    where
        Publish: Future<Output = Result<ResultValue, AgentFailure>>,
    {
        EncryptedAgentVault::with_expert_proposal(self, reference, publish).await
    }

    async fn with_recorded_expert_proposal<ResultValue, Inspect>(
        &self,
        reference: &ExpertProposalReference,
        inspect: impl FnOnce(ExpertResult) -> Inspect,
    ) -> Result<ResultValue, AgentFailure>
    where
        Inspect: Future<Output = Result<ResultValue, AgentFailure>>,
    {
        EncryptedAgentVault::with_recorded_expert_proposal(self, reference, inspect).await
    }
}
