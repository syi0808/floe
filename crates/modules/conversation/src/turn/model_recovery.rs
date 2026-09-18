use crate::{AgentMessage, ModelRequest, ModelResponse, ModelRunner, ModelStep};
use floe_agent_contract::AgentFailure;

/// One legacy model attempt with no correction retry.
///
/// Calls `ModelRunner::generate` exactly once, enforces deadline/cancellation,
/// and applies the legacy structural validation. It performs no second model
/// call, no usage-ledger accounting, and no attempt journaling: the caller owns
/// the budget attempt. The transitional General Conversation bridge
/// (`LegacyModelPort`) binds that attempt to the scope budget; removed when
/// InferenceService implements ModelPort.
pub async fn generate_once<Model: ModelRunner>(
    model: &Model,
    request: ModelRequest,
) -> Result<ModelResponse, AgentFailure> {
    let validators = validate_generate_once_request(&request)?;
    dispatch_generate_once(model, request, &validators).await
}

/// Legacy request preflight before any budget dispatch fence: validator
/// compilation, AgentCard validation, cancellation and deadline. A failure
/// here never reached the provider, so the caller must not mark the budget
/// attempt dispatched.
pub fn validate_generate_once_request(
    request: &ModelRequest,
) -> Result<Vec<Option<jsonschema::Validator>>, AgentFailure> {
    let validators = legacy_validators(request)?;
    for card in &request.active_agents {
        card.validate()?;
    }
    if request.cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if request.deadline <= tokio::time::Instant::now() {
        return Err(AgentFailure::DeadlineExceeded);
    }
    Ok(validators)
}

/// Provider handoff and structural validation for a preflighted request.
/// Call only after the budget attempt is marked dispatched: every failure
/// from here ran past the dispatch fence, including a provider error and a
/// response that fails structural validation after a successful call.
pub async fn dispatch_generate_once<Model: ModelRunner>(
    model: &Model,
    request: ModelRequest,
    validators: &[Option<jsonschema::Validator>],
) -> Result<ModelResponse, AgentFailure> {
    let result = tokio::select! {
        biased;
        _ = request.cancellation.cancelled() => Err(AgentFailure::Cancelled),
        _ = tokio::time::sleep_until(request.deadline) => Err(AgentFailure::DeadlineExceeded),
        result = model.generate(request.clone()) => result,
    };
    let response = result?;
    validate_legacy_response(&request, validators, &response)?;
    Ok(response)
}

pub async fn generate_with_recovery<Model: ModelRunner>(
    model: &Model,
    mut request: ModelRequest,
) -> Result<ModelResponse, AgentFailure> {
    let mut reserved_tokens = 0;
    let mut reserved_cost = 0;
    let validators = legacy_validators(&request)?;
    for card in &request.active_agents {
        card.validate()?;
    }
    for attempt in 0..2 {
        if request.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if request.deadline <= tokio::time::Instant::now() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let mut accounting = request.usage.begin(
            &mut request.remaining_tokens,
            &mut request.remaining_cost_micros,
        )?;
        let mut recorded_usage = floe_execution::budget::ModelUsage {
            attempts: 1,
            tokens: accounting.estimated_tokens(),
            estimated_tokens: accounting.estimated_tokens(),
            cost_micros: 0,
        };
        let lifecycle = floe_inference::AttemptLifecycle::start(
            &request.usage,
            request.turn_id,
            request.session_id,
            attempt + 1,
            model.placement(),
            recorded_usage,
        )
        .await?;
        let result = tokio::select! {
            biased;
            _ = request.cancellation.cancelled() => Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(request.deadline) => Err(AgentFailure::DeadlineExceeded),
            result = async {
                accounting.mark_dispatched();
                model.generate(request.clone()).await
            } => result,
        };
        let mut consumed_tokens = accounting.estimated_tokens();
        let mut consumed_cost = 0;
        let result = result.and_then(|response| {
            consumed_tokens = response.used_tokens;
            consumed_cost = response.cost_micros;
            recorded_usage.tokens = consumed_tokens;
            recorded_usage.cost_micros = consumed_cost;
            recorded_usage.estimated_tokens = 0;
            accounting.settle(consumed_tokens, consumed_cost)?;
            validate_legacy_response(&request, &validators, &response)?;
            Ok(response)
        });
        lifecycle
            .finish(result.as_ref().err().copied(), recorded_usage)
            .await?;
        match result {
            Ok(mut response) => {
                response.used_tokens = response
                    .used_tokens
                    .checked_add(reserved_tokens)
                    .ok_or(AgentFailure::BudgetExceeded)?;
                response.cost_micros = response
                    .cost_micros
                    .checked_add(reserved_cost)
                    .ok_or(AgentFailure::BudgetExceeded)?;
                return Ok(response);
            }
            Err(
                AgentFailure::InvalidModelOutput
                | AgentFailure::LocalModelInvalidOutput
                | AgentFailure::ServerModelInvalidOutput,
            ) if attempt == 0 => {
                reserved_tokens = consumed_tokens;
                reserved_cost = consumed_cost;
                request.remaining_cost_micros = request
                    .remaining_cost_micros
                    .checked_sub(reserved_cost)
                    .ok_or(AgentFailure::BudgetExceeded)?;
                request.remaining_tokens = request
                    .remaining_tokens
                    .checked_sub(reserved_tokens)
                    .filter(|remaining| *remaining > 0)
                    .ok_or(AgentFailure::BudgetExceeded)?;
                request.messages.push(AgentMessage::User {
                    turn_id: request.turn_id,
                    text: include_str!("../../prompts/model_correction.txt")
                        .trim()
                        .into(),
                });
            }
            Err(failure) => return Err(failure),
        }
    }
    Err(AgentFailure::InvalidModelOutput)
}

