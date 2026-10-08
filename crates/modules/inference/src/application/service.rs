use std::sync::Arc;

use floe_access::{
    DependencyResolver, GatewayAdmission, ModelDispatchRequest, ModelDispatchTarget,
    admit_model_dispatch, consume_model_dispatch, revalidate_model_dispatch,
};
use floe_agent_contract::{
    AgentFailure, AllowedCatalog, BoxFuture, ModelBudgetProfile, ModelCapabilities,
    ModelPlanRequest, ModelPort, ModelRequest, ModelResponse, ModelStep, ModelUsage,
    PreparedModelCall, PreparedModelPlan, ProcessingBoundary,
};
use floe_execution::ExecutionScope;
use uuid::Uuid;

use crate::{
    AdmittedDispatchTarget, CanonicalModelRequest, LocalObservation, ModelProvider,
    ObservedModelCapability, PreparedModelProfile, PreparedModelTransport, PrimaryObservation,
};

const MAX_ATTEMPT_TOKENS: u64 = 4_096;
const MAX_ATTEMPT_COST_MICROS: u64 = 1_000_000;

/// Availability is a projection of the same selected plan used for execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InferenceAvailability {
    pub boundary: ProcessingBoundary,
    pub capabilities: ModelCapabilities,
    pub budget_profile: ModelBudgetProfile,
}

impl InferenceAvailability {
    pub async fn observe(
        model: &dyn ModelPort,
        request: ModelPlanRequest,
        scope: &ExecutionScope,
    ) -> Result<Self, AgentFailure> {
        let prepared = model.prepare(request, scope).await?;
        Ok(Self {
            boundary: prepared.plan().boundary,
            capabilities: prepared.plan().capabilities.clone(),
            budget_profile: prepared
                .plan()
                .budget_profile
                .clone()
                .ok_or(AgentFailure::PolicyDenied)?,
        })
    }
}

/// The sole role-neutral planner. Source projection follows its immutable plan.
pub struct InferenceService<Provider, Resolver, Authority> {
    provider: Provider,
    resolver: Arc<Resolver>,
    authority: Arc<Authority>,
}

impl<Provider, Resolver, Authority> InferenceService<Provider, Resolver, Authority> {
    pub fn new(provider: Provider, resolver: Resolver, authority: Authority) -> Self {
        Self {
            provider,
            resolver: Arc::new(resolver),
            authority: Arc::new(authority),
        }
    }
}

impl<Provider, Resolver, Authority> ModelPort for InferenceService<Provider, Resolver, Authority>
where
    Provider: ModelProvider,
    Resolver: DependencyResolver + 'static,
    Authority: GatewayAdmission + 'static,
{
    fn prepare<'a>(
        &'a self,
        request: ModelPlanRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Box<dyn PreparedModelCall>, AgentFailure>> {
        Box::pin(async move {
            request.validate()?;
            scope
                .run(async {
                    // Only a positively established Primary absence permits local observation.
                    let (selected, expected_boundary) = match self
                        .provider
                        .observe_primary(&request, scope)
                        .await
                        .map_err(AgentFailure::from)?
                    {
                        PrimaryObservation::Available(selected) => {
                            (selected, ProcessingBoundary::Gateway)
                        }
                        PrimaryObservation::Absent(_) => match self
                            .provider
                            .observe_local_fallback(&request, scope)
                            .await
                            .map_err(|error| match error {
                                crate::ModelObservationError::InvalidInventory => {
                                    AgentFailure::LocalModelInvalidOutput
                                }
                                crate::ModelObservationError::TransportUnavailable => {
                                    AgentFailure::LocalModelUnavailable
                                }
                                error => AgentFailure::from(error),
                            })? {
                            LocalObservation::Available(selected) => {
                                (selected, ProcessingBoundary::Device)
                            }
                            LocalObservation::Unavailable(_) => {
                                return Err(AgentFailure::ModelUnavailable);
                            }
                        },
                    };
                    let PreparedModelProfile {
                        capability,
                        transport,
                    } = selected;
                    validate_observed_capability(&capability, &request, expected_boundary)?;
                    let target = transport.dispatch_target();
                    validate_target(&target, &capability, &request)?;
                    let plan = PreparedModelPlan {
                        operation_id: Uuid::new_v4(),
                        principal: request.principal,
                        device_id: request.device_id,
                        purpose: request.purpose,
                        consumer: request.consumer,
                        capabilities: capability.capabilities,
                        boundary: capability.boundary,
                        binding_digest: capability.binding_digest,
                        selection_commitment: Some(capability.selection_commitment),
                        budget_profile: Some(capability.budget_profile.clone()),
                    };
                    let prepared: Box<dyn PreparedModelCall> = Box::new(PreparedInferenceCall {
                        plan,
                        transport,
                        target,
                        resolver: self.resolver.clone(),
                        authority: self.authority.clone(),
                        preparation_cancellation: scope.cancellation().clone(),
                        preparation_deadline: scope.deadline(),
                    });
                    Ok(prepared)
                })
                .await
        })
    }
}

