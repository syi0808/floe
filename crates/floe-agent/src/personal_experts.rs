use floe_domain::PersonId;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::time::Instant;
use uuid::Uuid;

use crate::{
    AGENT_VERSION, AgentContext, AgentFailure, AgentMessage, AttentionView, CalendarContextView,
    ContextEvidence, DataClass, InferencePolicyDecision, ModelRequest, ModelRunner, ModelStep,
    PeopleView, PromptAssembly, SessionProtection, UsageLedger, WellbeingView, WorkContextView,
    calendar_context_evidence, focus_expert_prompt, generate_with_recovery,
    personal_context_evidence, relationships_expert_prompt, validate_attention_view,
    validate_calendar_context_view, validate_people_view, validate_wellbeing_view,
    validate_work_context_view, wellbeing_expert_prompt, work_context_evidence,
};

pub const CONFIRMED_INTERACTION_VIEW_ID: &str = "relationships.confirmed_interactions";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmedInteraction {
    pub identity_handle: String,
    pub evidence_handle: String,
    pub occurred_at_unix_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmedInteractionView {
    pub schema_version: u32,
    pub view_id: String,
    pub source_handle: String,
    pub observed_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub interactions: Vec<ConfirmedInteraction>,
}

#[derive(Clone, Debug)]
pub struct RelationshipsContextViews {
    pub people: PeopleView,
    pub confirmed_interactions: Vec<ConfirmedInteractionView>,
}

#[derive(Clone, Debug)]
pub struct FocusContextViews {
    pub attention: AttentionView,
    pub calendars: Vec<CalendarContextView>,
    pub active_work: Vec<WorkContextView>,
}

#[derive(Clone, Debug)]
pub struct WellbeingContextViews {
    pub wellbeing: WellbeingView,
    pub calendars: Vec<CalendarContextView>,
}

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
    pub cancellation: crate::Cancellation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipFollowUp {
    pub identity_handle: String,
    pub reason: String,
    pub evidence_handles: Vec<String>,
    pub confidence_millis: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipsExpertResult {
    pub schema_version: u32,
    pub invocation_id: Uuid,
    pub source_handle: String,
    #[serde(default)]
    pub source_handles: Vec<String>,
    pub expires_at_unix_ms: i64,
    pub summary: String,
    pub follow_ups: Vec<RelationshipFollowUp>,
}

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
struct RelationshipsOutput {
    summary: String,
    follow_ups: Vec<RelationshipFollowUp>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FocusOutput {
    summary: String,
    recommendation: FocusRecommendation,
    rationale: String,
    evidence_handles: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WellbeingOutput {
    summary: String,
    schedule_impact: ScheduleImpact,
    rationale: String,
    evidence_handles: Vec<String>,
}

pub async fn run_relationships_expert<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PersonalExpertInvocation,
    view: PeopleView,
) -> Result<RelationshipsExpertResult, AgentFailure> {
    run_relationships_expert_with_views(
        model,
        policy,
        invocation,
        RelationshipsContextViews {
            people: view,
            confirmed_interactions: vec![],
        },
    )
    .await
}

pub async fn run_relationships_expert_with_views<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PersonalExpertInvocation,
    views: RelationshipsContextViews,
) -> Result<RelationshipsExpertResult, AgentFailure> {
    validate_people_view(&views.people, invocation.current_time_unix_ms)?;
    let mut evidence = vec![personal_context_evidence(&views.people)?];
    let mut source_handles = vec![views.people.source_handle.clone()];
    let mut expires_at_unix_ms = views.people.expires_at_unix_ms;
    let mut support_by_identity = std::collections::HashMap::<String, Vec<String>>::new();
    for view in &views.confirmed_interactions {
        validate_confirmed_interaction_view(view, &views.people, invocation.current_time_unix_ms)?;
        ensure_unique_source(&source_handles, &view.source_handle)?;
        for interaction in &view.interactions {
            support_by_identity
                .entry(interaction.identity_handle.clone())
                .or_default()
                .push(interaction.evidence_handle.clone());
        }
        source_handles.push(view.source_handle.clone());
        expires_at_unix_ms = expires_at_unix_ms.min(view.expires_at_unix_ms);
        evidence.push(confirmed_interaction_evidence(view)?);
    }
    let memory_links = relationship_memory_links(&invocation.context, &views.people)?;
    if !memory_links.is_empty() {
        const MEMORY_SOURCE: &str = "relationships:confirmed-memory";
        ensure_unique_source(&source_handles, MEMORY_SOURCE)?;
        for link in &memory_links {
            support_by_identity
                .entry(link.identity_handle.clone())
                .or_default()
                .push(link.evidence_handle.clone());
            if let Some(valid_until) = link.valid_until_unix_ms {
                expires_at_unix_ms = expires_at_unix_ms.min(valid_until);
            }
        }
        source_handles.push(MEMORY_SOURCE.into());
        evidence.push(ContextEvidence {
            source_handle: MEMORY_SOURCE.into(),
            data_class: DataClass::Personal,
            untrusted_text: serde_json::to_string(&memory_links)
                .map_err(|_| AgentFailure::InvalidInput)?,
            expires_at_unix_ms: u64::try_from(expires_at_unix_ms)
                .map_err(|_| AgentFailure::InvalidInput)?,
        });
    }
    let output: RelationshipsOutput = run_personal_model(
        model,
        policy,
        &invocation,
        evidence,
        relationships_expert_prompt(),
    )
    .await?;
    validate_summary(&output.summary)?;
    if output.follow_ups.len() > 16 {
        return Err(AgentFailure::BudgetExceeded);
    }
    for (index, follow_up) in output.follow_ups.iter().enumerate() {
        let Some(identity) = views
            .people
            .identities
            .iter()
            .find(|identity| identity.identity_handle == follow_up.identity_handle)
        else {
            return Err(AgentFailure::InvalidModelOutput);
        };
        let supporting = support_by_identity
            .get(&follow_up.identity_handle)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if follow_up.reason.trim().is_empty()
            || follow_up.reason.len() > 512
            || follow_up.confidence_millis == 0
            || follow_up.confidence_millis > 1000
            || follow_up.evidence_handles.is_empty()
            || follow_up.evidence_handles.len() > 16
            || !follow_up
                .evidence_handles
                .iter()
                .any(|handle| supporting.contains(handle))
            || follow_up.evidence_handles.iter().any(|handle| {
                !identity.evidence_handles.contains(handle) && !supporting.contains(handle)
            })
            || output.follow_ups[..index]
                .iter()
                .any(|other| other.identity_handle == follow_up.identity_handle)
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
    }
    Ok(RelationshipsExpertResult {
        schema_version: AGENT_VERSION,
        invocation_id: invocation.invocation_id,
        source_handle: views.people.source_handle,
        source_handles,
        expires_at_unix_ms,
        summary: output.summary,
        follow_ups: output.follow_ups,
    })
}

pub async fn run_focus_expert<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PersonalExpertInvocation,
    view: AttentionView,
) -> Result<FocusExpertResult, AgentFailure> {
    run_focus_expert_with_views(
        model,
        policy,
        invocation,
        FocusContextViews {
            attention: view,
            calendars: vec![],
            active_work: vec![],
        },
    )
    .await
}

pub async fn run_focus_expert_with_views<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PersonalExpertInvocation,
    views: FocusContextViews,
) -> Result<FocusExpertResult, AgentFailure> {
    validate_attention_view(&views.attention, invocation.current_time_unix_ms)?;
    let mut evidence = vec![personal_context_evidence(&views.attention)?];
    let mut available = views.attention.evidence_handles.clone();
    let mut source_handles = vec![views.attention.source_handle.clone()];
    let mut expires_at_unix_ms = views.attention.expires_at_unix_ms;
    add_schedule_views(
        &views.calendars,
        invocation.current_time_unix_ms,
        &mut evidence,
        &mut available,
        &mut source_handles,
        &mut expires_at_unix_ms,
    )?;
    for view in &views.active_work {
        validate_work_context_view(view, invocation.current_time_unix_ms)?;
        ensure_unique_source(&source_handles, &view.source_handle)?;
        extend_unique_handles(
            &mut available,
            view.items.iter().map(|item| &item.evidence_handle),
        )?;
        source_handles.push(view.source_handle.clone());
        expires_at_unix_ms = expires_at_unix_ms.min(view.expires_at_unix_ms);
        evidence.push(work_context_evidence(view)?);
    }
    let output: FocusOutput =
        run_personal_model(model, policy, &invocation, evidence, focus_expert_prompt()).await?;
    validate_judgment(
        &output.summary,
        &output.rationale,
        &output.evidence_handles,
        &available,
        matches!(output.recommendation, FocusRecommendation::NoConclusion),
    )?;
    Ok(FocusExpertResult {
        schema_version: AGENT_VERSION,
        invocation_id: invocation.invocation_id,
        source_handle: views.attention.source_handle,
        source_handles,
        expires_at_unix_ms,
        summary: output.summary,
        recommendation: output.recommendation,
        rationale: output.rationale,
        evidence_handles: output.evidence_handles,
    })
}

