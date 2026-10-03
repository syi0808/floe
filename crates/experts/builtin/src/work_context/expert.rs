//! Work context insights over captured, admitted evidence.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::prompts::{PromptAssembly, PromptComponentKind};
use floe_agent_contract::{AGENT_VERSION, AgentFailure, Artifact};
use floe_context_contract::{WorkContextView, validate_work_context_view};
use floe_experts::{
    ExpertFinalOutput, ExpertProgram, ExpertProgramRequest, ExpertProgramSpec,
    ExpertToolObservation,
};

use crate::BuiltinContextSource;
use crate::prompts::work_context_expert_prompt;
use crate::shared::{valid_text, validate_summary};

const MAX_MODEL_OUTPUT_BYTES: usize = 8192;
const ROLE_APPENDIX: &str =
    "\n\nRead every required evidence tool before making your final judgment. Return the final answer as one JSON object matching this package output schema:\n";

#[derive(Clone, Copy, Debug, Default)]
pub struct WorkContextProgram;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkInsight {
    pub evidence_handle: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
    pub confidence_millis: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkContextExpertResult {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub source_handle: String,
    pub scope_handle: String,
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub insights: Vec<WorkInsight>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkOutput {
    summary: String,
    insights: Vec<WorkInsight>,
}

impl ExpertProgram for WorkContextProgram {
    fn specification(
        &self,
        request: &ExpertProgramRequest,
    ) -> Result<ExpertProgramSpec, AgentFailure> {
        let output_contract = work_context_output_contract();
        let prompt = append_output_contract(work_context_expert_prompt(), &output_contract)?;
        crate::program_support::specification(request, prompt, &output_contract)
    }

    fn finalize(
        &self,
        request: &ExpertProgramRequest,
        observations: &[ExpertToolObservation],
        text: &str,
        artifacts: &[Artifact],
    ) -> Result<ExpertFinalOutput, AgentFailure> {
        if !artifacts.is_empty() {
            return Err(AgentFailure::InvalidModelOutput);
        }

        let requirement_key = BuiltinContextSource::WorkContext.source_id();
        let view = match crate::program_support::read_one::<WorkContextView>(
            observations,
            requirement_key,
        )? {
            Some(view) => view,
            None if crate::program_support::was_unavailable(observations, requirement_key) => {
                return crate::program_support::unavailable(
                    request,
                    observations,
                    requirement_key,
                    crate::BuiltinExpertKind::WorkContext.result_artifact_name(),
                    super::RESULT_MEDIA_TYPE,
                    "Work context is temporarily unavailable, so there is no work assessment.",
                );
            }
            None => return Err(AgentFailure::InvalidModelOutput),
        };
        validate_work_context_view(&view, request.now_unix_ms)?;
        if text.len() > request.request.execution_context.max_output_bytes.min(MAX_MODEL_OUTPUT_BYTES)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let output: WorkOutput =
            serde_json::from_str(text).map_err(|_| AgentFailure::InvalidModelOutput)?;
        validate_summary(&output.summary)?;
        if output.insights.len() > 16 {
            return Err(AgentFailure::BudgetExceeded);
        }
        for (index, insight) in output.insights.iter().enumerate() {
            if !view
                .items
                .iter()
                .any(|item| item.evidence_handle == insight.evidence_handle)
                || insight.blocker.is_none() && insight.next_action.is_none()
                || insight
                    .blocker
                    .as_ref()
                    .is_some_and(|value| !valid_text(value, 512))
                || insight
                    .next_action
                    .as_ref()
                    .is_some_and(|value| !valid_text(value, 512))
                || insight.confidence_millis == 0
                || insight.confidence_millis > 1000
                || output.insights[..index]
                    .iter()
                    .any(|other| other.evidence_handle == insight.evidence_handle)
            {
                return Err(AgentFailure::InvalidModelOutput);
            }
        }

        let result = WorkContextExpertResult {
            schema_version: AGENT_VERSION,
            invocation_id: request.request.invocation_key.as_uuid(),
            source_handle: view.source_handle,
            scope_handle: view.scope_handle,
            expires_at_unix_ms: view.expires_at_unix_ms,
            summary: output.summary,
            insights: output.insights,
        };
        crate::program_support::result(
            request,
            observations,
            crate::BuiltinExpertKind::WorkContext.result_artifact_name(),
            super::RESULT_MEDIA_TYPE,
            result.summary.clone(),
            &result,
        )
    }
}

fn append_output_contract(
    mut prompt: PromptAssembly,
    output_contract: &str,
) -> Result<PromptAssembly, AgentFailure> {
    let mut roles = prompt
        .components
        .iter_mut()
        .filter(|component| component.kind == PromptComponentKind::Role);
    let role = roles.next().ok_or(AgentFailure::InvalidInput)?;
    if roles.next().is_some() {
        return Err(AgentFailure::InvalidInput);
    }
    let appendix = format!("{ROLE_APPENDIX}{output_contract}");
    let length = role
        .content
        .len()
        .checked_add(appendix.len())
        .ok_or(AgentFailure::InvalidInput)?;
    if length > 4096 {
        return Err(AgentFailure::InvalidInput);
    }
    role.content.push_str(&appendix);
    Ok(prompt)
}

fn work_context_output_contract() -> String {
    serde_json::json!({
        "type": "object",
        "properties": {
            "summary": { "type": "string", "minLength": 1, "maxLength": 2048 },
            "insights": {
                "type": "array",
                "maxItems": 16,
                "items": {
                    "type": "object",
                    "properties": {
                        "evidence_handle": { "type": "string", "minLength": 1, "maxLength": 128 },
                        "blocker": { "type": ["string", "null"], "minLength": 1, "maxLength": 512 },
                        "next_action": { "type": ["string", "null"], "minLength": 1, "maxLength": 512 },
                        "confidence_millis": { "type": "integer", "minimum": 1, "maximum": 1000 }
                    },
                    "required": ["evidence_handle", "confidence_millis"],
                    "anyOf": [
                        {
                            "required": ["blocker"],
                            "properties": { "blocker": { "type": "string", "minLength": 1 } }
                        },
                        {
                            "required": ["next_action"],
                            "properties": { "next_action": { "type": "string", "minLength": 1 } }
                        }
                    ],
                    "additionalProperties": false
                }
            }
        },
        "required": ["summary", "insights"],
        "additionalProperties": false
    })
    .to_string()
}
