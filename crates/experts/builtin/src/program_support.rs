//! Pure helpers for package roles and final judgments over actual source-tool observations.
use floe_agent_contract::{AgentFailure, Artifact, ArtifactPart, DependencyCoverage};
use floe_experts::{
    ExpertFinalOutput, ExpertProgramRequest, ExpertProgramSpec, ExpertSourceObservation,
    ExpertToolObservation, ExpertToolSpec,
};
use serde::de::DeserializeOwned;
use uuid::Uuid;

pub fn specification(
    request: &ExpertProgramRequest,
    prompt: floe_agent_contract::prompts::PromptAssembly,
    output_contract: &str,
) -> Result<ExpertProgramSpec, AgentFailure> {
    prompt.validate()?;
    let tools = request.selection.requirements.iter().map(|requirement| {
        let schema = match requirement.capability.as_str() {
            "calendar.timeline" => serde_json::json!({
                "type":"object","additionalProperties":false,
                "properties":{
                    "range_start_unix_ms":{"type":"integer","minimum":0},
                    "range_end_unix_ms":{"type":"integer","minimum":1},
                    "cursor":{"type":"string","maxLength":2048},
                    "limit":{"type":"integer","minimum":1,"maximum":floe_context_contract::MAX_CALENDAR_CONTEXT_ITEMS}
                },"required":["range_start_unix_ms","range_end_unix_ms","limit"]
            }),
            "mail.communication" => serde_json::json!({
                "type":"object","additionalProperties":false,
                "properties":{
                    "schema_version":{"const":floe_agent_contract::AGENT_VERSION},
                    "query":{"type":"string","maxLength":512},
                    "cursor":{"type":"integer","minimum":0},
                    "limit":{"type":"integer","minimum":1,"maximum":25}
                },"required":["schema_version","query","cursor","limit"]
            }),
            // A source owner resolves the declared capability. Models cannot
            // supply identity payloads as fresh source authority.
            _ => serde_json::json!({"type":"object","additionalProperties":false,
                "properties":{"schema_version":{"const":floe_agent_contract::AGENT_VERSION}},
                "required":["schema_version"]}),
        };
        Ok(ExpertToolSpec { requirement_key: requirement.key.clone(),
            description: format!("Read declared {} evidence. {}", requirement.capability,
                if requirement.minimum_sources > 0 { "Read this source before answering." } else { "Read when useful for the assignment." }),
            input_schema: serde_json::to_string(&schema).map_err(|_| AgentFailure::InvalidInput)?,
        })
    }).collect::<Result<Vec<_>, AgentFailure>>()?;
    Ok(ExpertProgramSpec {
        prompt,
        output_contract: output_contract.to_owned(),
        tools,
    })
}

pub fn read_one<Value: DeserializeOwned>(
    observations: &[ExpertToolObservation],
    key: &str,
) -> Result<Option<Value>, AgentFailure> {
    let values = observations
        .iter()
        .filter(|read| read.requirement_key == key)
        .filter_map(|read| match &read.outcome {
            ExpertSourceObservation::Ready { payload, .. } => Some(payload),
            ExpertSourceObservation::Unavailable { .. } => None,
        })
        .collect::<Vec<_>>();
    match values.as_slice() {
        [] => Ok(None),
        [value] => serde_json::from_value((*value).clone())
            .map(Some)
            .map_err(|_| AgentFailure::InvalidModelOutput),
        _ => Err(AgentFailure::InvalidModelOutput),
    }
}

pub fn was_unavailable(observations: &[ExpertToolObservation], key: &str) -> bool {
    observations
        .iter()
        .rev()
        .find(|read| read.requirement_key == key)
        .is_some_and(|read| matches!(read.outcome, ExpertSourceObservation::Unavailable { .. }))
}

pub fn coverage(
    observations: &[ExpertToolObservation],
) -> Result<DependencyCoverage, AgentFailure> {
    observations
        .iter()
        .try_fold(DependencyCoverage::Independent, |coverage, read| {
            coverage
                .merge(&read.coverage())
                .map_err(|_| AgentFailure::PolicyDenied)
        })
}

pub fn result<Result_: serde::Serialize>(
    request: &ExpertProgramRequest,
    observations: &[ExpertToolObservation],
    name: &str,
    media_type: &str,
    text: String,
    result: &Result_,
) -> Result<ExpertFinalOutput, AgentFailure> {
    if text.trim().is_empty() || text.len() > request.request.execution_context.max_output_bytes {
        return Err(AgentFailure::InvalidModelOutput);
    }
    if request.coverage == DependencyCoverage::Unknown
        || request
            .coverage
            .merge(&coverage(observations)?)
            .map_err(|_| AgentFailure::PolicyDenied)?
            != request.coverage
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let data = serde_json::to_string(result).map_err(|_| AgentFailure::InvalidModelOutput)?;
    let artifact = Artifact {
        artifact_id: Uuid::new_v5(&request.request.task_id.as_uuid(), name.as_bytes()),
        name: name.to_owned(),
        parts: vec![ArtifactPart::Data {
            media_type: media_type.to_owned(),
            data,
        }],
        coverage: request.coverage.clone(),
    };
    artifact.validate(request.request.execution_context.max_output_bytes)?;
    Ok(ExpertFinalOutput {
        payload: floe_agent_contract::ValidatedFinalPayload {
            text,
            artifacts: vec![artifact],
        },
        settlement: None,
    })
}

pub fn unavailable(
    request: &ExpertProgramRequest,
    observations: &[ExpertToolObservation],
    requirement_key: &str,
    name: &str,
    media_type: &str,
    summary: &str,
) -> Result<ExpertFinalOutput, AgentFailure> {
    if !was_unavailable(observations, requirement_key) {
        return Err(AgentFailure::InvalidModelOutput);
    }
    result(
        request,
        observations,
        name,
        media_type,
        summary.to_owned(),
        &serde_json::json!({
            "schema_version":floe_agent_contract::AGENT_VERSION,"status":"unavailable","summary":summary
        }),
    )
}
