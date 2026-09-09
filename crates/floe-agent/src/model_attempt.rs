use crate::{AgentFailure, AgentMessage, ModelRequest, ModelResponse, ModelRunner, ModelStep};

pub async fn generate_with_recovery<Model: ModelRunner>(
    model: &Model,
    mut request: ModelRequest,
) -> Result<ModelResponse, AgentFailure> {
    let mut reserved_tokens = 0;
    let mut reserved_cost = 0;
    let validators = request
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
        .collect::<Result<Vec<_>, _>>()?;
    for attempt in 0..2 {
        if request.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if request.deadline <= tokio::time::Instant::now() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        let result = tokio::select! {
            biased;
            _ = request.cancellation.cancelled() => return Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(request.deadline) => return Err(AgentFailure::DeadlineExceeded),
            result = model.generate(request.clone()) => result,
        };
        let mut consumed_tokens = 4096;
        let mut consumed_cost = 0;
        let result = result.and_then(|response| {
            consumed_tokens = response.used_tokens;
            consumed_cost = response.cost_micros;
            if response.used_tokens > request.remaining_tokens
                || response.cost_micros > request.remaining_cost_micros
                || serde_json::to_vec(&response.step)
                    .map_err(|_| AgentFailure::InvalidModelOutput)?
                    .len()
                    > request.max_output_bytes
            {
                return Err(AgentFailure::BudgetExceeded);
            }
            if response.schema_version != crate::AGENT_VERSION
                || matches!(&response.step, ModelStep::Answer { text } if text.trim().is_empty())
            {
                return Err(AgentFailure::InvalidModelOutput);
            }
            if let ModelStep::Call {
                capability_id,
                input,
            } = &response.step
            {
                let index = request
                    .capabilities
                    .iter()
                    .position(|capability| capability.id == *capability_id)
                    .ok_or(AgentFailure::CapabilityDenied)?;
                if let Some(validator) = &validators[index] {
                    let value: serde_json::Value = serde_json::from_str(input)
                        .map_err(|_| AgentFailure::InvalidModelOutput)?;
                    if !validator.is_valid(&value) {
                        return Err(AgentFailure::InvalidModelOutput);
                    }
                }
            }
            Ok(response)
        });
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
                    text: include_str!("../prompts/model_correction.txt")
                        .trim()
                        .into(),
                });
            }
            Err(failure) => return Err(failure),
        }
    }
    Err(AgentFailure::InvalidModelOutput)
}