pub async fn run_wellbeing_expert<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PersonalExpertInvocation,
    view: WellbeingView,
) -> Result<WellbeingExpertResult, AgentFailure> {
    run_wellbeing_expert_with_views(
        model,
        policy,
        invocation,
        WellbeingContextViews {
            wellbeing: view,
            calendars: vec![],
        },
    )
    .await
}

pub async fn run_wellbeing_expert_with_views<Model: ModelRunner>(
    model: &Model,
    policy: &InferencePolicyDecision,
    invocation: PersonalExpertInvocation,
    views: WellbeingContextViews,
) -> Result<WellbeingExpertResult, AgentFailure> {
    validate_wellbeing_view(&views.wellbeing, invocation.current_time_unix_ms)?;
    let mut evidence = vec![personal_context_evidence(&views.wellbeing)?];
    let mut available = views.wellbeing.evidence_handles.clone();
    let mut source_handles = vec![views.wellbeing.source_handle.clone()];
    let mut expires_at_unix_ms = views.wellbeing.expires_at_unix_ms;
    add_schedule_views(
        &views.calendars,
        invocation.current_time_unix_ms,
        &mut evidence,
        &mut available,
        &mut source_handles,
        &mut expires_at_unix_ms,
    )?;
    let output: WellbeingOutput = run_personal_model(
        model,
        policy,
        &invocation,
        evidence,
        wellbeing_expert_prompt(),
    )
    .await?;
    validate_judgment(
        &output.summary,
        &output.rationale,
        &output.evidence_handles,
        &available,
        matches!(output.schedule_impact, ScheduleImpact::NoConclusion),
    )?;
    Ok(WellbeingExpertResult {
        schema_version: AGENT_VERSION,
        invocation_id: invocation.invocation_id,
        source_handle: views.wellbeing.source_handle,
        source_handles,
        expires_at_unix_ms,
        summary: output.summary,
        schedule_impact: output.schedule_impact,
        rationale: output.rationale,
        evidence_handles: output.evidence_handles,
    })
}

