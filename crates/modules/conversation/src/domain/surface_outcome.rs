//! Immutable non-completed Run outcome evidence on the Person-facing Surface.
//! A status describes execution custody, never a permission or audio receipt.
use super::surface::{SurfaceAnchor, advance_surface, digest};
use crate::{RunRecord, RunState};
use floe_conversation_contract::MessageId;
use floe_kernel::{AgentFailure, CommandId, OwnerActor, PersonId, RunId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceOutcomeReference {
    pub run_id: RunId,
    pub run_revision: u64,
    pub state: RunState,
    pub terminal_digest: [u8; 32],
}
impl SurfaceOutcomeReference {
    pub fn for_run(run: &RunRecord) -> Result<Self, AgentFailure> {
        run.validate(run.person_id)?;
        if !run.state.is_terminal() || run.state == RunState::Completed {
            return Err(AgentFailure::Conflict);
        }
        Ok(Self {
            run_id: run.run_id,
            run_revision: run.aggregate_revision,
            state: run.state,
            terminal_digest: digest(
                b"floe.surface.run-outcome\0",
                &(
                    run.run_id,
                    run.aggregate_revision,
                    run.state,
                    &run.output,
                    &run.coverage,
                    run.issue,
                    &run.blocked,
                ),
            )?,
        })
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.run_id.is_valid()
            || self.run_revision == 0
            || !self.state.is_terminal()
            || self.state == RunState::Completed
            || self.terminal_digest == [0; 32]
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
    pub fn command_id(&self) -> CommandId {
        CommandId::from_uuid(Uuid::new_v5(
            &self.run_id.as_uuid(),
            b"surface-outcome-publication",
        ))
        .expect("UUID v5")
    }
    pub fn message_id(&self) -> MessageId {
        MessageId::from_uuid(Uuid::new_v5(
            &self.run_id.as_uuid(),
            b"surface-outcome-message",
        ))
        .expect("UUID v5")
    }
    /// Only owner-known execution status; no source/review payload or generated
    /// answer is relabeled as independent evidence.
    pub fn model_status(
        &self,
        run: &RunRecord,
    ) -> Result<floe_agent_contract::AgentMessage, AgentFailure> {
        if Self::for_run(run)? != *self {
            return Err(AgentFailure::Conflict);
        }
        let message = floe_agent_contract::AgentMessage {
            message_id: self.message_id().as_uuid(),
            role: floe_agent_contract::MessageRole::Assistant,
            text: serde_json::json!({
                "manager_execution_outcome": self.state,
                "run_id": self.run_id.as_uuid(),
                "partial_output_available": run.output.is_some(),
            })
            .to_string(),
            call_id: None,
            coverage: floe_agent_contract::DependencyCoverage::Independent,
        };
        message.validate()?;
        Ok(message)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceOutcomeReceipt {
    pub person_id: PersonId,
    pub device_id: String,
    pub before: SurfaceAnchor,
    pub through: SurfaceAnchor,
    pub outcome: SurfaceOutcomeReference,
    pub request_digest: [u8; 32],
}
impl SurfaceOutcomeReceipt {
    pub fn admit(
        actor: &OwnerActor,
        before: SurfaceAnchor,
        outcome: SurfaceOutcomeReference,
    ) -> Result<Self, AgentFailure> {
        actor.validate()?;
        before.validate()?;
        outcome.validate()?;
        let request_digest = digest(
            b"floe.surface.publish-outcome\0",
            &(actor.person_id, &actor.device_id, before, &outcome),
        )?;
        Ok(Self {
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            before,
            through: advance_surface(before, request_digest)?,
            outcome,
            request_digest,
        })
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        let actor = OwnerActor {
            person_id: self.person_id,
            device_id: self.device_id.clone(),
            runtime_epoch: 1,
        };
        if Self::admit(&actor, self.before, self.outcome.clone())? != *self {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}
