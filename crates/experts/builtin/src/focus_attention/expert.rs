//! Focus and attention recommendations.

use floe_agent_contract::{AGENT_VERSION, AgentFailure, Artifact};
use floe_context_contract::{
    AttentionView, CalendarContextView, WorkContextView, validate_attention_view,
    validate_calendar_context_view, validate_work_context_view,
};
use floe_experts::{ExpertProgram, ExpertProgramRequest, ExpertProgramSpec,
    ExpertToolObservation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::prompts::focus_expert_prompt;

const OUTPUT_CONTRACT: &str = r#"{
  "type":"object",
  "additionalProperties":false,
  "properties":{
    "summary":{"type":"string","minLength":1,"maxLength":2048},
    "recommendation":{"type":"string","enum":["protect_focus","available_for_interruptions","no_conclusion"]},
    "rationale":{"type":"string","minLength":1,"maxLength":512},
    "evidence_handles":{"type":"array","maxItems":16,"items":{"type":"string"}}
  },
  "required":["summary","recommendation","rationale","evidence_handles"],
  "allOf":[
    {"if":{"properties":{"recommendation":{"const":"no_conclusion"}},"required":["recommendation"]},"then":{"properties":{"evidence_handles":{"maxItems":0}}}},
    {"if":{"properties":{"recommendation":{"enum":["protect_focus","available_for_interruptions"]}},"required":["recommendation"]},"then":{"properties":{"evidence_handles":{"minItems":1}}}}
  ]
}"#;

const REQUIRED_EVIDENCE_INSTRUCTION: &str = "\n\nRead every required evidence tool before forming a final judgment. Return only one JSON object that conforms to this package's supplied output contract.";
const UNAVAILABLE_SUMMARY: &str =
    "Attention is temporarily unavailable, so there is no focus assessment.";