struct PreparedInferenceCall<Transport, Resolver, Authority> {
    plan: PreparedModelPlan,
    transport: Transport,
    target: ModelDispatchTarget,
    preparation_cancellation: floe_execution::Cancellation,
    preparation_deadline: tokio::time::Instant,
    resolver: Arc<Resolver>,
    authority: Arc<Authority>,
}

impl<Transport, Resolver, Authority> PreparedModelCall
    for PreparedInferenceCall<Transport, Resolver, Authority>
where
    Transport: PreparedModelTransport,
    Resolver: DependencyResolver,
    Authority: GatewayAdmission,
{
    fn plan(&self) -> &PreparedModelPlan {
        &self.plan
    }

    fn generate<'a>(
        &'a self,
        request: ModelRequest,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<ModelResponse, AgentFailure>> {
        Box::pin(async move {
            let preparation_lifetime = self.preparation_cancellation.child_scope();
            floe_execution::tasks::run_bounded(
                scope.run(self.dispatch_selected(request, scope)),
                self.preparation_deadline,
                &preparation_lifetime,
            )
            .await
        })
    }
}

impl<Transport, Resolver, Authority> PreparedInferenceCall<Transport, Resolver, Authority>
where
    Transport: PreparedModelTransport,
    Resolver: DependencyResolver,
    Authority: GatewayAdmission,
{
    async fn dispatch_selected(
        &self,
        request: ModelRequest,
        scope: &ExecutionScope,
    ) -> Result<ModelResponse, AgentFailure> {
        request.validate()?;
        if !self
            .plan
            .capabilities
            .includes(&ModelCapabilities::for_request(
                &request.projection.envelope.run_instructions.output_format,
                &request.catalog,
            )?)
        {
            return Err(AgentFailure::PolicyDenied);
        }
        if request.principal != self.plan.principal
            || request.device_id != self.plan.device_id
            || request.purpose != self.plan.purpose
            || request.consumer != self.plan.consumer
            || request.projection.plan_id != self.plan.operation_id
            || request.projection.binding_digest != self.plan.binding_digest
            || request.projection.envelope.run_instructions.purpose != self.plan.purpose
        {
            return Err(AgentFailure::PolicyDenied);
        }
        let person =
            Uuid::parse_str(&self.plan.principal).map_err(|_| AgentFailure::InvalidInput)?;
        let dispatch = ModelDispatchRequest {
            person_id: floe_access::PersonId::from_uuid(person)
                .ok_or(AgentFailure::InvalidInput)?,
            device_id: self.plan.device_id.clone(),
            plan_id: self.plan.operation_id,
            binding_digest: self.plan.binding_digest.0,
            projection_ref: request.projection.projection_ref.as_uuid(),
            projection_revision: request.projection.projection_revision,
            coverage: request.projection.coverage.clone(),
            input_data_classes: request.projection.input_data_classes.clone(),
            purpose: self.plan.purpose.clone(),
            consumer: self.plan.consumer.clone(),
            target: self.target.clone(),
            deadline: scope.deadline(),
            cancellation: scope.cancellation().clone(),
        };
        let permit = admit_model_dispatch(
            dispatch,
            self.resolver.as_ref(),
            self.authority.as_ref(),
            scope,
        )
        .await?;
        let mut tokens = request
            .reservation_ceiling
            .tokens
            .min(scope.budget().max_tokens())
            .min(MAX_ATTEMPT_TOKENS);
        let mut cost = request
            .reservation_ceiling
            .cost_micros
            .min(scope.budget().max_cost_micros())
            .min(MAX_ATTEMPT_COST_MICROS);
        let mut attempt =
            scope
                .budget()
                .begin_model_attempt(request.attempt_id, &mut tokens, &mut cost)?;
        let canonical = CanonicalModelRequest {
            attempt_id: request.attempt_id,
            envelope: request.projection.envelope.clone(),
            catalog: request.catalog.clone(),
            budget_profile: self
                .plan
                .budget_profile
                .clone()
                .ok_or(AgentFailure::PolicyDenied)?,
            input_data_classes: request.projection.input_data_classes.clone(),
            remaining_tokens: tokens,
            remaining_cost_micros: cost,
            max_output_bytes: request
                .projection
                .envelope
                .attempt
                .max_output_bytes
                .min(floe_agent_contract::MAX_OUTPUT_BYTES),
            deadline: scope.deadline(),
            cancellation: scope.cancellation().clone(),
        };
        canonical.validate()?;
        self.transport.validate_request(&canonical)?;
        let max_output_bytes = canonical.max_output_bytes;
        let fence = consume_model_dispatch(permit).await?;
        let target = AdmittedDispatchTarget::from_consumed(&fence);
        if !target.matches(&self.plan.binding_digest, self.plan.boundary) {
            return Err(AgentFailure::PolicyDenied);
        }
        attempt.mark_dispatched();
        let response = self.transport.generate(canonical, target).await?;
        // Settle trusted accounting before output policy or release checks.
        let receipt = attempt.settle_observed(response.usage.tokens, response.usage.cost_micros)?;
        revalidate_model_dispatch(&fence).await?;
        let steps = response.output?;
        validate_output(
            &steps,
            &request.catalog,
            &request.projection.envelope.run_instructions.output_format,
            max_output_bytes,
        )?;
        Ok(ModelResponse {
            attempt_id: request.attempt_id,
            steps,
            usage: ModelUsage {
                tokens: receipt.charged_tokens,
                cost_micros: receipt.charged_cost_micros,
            },
            accounting: receipt.accounting,
        })
    }
}

