//! Canonical Conversation/Context model projection.
//!
//! Conversation owns the transcript filtering and the role prompt; Context owns
//! the envelope assembly. This adapter binds them: it reauthorizes the typed
//! history through Context decisions, prepares the role prompt, and hands the
//! filtered input to the Context assembler. It takes no model policy, profile,
//! placement, route, recipient, or credential state.

use floe_agent_contract::{
    AgentContext, AgentFailure, ModelProjectionOutcome, BoxFuture, DataClass,
    ModelProjectionPort, ModelProjectionRequest,
    prompts::{PromptAssembly, PromptComponentKind},
};
use floe_context::{DependencyResolver, EvidenceReader};
use uuid::Uuid;

use super::history_projection::project_model_conversation_history;
use crate::{FINALIZATION_ROLE_ID, FINALIZATION_ROLE_PROMPT, prompts::manager_prompt};

/// Canonical root projector: Conversation filtering plus Context assembly.
pub struct ConversationModelProjection<Evidence, Resolver> {
    evidence: Evidence,
    resolver: Resolver,
    session_id: Uuid,
    agent_context: AgentContext,
    session_data_classes: Vec<DataClass>,
    manager_prompt: PromptAssembly,
    finalization_prompt: PromptAssembly,
    expert_environment: floe_agent_contract::ExpertEnvironmentManifestEntry,
}

impl<Evidence, Resolver> ConversationModelProjection<Evidence, Resolver> {
    /// Bind the projector to one Session's evidence, authority, live context,
    /// admitted data classes, and exact Run environment. Composition injects the
    /// concrete values; no policy is decided here.
    pub fn new(
        evidence: Evidence,
        resolver: Resolver,
        session_id: Uuid,
        agent_context: AgentContext,
        session_data_classes: Vec<DataClass>,
        environment: floe_experts::RunExpertEnvironmentIdentity,
    ) -> Result<Self, AgentFailure> {
        if session_id.is_nil()
            || session_data_classes.is_empty()
            || session_data_classes.len() > floe_agent_contract::MAX_INPUT_DATA_CLASSES
        {
            return Err(AgentFailure::InvalidInput);
        }
        agent_context.validate()?;
        environment.validate()?;
        let manager_prompt = manager_prompt(agent_context.persona.as_ref())?;
        let mut finalization_prompt = manager_prompt.clone();
        let component = finalization_prompt
            .components
            .iter_mut()
            .find(|component| component.kind == PromptComponentKind::Role)
            .ok_or(AgentFailure::InvalidInput)?;
        component.content = FINALIZATION_ROLE_PROMPT.into();
        manager_prompt.validate()?;
        finalization_prompt.validate()?;
        let expert_environment = floe_agent_contract::ExpertEnvironmentManifestEntry {
            revision: environment.revision,
            digest: environment.digest,
        };
        Ok(Self {
            evidence,
            resolver,
            session_id,
            agent_context,
            session_data_classes,
            manager_prompt,
            finalization_prompt,
            expert_environment,
        })
    }
}

impl<Evidence, Resolver: DependencyResolver> ConversationModelProjection<Evidence, Resolver> {
    pub fn coverage_resolver(&self) -> &dyn DependencyResolver {
        &self.resolver
    }
}

impl<Evidence, Resolver> ModelProjectionPort for ConversationModelProjection<Evidence, Resolver>
where
    Evidence: EvidenceReader,
    Resolver: DependencyResolver,
{
    fn project<'a>(
        &'a self,
        request: ModelProjectionRequest,
        scope: &'a floe_execution::ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelProjectionOutcome, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            let role = match request.role.role_id.as_str() {
                "manager" => floe_context::ContextProjectionRole::Manager,
                role if role == FINALIZATION_ROLE_ID => {
                    floe_context::ContextProjectionRole::Finalization
                }
                _ => return Err(AgentFailure::InvalidInput),
            };
            if request.catalog.revision != self.expert_environment.revision {
                return Err(AgentFailure::InvalidInput);
            }
            let prompt = match role {
                floe_context::ContextProjectionRole::Manager => self.manager_prompt.clone(),
                _ => self.finalization_prompt.clone(),
            };
            let authorization = floe_context::DependencyAuthorization {
                deadline: scope.deadline(),
                cancellation: scope.cancellation().clone(),
            };
            let projected = project_model_conversation_history(
                &self.evidence,
                self.session_id,
                &request.conversation,
                Some(&self.resolver),
                &authorization,
            )
            .await?;
            floe_context::assemble_context_projection(floe_context::ContextProjectionInput {
                role,
                plan: &request.plan,
                projection_operation_id: request.projection_operation_id,
                purpose: crate::CONVERSATION_PURPOSE,
                response_contract: &request.role.output_contract,
                correction: request.correction.clone(),
                prompt,
                conversation: projected.conversation,
                agent_context: &self.agent_context,
                catalog: &request.catalog,
                expert_environment: Some(self.expert_environment.clone()),
                authorized_history_dependencies: &projected.authorized_history_dependencies,
                input_data_classes: self.session_data_classes.clone(),
                max_output_bytes: request.max_output_bytes,
            })
        })
    }
}
