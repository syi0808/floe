//! Values and validation shared by the builtin Experts.

use floe_kernel::PersonId;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::time::Instant;
use uuid::Uuid;

use crate::prompts::{focus_expert_prompt, relationships_expert_prompt, wellbeing_expert_prompt};
use floe_context::{AttentionView, CalendarContextView, PeopleView, WellbeingView, WorkContextView, calendar_context_evidence, personal_context_evidence, validate_attention_view, validate_calendar_context_view, validate_people_view, validate_wellbeing_view, validate_work_context_view, work_context_evidence};
use floe_agent_contract::{AgentFailure, DataClass, SessionProtection};
use floe_context::{AgentContext, ContextEvidence, InferencePolicyDecision};
use floe_kernel::AGENT_VERSION;
use floe_conversation::{AgentMessage, ModelRequest, ModelRunner, ModelStep};
use floe_conversation::{UsageLedger, generate_with_recovery};
use floe_knowledge::prompts::{PromptAssembly};

pub struct PersonalExpertInvocation {
    pub usage: UsageLedger,
    pub person_id: PersonId,
    pub invocation_id: Uuid,
    pub assignment: String,
    pub current_time_unix_ms: i64,
    pub context: AgentContext,
    pub max_output_bytes: usize,
    pub max_model_tokens: u64,
    pub max_model_cost_micros: u64,
    pub deadline: Instant,
    pub cancellation: floe_execution::Cancellation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
fn add_schedule_views(
    calendars: &[CalendarContextView],
    now_unix_ms: i64,
    evidence: &mut Vec<ContextEvidence>,
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
        evidence.push(calendar_context_evidence(view)?);
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

fn valid_handle(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128
}

async fn run_personal_model<Output: DeserializeOwned, Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: &PersonalExpertInvocation,
    evidence: Vec<ContextEvidence>,
    prompt: PromptAssembly,
) -> Result<Output, AgentFailure> {
    if invocation.assignment.trim().is_empty()
        || invocation.assignment.len() > 2048
        || invocation.max_output_bytes == 0
        || invocation.max_model_tokens == 0
        || invocation.deadline <= Instant::now()
        || invocation.cancellation.is_cancelled()
    {
        return Err(AgentFailure::InvalidInput);
    }
    let mut context = invocation.context.clone();
    context.evidence.extend(evidence);
    policy.authorize(
        model.placement(),
        SessionProtection::Encrypted,
        &context,
        u64::try_from(invocation.current_time_unix_ms).map_err(|_| AgentFailure::InvalidInput)?,
    )?;
    let turn_id = Uuid::new_v4();
    let response = generate_with_recovery(
        model,
        ModelRequest {
            usage: invocation.usage.clone(),
            replay: vec![],
            schema_version: AGENT_VERSION,
            prompt,
            person_id: invocation.person_id,
            session_id: invocation.invocation_id,
            turn_id,
            policy: policy.clone(),
            context,
            messages: vec![AgentMessage::User {
                turn_id,
                text: invocation.assignment.clone(),
            }],
            capabilities: vec![],
            active_agents: vec![],
            remaining_tokens: invocation.max_model_tokens,
            remaining_cost_micros: invocation.max_model_cost_micros,
            max_output_bytes: invocation.max_output_bytes.min(8192),
            deadline: invocation.deadline,
            cancellation: invocation.cancellation.clone(),
        },
    )
    .await?;
    if response.schema_version != AGENT_VERSION
        || response.used_tokens > invocation.max_model_tokens
        || response.cost_micros > invocation.max_model_cost_micros
    {
        return Err(AgentFailure::BudgetExceeded);
    }
    let [ModelStep::Answer { text }] = response.output.as_slice() else {
        return Err(AgentFailure::InvalidModelOutput);
    };
    if text.len() > invocation.max_output_bytes.min(8192) {
        return Err(AgentFailure::BudgetExceeded);
    }
    serde_json::from_str(text).map_err(|_| AgentFailure::InvalidModelOutput)
}

fn validate_summary(summary: &str) -> Result<(), AgentFailure> {
    if summary.trim().is_empty() || summary.len() > 2048 {
        Err(AgentFailure::InvalidModelOutput)
    } else {
        Ok(())
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
