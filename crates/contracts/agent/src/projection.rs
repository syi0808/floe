//! Source-authorized model input bound to one immutable prepared plan.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AgentFailure, AllowedCatalog, ContextEnvelope, DataClass, DependencyCoverage, MAX_OUTPUT_BYTES,
    ModelConversation, RoleSpec,
};

/// Canonical host correction for one invalid structured model output.
///
/// The Engine attaches this to the next projection's Attempt context; it
/// is never forged into a user message.
pub const MODEL_CORRECTION_TEXT: &str = "The previous model response failed output validation and was not executed. Return a complete answer or registered tool calls with valid JSON object arguments matching their schemas. Optional explanatory text accompanying calls is a preamble, not a final answer. Do not mix a final answer with tool calls. Preserve all observed tool results; do not repeat completed work. This correction is protocol feedback, not a new user task; answer the original user request.";

pub const MAX_CORRECTION_BYTES: usize = 4096;
pub const MAX_INPUT_DATA_CLASSES: usize = 16;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ProjectionRef(pub Uuid);

impl ProjectionRef {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(value: Uuid) -> Option<Self> {
        (!value.is_nil()).then_some(Self(value))
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// Host-generated correction attached to a projection's Attempt context.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModelCorrection {
    pub text: String,
}

impl ModelCorrection {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if !crate::message::bounded(&self.text, MAX_CORRECTION_BYTES) {
            return Err(AgentFailure::InvalidInput);
        }
        Ok(())
    }
}

/// What the Engine asks the host to project before each model call.
#[derive(Clone, Debug)]
pub struct ModelProjectionRequest {
    pub principal: String,
    pub projection_operation_id: Uuid,
    pub plan: crate::PreparedModelPlan,
    pub role: RoleSpec,
    pub conversation: ModelConversation,
    pub catalog: AllowedCatalog,
    pub max_output_bytes: usize,
    pub correction: Option<ModelCorrection>,
}

impl ModelProjectionRequest {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.plan.validate()?;
        if self.projection_operation_id.is_nil()
            || self.principal != self.plan.principal
            || self.principal.trim().is_empty()
            || self.principal.len() > 256
            || self.principal.chars().any(char::is_control)
            || self.max_output_bytes == 0
            || self.max_output_bytes > MAX_OUTPUT_BYTES
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.role.validate()?;
        if !self.plan.capabilities.includes(&crate::ModelCapabilities::for_request(&self.role.output_format, &self.catalog)?) { return Err(AgentFailure::PolicyDenied); }
        self.conversation.validate()?;
        self.catalog
            .tools
            .iter()
            .try_for_each(crate::ToolDescriptor::validate)?;
        self.catalog
            .cards
            .iter()
            .try_for_each(crate::AgentDefinition::validate)?;
        if let Some(correction) = &self.correction {
            correction.validate()?;
        }
        Ok(())
    }
}

/// The envelope one attempt runs on, with the coverage and data classes of the
/// input that went into it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizedModelProjection {
    pub projection_ref: ProjectionRef,
    pub projection_operation_id: Uuid,
    pub plan_id: Uuid,
    pub binding_digest: crate::ModelBindingDigest,
    pub projection_revision: u64,
    pub envelope: ContextEnvelope,
    pub coverage: DependencyCoverage,
    pub input_data_classes: Vec<DataClass>,
}

impl AuthorizedModelProjection {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.projection_ref.as_uuid().is_nil()
            || self.projection_operation_id.is_nil()
            || self.plan_id.is_nil()
            || self.projection_revision == 0
            || self.input_data_classes.is_empty()
            || self.input_data_classes.len() > MAX_INPUT_DATA_CLASSES
        {
            return Err(AgentFailure::InvalidInput);
        }
        self.coverage
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)?;
        self.envelope.validate()
    }
}

#[derive(Clone, Debug)]
pub enum ModelProjectionOutcome {
    Ready(AuthorizedModelProjection),
    NeedsSourceReview(SourceProjectionReview),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceProjectionReview {
    pub projection_operation_id: Uuid,
    pub target_digest: [u8; 32],
    pub blockers: floe_context_contract::SourceAccessBlockers,
}

impl SourceProjectionReview {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.projection_operation_id.is_nil() || self.target_digest == [0; 32] {
            return Err(AgentFailure::InvalidInput);
        }
        self.blockers
            .validate()
            .map_err(|_| AgentFailure::InvalidInput)
    }
}
