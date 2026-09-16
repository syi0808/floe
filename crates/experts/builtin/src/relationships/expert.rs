//! Relationship follow-ups from confirmed interactions.

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
struct RelationshipsOutput {
    summary: String,
    follow_ups: Vec<RelationshipFollowUp>,
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

struct RelationshipMemoryLink {
    identity_handle: String,
    evidence_handle: String,
    target_id: Uuid,
    revision: u64,
    valid_until_unix_ms: Option<i64>,
}

pub fn validate_confirmed_interaction_view(
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
