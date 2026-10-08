//! Non-secret planning values shared by all model consumers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AgentFailure, ModelBudgetProfile};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingBoundary {
    Device,
    Gateway,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCapability {
    Chat,
    StructuredOutput,
    ToolProposals,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ModelCapabilities(pub Vec<ModelCapability>);

impl ModelCapabilities {
    pub fn chat() -> Self {
        Self(vec![ModelCapability::Chat])
    }

    pub fn for_request(
        format: &crate::ModelOutputFormat,
        catalog: &crate::AllowedCatalog,
    ) -> Result<Self, AgentFailure> {
        format.validate().map_err(|_| AgentFailure::InvalidInput)?;
        let has_tools = !catalog.tools.is_empty() || !catalog.cards.is_empty();
        if format.is_json() && has_tools {
            return Err(AgentFailure::InvalidInput);
        }
        let mut values = vec![ModelCapability::Chat];
        if format.is_json() {
            values.push(ModelCapability::StructuredOutput);
        }
        if has_tools {
            values.push(ModelCapability::ToolProposals);
        }
        Ok(Self(values))
    }

    pub fn contains(&self, capability: ModelCapability) -> bool {
        self.0.contains(&capability)
    }

    pub fn includes(&self, required: &Self) -> bool {
        required
            .0
            .iter()
            .all(|capability| self.contains(*capability))
    }

    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.0.is_empty()
            || self.0.len() > 3
            || !self.0.contains(&ModelCapability::Chat)
            || self.0.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

/// Correlation only. Possession of a digest never authorizes a dispatch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ModelBindingDigest(pub [u8; 32]);

/// Opaque provider-owned commitment to the selected target and its revision.
/// It is correlation evidence only; it cannot authorize a model dispatch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ModelSelectionCommitment(pub [u8; 32]);

impl ModelSelectionCommitment {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        (self.0 != [0; 32])
            .then_some(())
            .ok_or(AgentFailure::InvalidInput)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelPlanRequest {
    pub principal: String,
    pub device_id: String,
    pub purpose: String,
    pub consumer: String,
    pub required_capabilities: ModelCapabilities,
}

impl ModelPlanRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        let person = Uuid::parse_str(&self.principal).map_err(|_| AgentFailure::InvalidInput)?;
        if person.is_nil()
            || person.to_string() != self.principal
            || !valid_identifier(&self.device_id, 256)
            || !matches!(
                self.purpose.as_str(),
                "quick_response" | "everyday_assistance" | "deep_work"
            )
            || !valid_identifier(&self.consumer, 128)
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.required_capabilities.validate()
    }
}

/// Values that must stay fixed for one logical Agent execution.
/// Provider target identity remains opaque.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelExecutionSelection {
    pub commitment: ModelSelectionCommitment,
    pub principal: String,
    pub device_id: String,
    pub purpose: String,
    pub consumer: String,
    pub capabilities: ModelCapabilities,
    pub boundary: ProcessingBoundary,
    pub binding_digest: ModelBindingDigest,
    pub budget_profile: ModelBudgetProfile,
}

impl ModelExecutionSelection {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.commitment.validate()?;
        ModelPlanRequest {
            principal: self.principal.clone(),
            device_id: self.device_id.clone(),
            purpose: self.purpose.clone(),
            consumer: self.consumer.clone(),
            required_capabilities: self.capabilities.clone(),
        }
        .validate()?;
        self.budget_profile.validate()
    }
}

/// Selection state restored for one logical Agent execution.
/// `Unproven` represents historical evidence that lacks a complete pin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelSelectionState {
    Fresh,
    Pinned(ModelExecutionSelection),
    Unproven,
}

impl ModelSelectionState {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if let Self::Pinned(selection) = self {
            selection.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedModelPlan {
    pub operation_id: Uuid,
    pub principal: String,
    pub device_id: String,
    pub purpose: String,
    pub consumer: String,
    pub capabilities: ModelCapabilities,
    pub boundary: ProcessingBoundary,
    pub binding_digest: ModelBindingDigest,
    /// Opaque target/revision commitment. Missing only in historical evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_commitment: Option<ModelSelectionCommitment>,
    /// Newly admitted plans always pin a profile. `None` is retained only for
    /// previously persisted evidence; it is never sufficient for dispatch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_profile: Option<ModelBudgetProfile>,
}

impl PreparedModelPlan {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.operation_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        if let Some(profile) = &self.budget_profile {
            profile.validate()?;
        }
        if let Some(commitment) = &self.selection_commitment {
            commitment.validate()?;
        }
        ModelPlanRequest {
            principal: self.principal.clone(),
            device_id: self.device_id.clone(),
            purpose: self.purpose.clone(),
            consumer: self.consumer.clone(),
            required_capabilities: self.capabilities.clone(),
        }
        .validate()
    }

    pub fn model_execution_selection(
        &self,
    ) -> Result<Option<ModelExecutionSelection>, AgentFailure> {
        let (Some(commitment), Some(budget_profile)) =
            (self.selection_commitment, self.budget_profile.as_ref())
        else {
            return Ok(None);
        };
        let selection = ModelExecutionSelection {
            commitment,
            principal: self.principal.clone(),
            device_id: self.device_id.clone(),
            purpose: self.purpose.clone(),
            consumer: self.consumer.clone(),
            capabilities: self.capabilities.clone(),
            boundary: self.boundary,
            binding_digest: self.binding_digest,
            budget_profile: budget_profile.clone(),
        };
        selection.validate()?;
        Ok(Some(selection))
    }

