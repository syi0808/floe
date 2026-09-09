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
        let accounting = request.usage.begin(
            &mut request.remaining_tokens,
            &mut request.remaining_cost_micros,
        )?;
        let mut record = crate::ModelAttemptRecord {
            id: uuid::Uuid::new_v4(),
            turn_id: request.turn_id,
            scope_id: request.session_id,
            attempt: attempt + 1,
            placement: model.placement(),
            state: crate::ModelAttemptState::Started,
            failure: None,
            usage: crate::ModelUsage {
                attempts: 1,
                tokens: request.remaining_tokens.min(4096),
                estimated_tokens: request.remaining_tokens.min(4096),
                cost_micros: 0,
            },
        };
        request.usage.record(record.clone()).await?;
        let result = tokio::select! {
            biased;
            _ = request.cancellation.cancelled() => Err(AgentFailure::Cancelled),
            _ = tokio::time::sleep_until(request.deadline) => Err(AgentFailure::DeadlineExceeded),
            result = model.generate(request.clone()) => result,
        };
        let mut consumed_tokens = 4096;
        let mut consumed_cost = 0;
        let result = result.and_then(|response| {
            consumed_tokens = response.used_tokens;
            consumed_cost = response.cost_micros;
            record.usage.tokens = consumed_tokens;
            record.usage.cost_micros = consumed_cost;
            record.usage.estimated_tokens = 0;
            accounting.settle(consumed_tokens, consumed_cost)?;
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
                        let value: serde_json::Value = serde_json::from_str(input)
                            .map_err(|_| AgentFailure::InvalidModelOutput)?;
                        if !value.is_object()
                            || validators[index]
                                .as_ref()
                                .is_some_and(|validator| !validator.is_valid(&value))
                        {
                            return Err(AgentFailure::InvalidModelOutput);
                        }
                    }
                    ModelStep::Delegate { agent_id, message } => {
                        if !request
                            .active_agents
                            .iter()
                            .any(|card| card.id == *agent_id)
                            || message.trim().is_empty()
                            || message.len() > 4096
                        {
                            return Err(AgentFailure::InvalidModelOutput);
                        }
                    }
                }
            }
            Ok(response)
        });
        record.state = match &result {
            Ok(_) => crate::ModelAttemptState::Accepted,
            Err(AgentFailure::Cancelled | AgentFailure::DeadlineExceeded) => {
                crate::ModelAttemptState::Interrupted
            }
            Err(_) => crate::ModelAttemptState::Rejected,
        };
        record.failure = result.as_ref().err().copied();
        request.usage.record(record).await?;
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