#[derive(Serialize)]
struct RelationshipMemoryLink {
    identity_handle: String,
    evidence_handle: String,
    target_id: Uuid,
    revision: u64,
    valid_until_unix_ms: Option<i64>,
}

fn validate_confirmed_interaction_view(
    view: &ConfirmedInteractionView,
    people: &PeopleView,
    now_unix_ms: i64,
) -> Result<(), AgentFailure> {
    if view.schema_version != AGENT_VERSION
        || view.view_id != CONFIRMED_INTERACTION_VIEW_ID
        || !valid_handle(&view.source_handle)
        || view.observed_at_unix_ms > now_unix_ms
        || view.expires_at_unix_ms <= now_unix_ms
        || view.expires_at_unix_ms <= view.observed_at_unix_ms
        || view.expires_at_unix_ms - view.observed_at_unix_ms > 300_000
        || view.interactions.len() > 64
        || serde_json::to_vec(view)
            .map_err(|_| AgentFailure::InvalidInput)?
            .len()
            > crate::MAX_PERSONAL_CONTEXT_BYTES
    {
        return Err(AgentFailure::InvalidInput);
    }
    for (index, interaction) in view.interactions.iter().enumerate() {
        if !valid_handle(&interaction.identity_handle)
            || !valid_handle(&interaction.evidence_handle)
            || interaction.occurred_at_unix_ms < 0
            || interaction.occurred_at_unix_ms > view.observed_at_unix_ms
            || !people
                .identities
                .iter()
                .any(|identity| identity.identity_handle == interaction.identity_handle)
            || view.interactions[..index]
                .iter()
                .any(|other| other.evidence_handle == interaction.evidence_handle)
        {
            return Err(AgentFailure::InvalidInput);
        }
    }
    Ok(())
}

fn confirmed_interaction_evidence(
    view: &ConfirmedInteractionView,
) -> Result<ContextEvidence, AgentFailure> {
    Ok(ContextEvidence {
        source_handle: view.source_handle.clone(),
        data_class: DataClass::Personal,
        untrusted_text: serde_json::to_string(&view.interactions)
            .map_err(|_| AgentFailure::InvalidInput)?,
        expires_at_unix_ms: u64::try_from(view.expires_at_unix_ms)
            .map_err(|_| AgentFailure::InvalidInput)?,
    })
}

fn relationship_memory_links(
    context: &AgentContext,
    people: &PeopleView,
) -> Result<Vec<RelationshipMemoryLink>, AgentFailure> {
    let mut links = vec![];
    for memory in &context.memories {
        let target = memory.target_id.to_string();
        let Some(identity) = people.identities.iter().find(|identity| {
            identity.identity_handle == target
                || identity.identity_handle == format!("person:{target}")
        }) else {
            continue;
        };
        let evidence_handle = format!("memory:{}:{}", memory.target_id, memory.revision);
        if !valid_handle(&evidence_handle)
            || links
                .iter()
                .any(|link: &RelationshipMemoryLink| link.evidence_handle == evidence_handle)
        {
            return Err(AgentFailure::InvalidInput);
        }
        links.push(RelationshipMemoryLink {
            identity_handle: identity.identity_handle.clone(),
            evidence_handle,
            target_id: memory.target_id,
            revision: memory.revision,
            valid_until_unix_ms: memory.valid_until_unix_ms,
        });
    }
    Ok(links)
}

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