fn validate_observed_capability(
    capability: &ObservedModelCapability,
    request: &ModelPlanRequest,
    expected_boundary: ProcessingBoundary,
) -> Result<(), AgentFailure> {
    capability
        .capabilities
        .validate()
        .map_err(|_| AgentFailure::ServerModelInvalidOutput)?;
    if capability.purpose.as_str() != request.purpose
        || capability.consumer.as_str() != request.consumer
        || capability.boundary != expected_boundary
        || !capability
            .capabilities
            .includes(&request.required_capabilities)
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

fn validate_target(
    target: &ModelDispatchTarget,
    capability: &ObservedModelCapability,
    request: &ModelPlanRequest,
) -> Result<(), AgentFailure> {
    match (target, capability.boundary) {
        (ModelDispatchTarget::Device, ProcessingBoundary::Device) => Ok(()),
        (ModelDispatchTarget::Gateway { expected }, ProcessingBoundary::Gateway) => {
            expected.validate()?;
            if expected.person_id != request.principal
                || expected.device_id != request.device_id
                || expected.binding_digest() != capability.binding_digest.0
            {
                return Err(AgentFailure::PolicyDenied);
            }
            Ok(())
        }
        _ => Err(AgentFailure::PolicyDenied),
    }
}

fn validate_output(
    output: &[ModelStep],
    catalog: &AllowedCatalog,
    output_format: &floe_agent_contract::ModelOutputFormat,
    max_output_bytes: usize,
) -> Result<(), AgentFailure> {
    if let floe_agent_contract::ModelOutputFormat::Json { schema } = output_format {
        let [ModelStep::Answer { text, artifacts }] = output else {
            return Err(AgentFailure::InvalidModelOutput);
        };
        if !artifacts.is_empty() || !catalog.tools.is_empty() || !catalog.cards.is_empty() {
            return Err(AgentFailure::InvalidModelOutput);
        }
        let value = floe_agent_contract::strict_model_json(text.as_bytes(), max_output_bytes)
            .map_err(|_| AgentFailure::InvalidModelOutput)?;
        schema
            .validate_value(&value)
            .map_err(|_| AgentFailure::InvalidModelOutput)?;
    }
    if output.is_empty()
        || output.len() > 16
        || serde_json::to_vec(output)
            .map_err(|_| AgentFailure::ServerModelInvalidOutput)?
            .len()
            > max_output_bytes
    {
        return Err(AgentFailure::ServerModelInvalidOutput);
    }
    for step in output {
        let valid = match step {
            ModelStep::CallTool {
                tool_id,
                definition_revision,
                input,
            } => {
                catalog.tools.iter().any(|descriptor| {
                    descriptor.id == *tool_id
                        && descriptor.definition_revision == *definition_revision
                }) && floe_agent_contract::validate_tool_input(input).is_ok()
            }
            ModelStep::Delegate {
                agent_id,
                definition_revision,
                message,
                context_refs,
            } => {
                catalog.cards.iter().any(|definition| {
                    definition.card.id == *agent_id
                        && definition.definition_revision == *definition_revision
                }) && !message.trim().is_empty()
                    && floe_agent_contract::valid_context_refs(context_refs)
            }
            ModelStep::Preamble { text } | ModelStep::Answer { text, .. } => {
                !text.trim().is_empty()
            }
        };
        if !valid {
            return Err(AgentFailure::ServerModelInvalidOutput);
        }
    }
    Ok(())
}