fn legacy_validators(
    request: &ModelRequest,
) -> Result<Vec<Option<jsonschema::Validator>>, AgentFailure> {
    request
        .capabilities
        .iter()
        .map(|capability| {
            capability
                .input_schema
                .as_ref()
                .map(jsonschema::validator_for)
                .transpose()
                .map_err(|_| AgentFailure::InvalidInput)
        })
        .collect()
}

/// Structural validation shared by the retrying legacy path and the
/// single-attempt transitional bridge: budget caps, batch shape, replay
/// linkage, and per-step checks. The caller's accounting (if any) settles
/// before this runs, so a response that fails here still consumed its attempt.
fn validate_legacy_response(
    request: &ModelRequest,
    validators: &[Option<jsonschema::Validator>],
    response: &ModelResponse,
) -> Result<(), AgentFailure> {
    if response.used_tokens > request.remaining_tokens
        || response.cost_micros > request.remaining_cost_micros
        || serde_json::to_vec(&response.output)
            .map_err(|_| AgentFailure::InvalidModelOutput)?
            .len()
            > request.max_output_bytes
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    if response.schema_version != crate::AGENT_VERSION
        || response.output.is_empty()
        || response.output.len() > 16
        || response.call_count() > 8
        || response.delegation_count() > 1
        || (response.delegation_count() > 0 && response.capability_call_count() > 0)
    {
        return Err(AgentFailure::InvalidModelOutput);
    }
    let answers = response
        .output
        .iter()
        .filter(|step| matches!(step, ModelStep::Answer { .. }))
        .count();
    if (response.call_count() > 0 && answers != 0)
        || (response.call_count() == 0
            && (answers != 1
                || !matches!(response.output.last(), Some(ModelStep::Answer { .. }))))
    {
        return Err(AgentFailure::InvalidModelOutput);
    }
    if let Some(replay) = &response.replay {
        let unique: std::collections::HashSet<_> = replay.call_ids.iter().collect();
        if replay.call_ids.len() != response.call_count()
            || replay.call_ids.is_empty()
            || replay.call_ids.first() != Some(&replay.provider_call_id)
            || unique.len() != replay.call_ids.len()
            || replay
                .call_ids
                .iter()
                .any(|id| id.is_empty() || id.len() > 128)
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
    }
    for step in &response.output {
        match step {
            ModelStep::Answer { text } | ModelStep::Preamble { text } => {
                if text.trim().is_empty() {
                    return Err(AgentFailure::InvalidModelOutput);
                }
            }
            ModelStep::Call {
                capability_id,
                input,
            } => {
                let index = request
                    .capabilities
                    .iter()
                    .position(|capability| capability.id == *capability_id)
                    .ok_or(AgentFailure::CapabilityDenied)?;
                if !request.capabilities[index].read_only {
                    return Err(AgentFailure::CapabilityDenied);
                }
                let value: serde_json::Value =
                    serde_json::from_str(input).map_err(|_| AgentFailure::InvalidModelOutput)?;
                if !value.is_object()
                    || validators
                        .get(index)
                        .and_then(Option::as_ref)
                        .is_some_and(|validator| !validator.is_valid(&value))
                {
                    return Err(AgentFailure::InvalidModelOutput);
                }
            }
            ModelStep::Delegate {
                agent_id,
                message,
                context_refs,
            } => {
                if !request
                    .active_agents
                    .iter()
                    .any(|card| card.id == *agent_id)
                    || message.trim().is_empty()
                    || message.len() > 4096
                    || !floe_agent_contract::valid_context_refs(context_refs)
                {
                    return Err(AgentFailure::InvalidModelOutput);
                }
            }
        }
    }
    Ok(())
}
