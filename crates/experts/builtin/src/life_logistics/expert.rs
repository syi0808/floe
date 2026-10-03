//! Life logistics preparation and urgency over captured, admitted evidence.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::prompts::{PromptAssembly, PromptComponentKind};
use floe_agent_contract::{AGENT_VERSION, AgentFailure, Artifact};
use floe_context_contract::{LogisticsView, validate_logistics_view};
use floe_experts::{
    ExpertFinalOutput, ExpertProgram, ExpertProgramRequest, ExpertProgramSpec,
    ExpertToolObservation,
};

use crate::BuiltinContextSource;
use crate::prompts::life_logistics_expert_prompt;
use crate::shared::{valid_text, validate_summary};

const MAX_MODEL_OUTPUT_BYTES: usize = 8192;
const ROLE_APPENDIX: &str = "\n\nRead every required evidence tool before making your final judgment. Return the final answer as one JSON object matching this package output schema:\n";

#[derive(Clone, Copy, Debug, Default)]
pub struct LifeLogisticsProgram;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LogisticsUrgency {
    Now,
    Soon,
    Later,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LogisticsPreparation {
    pub evidence_handle: String,
    pub recommendation: String,
    pub urgency: LogisticsUrgency,
    pub requires_approval: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LifeLogisticsExpertResult {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub source_handle: String,
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub preparations: Vec<LogisticsPreparation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LogisticsOutput {
    summary: String,
    preparations: Vec<LogisticsPreparation>,
}

impl ExpertProgram for LifeLogisticsProgram {
    fn specification(
        &self,
        request: &ExpertProgramRequest,
    ) -> Result<ExpertProgramSpec, AgentFailure> {
        let output_contract = life_logistics_output_contract();
        let prompt = append_output_contract(life_logistics_expert_prompt(), &output_contract)?;
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

        let requirement_key = BuiltinContextSource::Logistics.source_id();
        let view =
            match crate::program_support::read_one::<LogisticsView>(observations, requirement_key)?
            {
                Some(view) => view,
                None if crate::program_support::was_unavailable(observations, requirement_key) => {
                    return crate::program_support::unavailable(
                        request,
                        observations,
                        requirement_key,
                        crate::BuiltinExpertKind::LifeLogistics.result_artifact_name(),
                        super::RESULT_MEDIA_TYPE,
                        "Logistics are temporarily unavailable, so there is no logistics plan.",
                    );
                }
                None => return Err(AgentFailure::InvalidModelOutput),
            };
        validate_logistics_view(&view, request.now_unix_ms)?;
        if text.len()
            > request
                .request
                .execution_context
                .max_output_bytes
                .min(MAX_MODEL_OUTPUT_BYTES)
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        let output: LogisticsOutput =
            serde_json::from_str(text).map_err(|_| AgentFailure::InvalidModelOutput)?;
        validate_summary(&output.summary)?;
        if output.preparations.len() > 16 {
            return Err(AgentFailure::BudgetExceeded);
        }
        for (index, preparation) in output.preparations.iter().enumerate() {
            if !view
                .items
                .iter()
                .any(|item| item.evidence_handle == preparation.evidence_handle)
                || !valid_text(&preparation.recommendation, 512)
                || output.preparations[..index]
                    .iter()
                    .any(|other| other.evidence_handle == preparation.evidence_handle)
            {
                return Err(AgentFailure::InvalidModelOutput);
            }
        }

        let result = LifeLogisticsExpertResult {
            schema_version: AGENT_VERSION,
            invocation_id: request.request.invocation_key.as_uuid(),
            source_handle: view.source_handle,
            expires_at_unix_ms: view.expires_at_unix_ms,
            summary: output.summary,
            preparations: output.preparations,
        };
        crate::program_support::result(
            request,
            observations,
            crate::BuiltinExpertKind::LifeLogistics.result_artifact_name(),
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

fn life_logistics_output_contract() -> String {
    serde_json::json!({
        "type": "object",
        "properties": {
            "summary": { "type": "string", "minLength": 1, "maxLength": 2048 },
            "preparations": {
                "type": "array",
                "maxItems": 16,
                "items": {
                    "type": "object",
                    "properties": {
                        "evidence_handle": { "type": "string", "minLength": 1, "maxLength": 128 },
                        "recommendation": { "type": "string", "minLength": 1, "maxLength": 512 },
                        "urgency": { "type": "string", "enum": ["now", "soon", "later"] },
                        "requires_approval": { "type": "boolean" }
                    },
                    "required": ["evidence_handle", "recommendation", "urgency", "requires_approval"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["summary", "preparations"],
        "additionalProperties": false
    })
    .to_string()
}
