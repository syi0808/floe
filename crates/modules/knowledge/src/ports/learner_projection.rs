use chrono::{DateTime, Utc};
use floe_agent_contract::{AgentFailure, BoxFuture, ExecutionScope, ModelCorrection,
    ModelProjectionOutcome, OwnerActor, PreparedModelPlan};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct LearnerClaimRef {
    pub job_id: Uuid,
    pub claim_attempt: u8,
}

impl LearnerClaimRef {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.job_id.is_nil() || !(1..=crate::MAX_LEARNER_JOB_ATTEMPTS).contains(&self.claim_attempt) {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }

    pub fn execution_id(&self) -> Uuid {
        Uuid::new_v5(&self.job_id, format!("floe.learner.claim:{}", self.claim_attempt).as_bytes())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LearnerProjectionBounds {
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
}

#[derive(Clone, Debug)]
pub struct LearnerProjectionRequest {
    pub actor: OwnerActor,
    pub claim: LearnerClaimRef,
    pub projection_operation_id: Uuid,
    pub plan: PreparedModelPlan,
    pub correction: Option<ModelCorrection>,
    pub evidence_refs: Vec<crate::LearningEvidenceRef>,
    pub bounds: LearnerProjectionBounds,
    pub expires_at: DateTime<Utc>,
}

impl LearnerProjectionRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.actor.validate()?;
        self.claim.validate()?;
        self.plan.validate()?;
        if self.projection_operation_id.is_nil()
            || self.plan.principal != self.actor.person_id.to_string()
            || self.plan.device_id != self.actor.device_id
            || self.plan.purpose != crate::LEARNER_INFERENCE_PURPOSE
            || self.plan.consumer != crate::LEARNER_INFERENCE_CONSUMER
            || self.evidence_refs.is_empty() || self.evidence_refs.len() > 32
            || self.bounds.max_input_bytes == 0 || self.bounds.max_input_bytes > 16 * 1024
            || self.bounds.max_output_bytes == 0 || self.bounds.max_output_bytes > 4 * 1024
        {
            return Err(AgentFailure::InvalidInput);
        }
        if let Some(correction) = &self.correction { correction.validate()?; }
        Ok(())
    }
}

pub trait LearnerProjectionPort: Send + Sync {
    fn project<'a>(&'a self, request: LearnerProjectionRequest, scope: &'a ExecutionScope)
        -> BoxFuture<'a, Result<ModelProjectionOutcome, AgentFailure>>;
}

/// Reads the immutable input of an actually running, exact job claim.
/// Implementations validate the stored claim, Person, evidence revision and current authority.
pub trait LearnerEvidenceRepository: Send + Sync {
    fn read_claim<'a>(&'a self, actor: &'a OwnerActor, claim: LearnerClaimRef,
        scope: &'a ExecutionScope) -> BoxFuture<'a, Result<crate::LearnerReviewInput, AgentFailure>>;
}