    /// Validate a fresh plan before projection or provider dispatch.
    pub fn validate_for_dispatch(&self) -> Result<(), AgentFailure> {
        self.validate()?;
        self.model_execution_selection()?
            .ok_or(AgentFailure::PolicyDenied)?
            .validate()
    }
}

/// Validate a newly admitted ModelIntent against the state produced by the
/// already durable prefix. Historical journal projection remains permissive;
/// owners call this only for the new append before committing it.
pub fn validate_model_intent_selection(
    prior: &ModelSelectionState,
    plan: &PreparedModelPlan,
) -> Result<ModelExecutionSelection, AgentFailure> {
    plan.validate_for_dispatch()?;
    let selection = plan
        .model_execution_selection()?
        .ok_or(AgentFailure::PolicyDenied)?;
    match prior {
        ModelSelectionState::Fresh => Ok(selection),
        ModelSelectionState::Pinned(pinned) if *pinned == selection => Ok(selection),
        ModelSelectionState::Pinned(_) | ModelSelectionState::Unproven => {
            Err(AgentFailure::PolicyDenied)
        }
    }
}

fn valid_identifier(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SourceProjectionReview, TaskBlockage, TaskExecutionKey, TaskExecutionReceipt,
        TaskExecutionReceiptRef, TaskModelAccounting, TaskSnapshot, TaskState,
    };
    use floe_context_contract::{
        DependencyCoverage, GrantConsumer, GrantOperation, GrantPurpose, SourceAccessBlockers,
        SourceAccessRequirement, SourceAccessRequirementKind,
    };
    use sha2::Digest;

    const LEGACY_PLAN: &str =
        include_str!("../../../../testdata/r3a/legacy-prepared-model-plan.json");

    fn legacy_plan() -> PreparedModelPlan {
        serde_json::from_str(LEGACY_PLAN)
            .expect("legacy plan without budget_profile remains readable")
    }

    #[test]
    fn legacy_plan_roundtrip_preserves_bytes_and_requires_profile_for_dispatch() {
        let plan = legacy_plan();
        assert!(plan.validate().is_ok(), "stored evidence remains valid");
        assert_eq!(serde_json::to_string(&plan).unwrap(), LEGACY_PLAN.trim());
        assert_eq!(plan.model_execution_selection().unwrap(), None);
        assert_eq!(
            plan.validate_for_dispatch(),
            Err(AgentFailure::PolicyDenied),
            "a missing profile cannot authorize a new dispatch"
        );
    }

    #[test]
    fn legacy_task_receipt_preserves_source_review_digest_without_profile_field() {
        let plan = legacy_plan();
        let projection_operation_id =
            Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();
        let blocker = SourceAccessRequirement::try_new(
            "floe.source.calendar",
            None,
            None,
            GrantOperation::Read,
            GrantConsumer::builtin("manager").unwrap(),
            GrantPurpose::Assistant,
            Vec::new(),
            None,
            SourceAccessRequirementKind::SelectResource,
            None,
            None,
            false,
        )
        .unwrap();
        let blockers = SourceAccessBlockers::try_new(vec![blocker]).unwrap();
        let digest: [u8; 32] = sha2::Sha256::digest(
            serde_json::to_vec(&(&plan, projection_operation_id, &blockers)).unwrap(),
        )
        .into();
        let task_id = crate::TaskId::from_uuid(
            Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap(),
        )
        .unwrap();
        let execution_id = Uuid::parse_str("66666666-6666-4666-8666-666666666666").unwrap();
        let receipt = TaskExecutionReceipt {
            reference: TaskExecutionReceiptRef {
                execution: TaskExecutionKey {
                    task_id,
                    execution_id,
                    executor_generation: 1,
                },
                task_revision: 2,
                journal_revision: 1,
                digest: [1; 32],
            },
            journal_digest: [2; 32],
            snapshot: TaskSnapshot {
                task_id,
                parent_run_id: None,
                principal: plan.principal.clone(),
                agent_id: "floe.expert.example".to_owned(),
                definition_revision: 1,
                state: TaskState::Blocked,
                result: None,
                artifacts: Vec::new(),
                coverage: DependencyCoverage::Independent,
                issue: None,
                blockage: Some(TaskBlockage::ModelProjection {
                    plan: plan.clone(),
                    review: SourceProjectionReview {
                        projection_operation_id,
                        target_digest: digest,
                        blockers,
                    },
                }),
            },
            accounting: TaskModelAccounting::default(),
        };
        let encoded = serde_json::to_vec(&receipt).unwrap();
        assert!(!String::from_utf8_lossy(&encoded).contains("budget_profile"));
        assert!(!String::from_utf8_lossy(&encoded).contains("selection_commitment"));
        let decoded: TaskExecutionReceipt = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, receipt);
        assert!(
            decoded
                .validate(crate::MAX_TASK_EXECUTION_RECEIPT_BYTES)
                .is_ok()
        );
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), encoded);
    }
}