#[derive(Clone, Copy, Debug, Default)]
pub struct FocusAttentionProgram;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusRecommendation {
    ProtectFocus,
    AvailableForInterruptions,
    NoConclusion,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FocusExpertResult {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub source_handle: String,
    #[serde(default)]
    pub source_handles: Vec<String>,
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub recommendation: FocusRecommendation,
    pub rationale: String,
    pub evidence_handles: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FocusOutput {
    summary: String,
    recommendation: FocusRecommendation,
    rationale: String,
    evidence_handles: Vec<String>,
}

impl ExpertProgram for FocusAttentionProgram {
    fn specification(
        &self,
        request: &ExpertProgramRequest,
    ) -> Result<ExpertProgramSpec, AgentFailure> {
        let mut prompt = focus_expert_prompt();
        append_evidence_instruction(&mut prompt)?;
        crate::program_support::specification(request, prompt, OUTPUT_CONTRACT)
    }

    fn finalize(
        &self,
        request: &ExpertProgramRequest,
        observations: &[ExpertToolObservation],
        text: &str,
        artifacts: &[Artifact],
    ) -> Result<floe_experts::ExpertFinalOutput, AgentFailure> {
        if !artifacts.is_empty() {
            return Err(AgentFailure::InvalidModelOutput);
        }

        let attention_key = crate::BuiltinContextSource::Attention.source_id();
        let attention = match crate::program_support::read_one::<AttentionView>(
            observations,
            attention_key,
        )? {
            Some(view) => view,
            None if crate::program_support::was_unavailable(observations, attention_key) => {
                return crate::program_support::unavailable(
                    request,
                    observations,
                    attention_key,
                    crate::BuiltinExpertKind::FocusAttention.result_artifact_name(),
                    super::RESULT_MEDIA_TYPE,
                    UNAVAILABLE_SUMMARY,
                );
            }
            None => return Err(AgentFailure::InvalidModelOutput),
        };

        validate_attention_view(&attention, request.now_unix_ms)?;
        let mut available = attention.evidence_handles.clone();
        let mut source_handles = vec![attention.source_handle.clone()];
        let mut expires_at_unix_ms = attention.expires_at_unix_ms;

        let calendar_key = crate::BuiltinContextSource::Calendar.source_id();
        if let Some(calendars) = crate::program_support::read_one::<Vec<CalendarContextView>>(
            observations,
            calendar_key,
        )? {
            add_calendar_views(
                &calendars,
                request.now_unix_ms,
                &mut available,
                &mut source_handles,
                &mut expires_at_unix_ms,
            )?;
        }

        let work_key = crate::BuiltinContextSource::WorkContext.source_id();
        if let Some(view) = crate::program_support::read_one::<WorkContextView>(
            observations,
            work_key,
        )? {
            validate_work_context_view(&view, request.now_unix_ms)?;
            ensure_unique_source(&source_handles, &view.source_handle)?;
            extend_unique_handles(
                &mut available,
                view.items.iter().map(|item| &item.evidence_handle),
            )?;
            source_handles.push(view.source_handle.clone());
            expires_at_unix_ms = expires_at_unix_ms.min(view.expires_at_unix_ms);
        }

        if text.len() > request.request.execution_context.max_output_bytes.min(8192) {
            return Err(AgentFailure::BudgetExceeded);
        }
        let output: FocusOutput =
            serde_json::from_str(text).map_err(|_| AgentFailure::InvalidModelOutput)?;
        validate_judgment(
            &output.summary,
            &output.rationale,
            &output.evidence_handles,
            &available,
            matches!(output.recommendation, FocusRecommendation::NoConclusion),
        )?;

        let result = FocusExpertResult {
            schema_version: AGENT_VERSION,
            invocation_id: request.request.invocation_key.as_uuid(),
            source_handle: attention.source_handle,
            source_handles,
            expires_at_unix_ms,
            summary: output.summary,
            recommendation: output.recommendation,
            rationale: output.rationale,
            evidence_handles: output.evidence_handles,
        };
        crate::program_support::result(
            request,
            observations,
            crate::BuiltinExpertKind::FocusAttention.result_artifact_name(),
            super::RESULT_MEDIA_TYPE,
            result.summary.clone(),
            &result,
        )
    }
}

fn append_evidence_instruction(
    prompt: &mut floe_agent_contract::prompts::PromptAssembly,
) -> Result<(), AgentFailure> {
    let role = prompt
        .components
        .iter_mut()
        .find(|component| component.kind == floe_agent_contract::prompts::PromptComponentKind::Role)
        .ok_or(AgentFailure::InvalidInput)?;
    role.content.push_str(REQUIRED_EVIDENCE_INSTRUCTION);
    prompt.validate()
}

fn add_calendar_views(
    calendars: &[CalendarContextView],
    now_unix_ms: i64,
    available: &mut Vec<String>,
    source_handles: &mut Vec<String>,
    expires_at_unix_ms: &mut i64,
) -> Result<(), AgentFailure> {
    for view in calendars {
        validate_calendar_context_view(view, now_unix_ms)?;
        ensure_unique_source(source_handles, &view.source_handle)?;
        extend_unique_handles(
            available,
            view.items.iter().map(|item| &item.evidence_handle),
        )?;
        source_handles.push(view.source_handle.clone());
        *expires_at_unix_ms = (*expires_at_unix_ms).min(view.expires_at_unix_ms);
    }
    Ok(())
}

fn ensure_unique_source(source_handles: &[String], source: &str) -> Result<(), AgentFailure> {
    if source_handles.iter().any(|value| value == source) {
        Err(AgentFailure::InvalidInput)
    } else {
        Ok(())
    }
}

fn extend_unique_handles<'a>(
    available: &mut Vec<String>,
    handles: impl Iterator<Item = &'a String>,
) -> Result<(), AgentFailure> {
    for handle in handles {
        if available.contains(handle) {
            return Err(AgentFailure::InvalidInput);
        }
        available.push(handle.clone());
    }
    Ok(())
}

fn validate_summary(summary: &str) -> Result<(), AgentFailure> {
    if !summary.trim().is_empty() && summary.len() <= 2048 {
        Ok(())
    } else {
        Err(AgentFailure::InvalidModelOutput)
    }
}

fn validate_judgment(
    summary: &str,
    rationale: &str,
    evidence_handles: &[String],
    available: &[String],
    no_conclusion: bool,
) -> Result<(), AgentFailure> {
    validate_summary(summary)?;
    if rationale.trim().is_empty()
        || rationale.len() > 512
        || evidence_handles.len() > 16
        || no_conclusion != evidence_handles.is_empty()
        || evidence_handles
            .iter()
            .any(|handle| !available.contains(handle))
    {
        Err(AgentFailure::InvalidModelOutput)
    } else {
        Ok(())
    }
}
