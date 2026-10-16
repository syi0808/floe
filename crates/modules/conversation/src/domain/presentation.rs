//! Immutable presentation evidence on the same Person-facing prefix as input.
//! Published text means available in the transcript, never proof of being read
//! or of audio playback. The body remains in its canonical Manager Run.
use super::surface::{SurfaceAnchor, SurfaceInputReceipt, digest};
use crate::{RunRecord, RunState};
use floe_conversation_contract::MessageId;
use floe_kernel::{AgentFailure, CommandId, OwnerActor, PersonId, RunId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceResultCompleteness {
    Complete,
    Partial,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceResultReference {
    pub run_id: RunId,
    pub run_revision: u64,
    pub completeness: SurfaceResultCompleteness,
    pub output_digest: [u8; 32],
}
impl SurfaceResultReference {
    pub fn for_run(run: &RunRecord) -> Result<Self, AgentFailure> {
        run.validate(run.person_id)?;
        let completeness = match run.state {
            RunState::Completed => SurfaceResultCompleteness::Complete,
            RunState::Failed if run.output.is_some() => SurfaceResultCompleteness::Partial,
            _ => return Err(AgentFailure::Conflict),
        };
        let output = run.output.as_ref().ok_or(AgentFailure::NotFound)?;
        Ok(Self {
            run_id: run.run_id,
            run_revision: run.aggregate_revision,
            completeness,
            output_digest: digest(
                b"floe.surface.result-body\0",
                &(
                    run.run_id,
                    run.aggregate_revision,
                    completeness,
                    output,
                    &run.coverage,
                ),
            )?,
        })
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !self.run_id.is_valid() || self.run_revision == 0 || self.output_digest == [0; 32] {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
    pub fn command_id(&self) -> CommandId {
        CommandId::from_uuid(Uuid::new_v5(
            &self.run_id.as_uuid(),
            b"surface-result-publication",
        ))
        .expect("UUID v5")
    }
    pub fn message_id(&self) -> MessageId {
        MessageId::from_uuid(Uuid::new_v5(
            &self.run_id.as_uuid(),
            b"surface-result-message",
        ))
        .expect("UUID v5")
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceResultReceipt {
    pub person_id: PersonId,
    pub device_id: String,
    pub before: SurfaceAnchor,
    pub through: SurfaceAnchor,
    pub result: SurfaceResultReference,
    pub request_digest: [u8; 32],
}
impl SurfaceResultReceipt {
    /// The repository must resolve/verify the referenced Run before admission.
    pub fn admit(
        actor: &OwnerActor,
        before: SurfaceAnchor,
        result: SurfaceResultReference,
    ) -> Result<Self, AgentFailure> {
        actor.validate()?;
        before.validate()?;
        result.validate()?;
        let request_digest = digest(
            b"floe.surface.publish-result\0",
            &(actor.person_id, &actor.device_id, before, &result),
        )?;
        let through = before.advance(request_digest)?;
        Ok(Self {
            person_id: actor.person_id,
            device_id: actor.device_id.clone(),
            before,
            through,
            result,
            request_digest,
        })
    }
    pub fn validate(&self) -> Result<(), AgentFailure> {
        let actor = OwnerActor {
            person_id: self.person_id,
            device_id: self.device_id.clone(),
            runtime_epoch: 1,
        };
        if Self::admit(&actor, self.before, self.result.clone())? != *self {
            return Err(AgentFailure::StorageUnavailable);
        }
        Ok(())
    }
}

/// One ordered envelope for user input and result publication. Typed input
/// receipts remain useful input proofs; they are not a parallel timeline.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    content = "receipt",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum SurfaceEntryReceipt {
    Input(SurfaceInputReceipt),
    PublishedResult(SurfaceResultReceipt),
    RunOutcome(crate::SurfaceOutcomeReceipt),
    PendingWorkCancelled(crate::PendingWorkCancellationEntry),
}
impl SurfaceEntryReceipt {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        match self {
            Self::Input(value) => value.validate(),
            Self::PublishedResult(value) => value.validate(),
            Self::RunOutcome(value) => value.validate(),
            Self::PendingWorkCancelled(value) => value.validate(),
        }
    }
    pub fn before(&self) -> SurfaceAnchor {
        match self {
            Self::Input(v) => v.before,
            Self::PublishedResult(v) => v.before,
            Self::RunOutcome(v) => v.before,
            Self::PendingWorkCancelled(v) => v.before,
        }
    }
    pub fn through(&self) -> SurfaceAnchor {
        match self {
            Self::Input(v) => v.through,
            Self::PublishedResult(v) => v.through,
            Self::RunOutcome(v) => v.through,
            Self::PendingWorkCancelled(v) => v.through,
        }
    }
    pub fn person_id(&self) -> PersonId {
        match self {
            Self::Input(v) => v.person_id,
            Self::PublishedResult(v) => v.person_id,
            Self::RunOutcome(v) => v.person_id,
            Self::PendingWorkCancelled(v) => v.cancellation.person_id,
        }
    }
    pub fn device_id(&self) -> &str {
        match self {
            Self::Input(v) => &v.device_id,
            Self::PublishedResult(v) => &v.device_id,
            Self::RunOutcome(v) => &v.device_id,
            Self::PendingWorkCancelled(v) => &v.cancellation.device_id,
        }
    }
    pub fn command_id(&self) -> CommandId {
        match self {
            Self::Input(v) => v.command_id,
            Self::PublishedResult(v) => v.result.command_id(),
            Self::RunOutcome(v) => v.outcome.command_id(),
            Self::PendingWorkCancelled(v) => v.cancellation.command_id,
        }
    }
    pub fn message_id(&self) -> MessageId {
        match self {
            Self::Input(v) => v.input.message_id,
            Self::PublishedResult(v) => v.result.message_id(),
            Self::RunOutcome(v) => v.outcome.message_id(),
            Self::PendingWorkCancelled(v) => v.message_id(),
        }
    }
    pub fn input(&self) -> Option<&SurfaceInputReceipt> {
        match self {
            Self::Input(v) => Some(v),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn completed() -> RunRecord {
        RunRecord {
            expert_environment: floe_experts::RunExpertEnvironmentIdentity {
                revision: 1,
                digest: [7; 32],
            },
            run_id: RunId::new(),
            command_id: CommandId::new(),
            session_id: Uuid::new_v4(),
            person_id: PersonId::new(),
            device_id: "synthetic-device".into(),
            user_message_id: Uuid::new_v4(),
            initial_session_revision: 0,
            request_digest: [9; 32],
            state: RunState::Completed,
            pending_terminal: None,
            output: Some("Synthetic result".into()),
            coverage: floe_agent_contract::DependencyCoverage::Independent,
            issue: None,
            blocked: None,
            session_revision: 2,
            aggregate_revision: 2,
            journal_revision: 1,
            executor_generation: 1,
            continuation_of: None,
            continuation_executor_generation: None,
            continuation_level: 0,
            retry_of: None,
            resume_of: None,
            resume_lineage: 0,
        }
    }
    #[test]
    fn result_reference_binds_canonical_output_and_never_claims_playback() {
        let mut run = completed();
        let original = SurfaceResultReference::for_run(&run).unwrap();
        let actor = OwnerActor {
            person_id: run.person_id,
            device_id: run.device_id.clone(),
            runtime_epoch: 2,
        };
        let receipt = SurfaceResultReceipt::admit(
            &actor,
            SurfaceAnchor::empty(run.session_id).unwrap(),
            original.clone(),
        )
        .unwrap();
        receipt.validate().unwrap();
        run.output = Some("Different result".into());
        let changed = SurfaceResultReference::for_run(&run).unwrap();
        assert_ne!(original.output_digest, changed.output_digest);
        assert_eq!(
            original.command_id(),
            changed.command_id(),
            "changed body cannot get a new publication identity"
        );
        assert_ne!(
            original.command_id().as_uuid(),
            original.message_id().as_uuid()
        );
        let encoded =
            serde_json::to_string(&SurfaceEntryReceipt::PublishedResult(receipt)).unwrap();
        assert!(
            !encoded.contains("Synthetic result"),
            "canonical body must not be copied"
        );
        assert!(!encoded.contains("played"));
    }
    #[test]
    fn publication_receipt_detects_prefix_and_reference_changes() {
        let run = completed();
        let actor = OwnerActor {
            person_id: run.person_id,
            device_id: run.device_id.clone(),
            runtime_epoch: 2,
        };
        let receipt = SurfaceResultReceipt::admit(
            &actor,
            SurfaceAnchor::empty(run.session_id).unwrap(),
            SurfaceResultReference::for_run(&run).unwrap(),
        )
        .unwrap();
        let mut changed = receipt.clone();
        changed.result.output_digest[0] ^= 1;
        assert!(changed.validate().is_err());
        let mut changed = receipt.clone();
        changed.through.sequence += 1;
        assert!(changed.validate().is_err());
        let mut changed = receipt;
        changed.device_id = "another-device".into();
        assert!(changed.validate().is_err());
    }
    #[test]
    fn working_or_textless_failed_run_cannot_publish_result_body() {
        let mut run = completed();
        run.state = RunState::Working;
        run.output = None;
        run.coverage = floe_agent_contract::DependencyCoverage::Unknown;
        run.session_revision = 1;
        run.aggregate_revision = 1;
        run.validate(run.person_id).unwrap();
        assert_eq!(
            SurfaceResultReference::for_run(&run),
            Err(AgentFailure::Conflict)
        );
        run.state = RunState::Failed;
        run.session_revision = 2;
        run.aggregate_revision = 2;
        run.issue = Some(AgentFailure::Cancelled);
        run.validate(run.person_id).unwrap();
        assert!(SurfaceResultReference::for_run(&run).is_err());
    }
    #[test]
    fn partial_output_and_status_have_distinct_content_bound_evidence() {
        let mut run = completed();
        run.state = RunState::Failed;
        run.issue = Some(AgentFailure::BudgetExceeded);
        let body = SurfaceResultReference::for_run(&run).unwrap();
        assert_eq!(body.completeness, SurfaceResultCompleteness::Partial);
        let status = crate::SurfaceOutcomeReference::for_run(&run).unwrap();
        assert_ne!(status.message_id(), body.message_id());
        assert_ne!(status.command_id(), body.command_id());
        let normalized = status.model_status(&run).unwrap();
        assert_eq!(
            normalized.coverage,
            floe_agent_contract::DependencyCoverage::Independent
        );
        assert!(!normalized.text.contains("Synthetic result"));
        assert!(normalized.text.contains("partial_output_available\":true"));
        assert!(normalized.text.contains("failed"));
        let original = status.clone();
        run.output = Some("A changed partial output".into());
        assert_ne!(
            original,
            crate::SurfaceOutcomeReference::for_run(&run).unwrap()
        );
        assert!(original.model_status(&run).is_err());
    }
    #[test]
    fn textless_terminal_status_does_not_invent_result_or_delivery() {
        for (state, issue) in [
            (RunState::Failed, AgentFailure::StorageUnavailable),
            (RunState::Cancelled, AgentFailure::Cancelled),
            (RunState::TimedOut, AgentFailure::DeadlineExceeded),
            (RunState::Interrupted, AgentFailure::Interrupted),
        ] {
            let mut run = completed();
            run.state = state;
            run.issue = Some(issue);
            run.output = None;
            run.coverage = floe_agent_contract::DependencyCoverage::Unknown;
            let outcome = crate::SurfaceOutcomeReference::for_run(&run).unwrap();
            assert!(SurfaceResultReference::for_run(&run).is_err());
            let actor = OwnerActor {
                person_id: run.person_id,
                device_id: run.device_id.clone(),
                runtime_epoch: 2,
            };
            let receipt = crate::SurfaceOutcomeReceipt::admit(
                &actor,
                SurfaceAnchor::empty(run.session_id).unwrap(),
                outcome,
            )
            .unwrap();
            receipt.validate().unwrap();
            let encoded =
                serde_json::to_string(&SurfaceEntryReceipt::RunOutcome(receipt.clone())).unwrap();
            assert!(!encoded.contains("played"));
            assert!(!encoded.contains("Synthetic result"));
            let mut altered = receipt;
            altered.outcome.terminal_digest[0] ^= 1;
            assert!(altered.validate().is_err());
        }
    }
    #[test]
    fn blocked_status_keeps_review_targets_out_of_independent_context() {
        let mut run = completed();
        run.state = RunState::Blocked;
        run.output = None;
        run.coverage = floe_agent_contract::DependencyCoverage::Unknown;
        run.blocked = Some(crate::RunBlockRecord {
            review_group_id: Uuid::new_v4(),
            prior_exhaustion: None,
            interactions: vec![crate::BlockedInteractionLink {
                interaction_id: Uuid::new_v4(),
                origin: crate::RunBlockOrigin {
                    session_id: run.session_id,
                    person_id: run.person_id,
                    device_id: run.device_id.clone(),
                    run_id: run.run_id,
                    executor_generation: run.executor_generation,
                    origin: crate::InteractionOrigin::Projection {
                        run_id: run.run_id,
                        projection_operation_id: Uuid::new_v4(),
                        target_digest: [17; 32],
                    },
                },
                target: crate::ReviewedTarget::NavigationOnly(crate::NavigationOnlyTarget {
                    destination: crate::NavigationDestination::ConnectionSettings,
                    source_id: "synthetic-sensitive-source".into(),
                    connection_id: None,
                    consumer: "conversation".into(),
                    purpose: "synthetic-private-purpose".into(),
                }),
            }],
        });
        let reference = crate::SurfaceOutcomeReference::for_run(&run).unwrap();
        let message = reference.model_status(&run).unwrap();
        assert!(message.text.contains("blocked"));
        assert!(!message.text.contains("synthetic-sensitive-source"));
        assert!(!message.text.contains("synthetic-private-purpose"));
        assert!(SurfaceResultReference::for_run(&run).is_err());
        run.blocked.as_mut().unwrap().review_group_id = Uuid::new_v4();
        assert_ne!(
            reference,
            crate::SurfaceOutcomeReference::for_run(&run).unwrap()
        );
    }
    #[test]
    fn partial_body_at_existing_output_bound_is_not_wrapped_or_truncated() {
        let mut run = completed();
        run.state = RunState::Failed;
        run.issue = Some(AgentFailure::BudgetExceeded);
        run.output = Some("x".repeat(floe_agent_contract::MAX_OUTPUT_BYTES));
        let reference = SurfaceResultReference::for_run(&run).unwrap();
        assert_eq!(reference.completeness, SurfaceResultCompleteness::Partial);
        assert_eq!(
            run.output.as_ref().unwrap().len(),
            floe_agent_contract::MAX_OUTPUT_BYTES
        );
        let status = crate::SurfaceOutcomeReference::for_run(&run).unwrap();
        assert!(status.model_status(&run).unwrap().text.len() < 256);
    }
}
