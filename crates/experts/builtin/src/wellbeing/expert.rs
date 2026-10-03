//! Wellbeing judgment and its schedule impact.

use floe_agent_contract::{AGENT_VERSION, AgentFailure, Artifact};
use floe_context_contract::{
    CalendarContextView, WellbeingView, validate_calendar_context_view, validate_wellbeing_view,
};
use floe_experts::{ExpertProgram, ExpertProgramRequest, ExpertProgramSpec, ExpertToolObservation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::prompts::wellbeing_expert_prompt;

const OUTPUT_CONTRACT: &str = r#"{
  "type":"object",
  "additionalProperties":false,
  "properties":{
    "summary":{"type":"string","minLength":1,"maxLength":2048},
    "schedule_impact":{"type":"string","enum":["keep_plan","reduce_load","protect_recovery","no_conclusion"]},
    "rationale":{"type":"string","minLength":1,"maxLength":512},
    "evidence_handles":{"type":"array","maxItems":16,"items":{"type":"string"}}
  },
  "required":["summary","schedule_impact","rationale","evidence_handles"],
  "allOf":[
    {"if":{"properties":{"schedule_impact":{"const":"no_conclusion"}},"required":["schedule_impact"]},"then":{"properties":{"evidence_handles":{"maxItems":0}}}},
    {"if":{"properties":{"schedule_impact":{"enum":["keep_plan","reduce_load","protect_recovery"]}},"required":["schedule_impact"]},"then":{"properties":{"evidence_handles":{"minItems":1}}}}
  ]
}"#;

const REQUIRED_EVIDENCE_INSTRUCTION: &str = "\n\nRead every required evidence tool before forming a final judgment. Return only one JSON object that conforms to this package's supplied output contract.";
const UNAVAILABLE_SUMMARY: &str =
    "Wellbeing is temporarily unavailable, so there is no wellbeing assessment.";

#[derive(Clone, Copy, Debug, Default)]
pub struct WellbeingProgram;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleImpact {
    KeepPlan,
    ReduceLoad,
    ProtectRecovery,
    NoConclusion,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WellbeingExpertResult {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub source_handle: String,
    #[serde(default)]
    pub source_handles: Vec<String>,
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub schedule_impact: ScheduleImpact,
    pub rationale: String,
    pub evidence_handles: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WellbeingOutput {
    summary: String,
    schedule_impact: ScheduleImpact,
    rationale: String,
    evidence_handles: Vec<String>,
}

impl ExpertProgram for WellbeingProgram {
    fn specification(
        &self,
        request: &ExpertProgramRequest,
    ) -> Result<ExpertProgramSpec, AgentFailure> {
        let mut prompt = wellbeing_expert_prompt();
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

        let wellbeing_key = crate::BuiltinContextSource::Wellbeing.source_id();
        let wellbeing =
            match crate::program_support::read_one::<WellbeingView>(observations, wellbeing_key)? {
                Some(view) => view,
                None if crate::program_support::was_unavailable(observations, wellbeing_key) => {
                    return crate::program_support::unavailable(
                        request,
                        observations,
                        wellbeing_key,
                        crate::BuiltinExpertKind::Wellbeing.result_artifact_name(),
                        super::RESULT_MEDIA_TYPE,
                        UNAVAILABLE_SUMMARY,
                    );
                }
                None => return Err(AgentFailure::InvalidModelOutput),
            };

        validate_wellbeing_view(&wellbeing, request.now_unix_ms)?;
        let mut available = wellbeing.evidence_handles.clone();
        let mut source_handles = vec![wellbeing.source_handle.clone()];
        let mut expires_at_unix_ms = wellbeing.expires_at_unix_ms;

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

        if text.len() > request.request.execution_context.max_output_bytes.min(8192) {
            return Err(AgentFailure::BudgetExceeded);
        }
        let output: WellbeingOutput =
            serde_json::from_str(text).map_err(|_| AgentFailure::InvalidModelOutput)?;
        validate_judgment(
            &output.summary,
            &output.rationale,
            &output.evidence_handles,
            &available,
            matches!(output.schedule_impact, ScheduleImpact::NoConclusion),
        )?;

        let result = WellbeingExpertResult {
            schema_version: AGENT_VERSION,
            invocation_id: request.request.invocation_key.as_uuid(),
            source_handle: wellbeing.source_handle,
            source_handles,
            expires_at_unix_ms,
            summary: output.summary,
            schedule_impact: output.schedule_impact,
            rationale: output.rationale,
            evidence_handles: output.evidence_handles,
        };
        crate::program_support::result(
            request,
            observations,
            crate::BuiltinExpertKind::Wellbeing.result_artifact_name(),
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
