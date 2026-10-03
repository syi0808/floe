//! Communication assessment over confirmed interactions.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::prompts::{PromptAssembly, PromptComponentKind};
use floe_agent_contract::{AGENT_VERSION, AgentFailure, AgentContext};
use floe_context_contract::{
    CommunicationView, communication_context_evidence, validate_communication_view,
    MAX_COMMUNICATION_BYTES, MAX_COMMUNICATION_ITEMS,
};
use floe_experts::{
    ExpertFinalOutput, ExpertProgram, ExpertProgramRequest, ExpertProgramSpec,
    ExpertToolObservation,
};

use crate::prompts::communication_expert_prompt;

const MAIL_REQUIREMENT: &str = "floe.source.mail";
const FINAL_JUDGMENT_INSTRUCTION: &str = "Before the final judgment, read every declared evidence tool marked as required. Return exactly one JSON object matching this package's output contract.";
const OUTPUT_CONTRACT: &str = r#"{"type":"object","additionalProperties":false,"properties":{"summary":{"type":"string","minLength":1,"maxLength":2048},"assessments":{"type":"array","maxItems":16,"items":{"type":"object","additionalProperties":false,"properties":{"evidence_handle":{"type":"string","minLength":1,"maxLength":128},"needs_reply":{"type":"boolean"},"rationale":{"type":"string","minLength":1,"maxLength":512},"channel":{"type":"string","const":"email"},"tone":{"type":"string","minLength":1,"maxLength":64},"draft":{"type":["string","null"],"minLength":1,"maxLength":4096}},"required":["evidence_handle","needs_reply","rationale","channel","tone","draft"]}}},"required":["summary","assessments"]}"#;

#[derive(Clone, Copy, Debug, Default)]
pub struct CommunicationProgram;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommunicationChannel {
    Email,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommunicationResultKind {
    #[default]
    NoReply,
    ReplyRecommended,
    DraftForReview,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommunicationAssessment {
    pub evidence_handle: String,
    pub needs_reply: bool,
    pub rationale: String,
    pub channel: CommunicationChannel,
    pub tone: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    #[serde(default)]
    pub result: CommunicationResultKind,
    #[serde(default)]
    pub requires_review: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommunicationExpertResult {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub source_handle: String,
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub assessments: Vec<CommunicationAssessment>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationModelOutput {
    summary: String,
    assessments: Vec<CommunicationAssessment>,
}

impl ExpertProgram for CommunicationProgram {
    fn specification(
        &self,
        request: &ExpertProgramRequest,
    ) -> Result<ExpertProgramSpec, AgentFailure> {
        let prompt = append_required_evidence_instruction(communication_expert_prompt())?;
        crate::program_support::specification(request, prompt, OUTPUT_CONTRACT)
    }

    fn finalize(
        &self,
        request: &ExpertProgramRequest,
        observations: &[ExpertToolObservation],
        text: &str,
        artifacts: &[floe_agent_contract::Artifact],
    ) -> Result<ExpertFinalOutput, AgentFailure> {
        if !artifacts.is_empty() {
            return Err(AgentFailure::InvalidModelOutput);
        }
        if text.len() > request.request.execution_context.max_output_bytes.min(8192) {
            return Err(AgentFailure::BudgetExceeded);
        }

        let view = match crate::program_support::read_one::<CommunicationView>(
            observations,
            MAIL_REQUIREMENT,
        )? {
            Some(view) => view,
            None if crate::program_support::was_unavailable(observations, MAIL_REQUIREMENT) => {
                return crate::program_support::unavailable(
                    request,
                    observations,
                    MAIL_REQUIREMENT,
                    crate::BuiltinExpertKind::Communication.result_artifact_name(),
                    super::RESULT_MEDIA_TYPE,
                    "Mail is temporarily unavailable, so there is no communication assessment.",
                );
            }
            None => return Err(AgentFailure::InvalidModelOutput),
        };

        validate_communication_view(
            &view,
            request.now_unix_ms,
            MAX_COMMUNICATION_ITEMS,
            MAX_COMMUNICATION_BYTES,
        )?;
        let mut context: AgentContext = request.context.clone();
        context.evidence.push(communication_context_evidence(&view)?);
        context.validate()?;

        let mut output: CommunicationModelOutput =
            serde_json::from_str(text).map_err(|_| AgentFailure::InvalidModelOutput)?;
        crate::shared::validate_summary(&output.summary)?;
        if output.assessments.len() > crate::shared::MAX_MAIL_EXPERT_FINDINGS {
            return Err(AgentFailure::BudgetExceeded);
        }
        let mut assessment_handles = std::collections::HashSet::new();
        for assessment in &mut output.assessments {
            if !view
                .items
                .iter()
                .any(|item| item.evidence_handle == assessment.evidence_handle)
                || assessment.rationale.trim().is_empty()
                || assessment.rationale.len() > 512
                || assessment.tone.trim().is_empty()
                || assessment.tone.len() > 64
                || assessment.draft.as_ref().is_some_and(|draft| {
                    draft.trim().is_empty() || draft.len() > 4096 || !assessment.needs_reply
                })
                || !assessment_handles.insert(assessment.evidence_handle.clone())
            {
                return Err(AgentFailure::InvalidModelOutput);
            }
            assessment.result = match (assessment.needs_reply, assessment.draft.is_some()) {
                (false, _) => CommunicationResultKind::NoReply,
                (true, false) => CommunicationResultKind::ReplyRecommended,
                (true, true) => CommunicationResultKind::DraftForReview,
            };
            assessment.requires_review = assessment.draft.is_some();
        }
        let result = CommunicationExpertResult {
            schema_version: AGENT_VERSION,
            invocation_id: request.request.invocation_key.as_uuid(),
            source_handle: view.source_handle,
            expires_at_unix_ms: view.expires_at_unix_ms,
            summary: output.summary,
            assessments: output.assessments,
        };
        crate::program_support::result(
            request,
            observations,
            crate::BuiltinExpertKind::Communication.result_artifact_name(),
            super::RESULT_MEDIA_TYPE,
            result.summary.clone(),
            &result,
        )
    }
}

fn append_required_evidence_instruction(
    mut prompt: PromptAssembly,
) -> Result<PromptAssembly, AgentFailure> {
    let role = prompt
        .components
        .iter_mut()
        .find(|component| component.kind == PromptComponentKind::Role)
        .ok_or(AgentFailure::InvalidInput)?;
    role.content.push_str("\n\n");
    role.content.push_str(FINAL_JUDGMENT_INSTRUCTION);
    prompt.validate()?;
    Ok(prompt)
}
