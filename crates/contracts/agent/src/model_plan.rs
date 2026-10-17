//! Non-secret planning values shared by all model consumers.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AgentFailure, ModelBindingDigest, ModelBudgetProfile, ModelSelectionCommitment,
    ProcessingBoundary,
};

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
/// `Unproven` marks a batch-only continuation whose current projection lacks
/// the original selection lineage. It never authorizes new model dispatch.
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
    /// Opaque target/revision commitment pinned by the selected provider.
    pub selection_commitment: ModelSelectionCommitment,
    /// Every admitted or stored plan pins a complete budget profile. Unknown
    /// provider limits are represented inside the profile, not by absent proof.
    pub budget_profile: ModelBudgetProfile,
}

impl PreparedModelPlan {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.operation_id.is_nil() {
            return Err(AgentFailure::InvalidInput);
        }
        self.budget_profile.validate()?;
        self.selection_commitment.validate()?;
        ModelPlanRequest {
            principal: self.principal.clone(),
            device_id: self.device_id.clone(),
            purpose: self.purpose.clone(),
            consumer: self.consumer.clone(),
            required_capabilities: self.capabilities.clone(),
        }
        .validate()
    }

    pub fn model_execution_selection(&self) -> Result<ModelExecutionSelection, AgentFailure> {
        self.validate()?;
        let selection = ModelExecutionSelection {
            commitment: self.selection_commitment,
            principal: self.principal.clone(),
            device_id: self.device_id.clone(),
            purpose: self.purpose.clone(),
            consumer: self.consumer.clone(),
            capabilities: self.capabilities.clone(),
            boundary: self.boundary,
            binding_digest: self.binding_digest,
            budget_profile: self.budget_profile.clone(),
        };
        Ok(selection)
    }
}

/// Validate a newly admitted ModelIntent against the state produced by the
/// already durable prefix. Every decoded plan has the same required proof;
/// owners additionally enforce execution-stable selection before append.
pub fn validate_model_intent_selection(
    prior: &ModelSelectionState,
    plan: &PreparedModelPlan,
) -> Result<ModelExecutionSelection, AgentFailure> {
    let selection = plan.model_execution_selection()?;
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

    fn complete_plan() -> PreparedModelPlan {
        let mut value: serde_json::Value = serde_json::from_str(LEGACY_PLAN).unwrap();
        value["selection_commitment"] = serde_json::to_value([7u8; 32]).unwrap();
        value["budget_profile"] = serde_json::to_value(ModelBudgetProfile::unknown()).unwrap();
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn prepared_plan_requires_complete_selection_and_budget_at_decode() {
        let mut value: serde_json::Value = serde_json::from_str(LEGACY_PLAN).unwrap();
        value["selection_commitment"] = serde_json::to_value([7u8; 32]).unwrap();
        value["budget_profile"] = serde_json::to_value(ModelBudgetProfile::unknown()).unwrap();
        let complete: PreparedModelPlan = serde_json::from_value(value.clone()).unwrap();
        complete.validate().unwrap();
        let mut invalid = complete.clone();
        invalid.selection_commitment = ModelSelectionCommitment([0; 32]);
        assert!(invalid.model_execution_selection().is_err());
        let mut invalid = complete.clone();
        invalid.budget_profile.framing.max_messages = 0;
        assert!(invalid.model_execution_selection().is_err());
        let mut invalid = complete.clone();
        invalid.operation_id = Uuid::nil();
        assert!(invalid.model_execution_selection().is_err());
        for field in ["selection_commitment", "budget_profile"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<PreparedModelPlan>(missing).is_err(),
                "missing {field} must not become a readable historical plan"
            );
            let mut null = value.clone();
            null[field] = serde_json::Value::Null;
            assert!(
                serde_json::from_value::<PreparedModelPlan>(null).is_err(),
                "null {field} must fail closed"
            );
        }
    }

    #[test]
    fn complete_plan_roundtrip_retains_every_required_proof() {
        let plan = complete_plan();
        plan.validate().unwrap();
        let encoded = serde_json::to_vec(&plan).unwrap();
        let decoded: PreparedModelPlan = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, plan);
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), encoded);
        assert!(serde_json::from_str::<PreparedModelPlan>(LEGACY_PLAN).is_err());
        assert_eq!(
            plan.model_execution_selection().unwrap().budget_profile,
            plan.budget_profile
        );
    }

    #[test]
    fn current_task_receipt_preserves_source_review_digest_with_required_profile() {
        let plan = complete_plan();
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
        assert!(String::from_utf8_lossy(&encoded).contains("budget_profile"));
        assert!(String::from_utf8_lossy(&encoded).contains("selection_commitment"));
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
