//! Relationship follow-ups from confirmed interactions.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use floe_agent_contract::{AgentFailure, DataClass};
use floe_kernel::AGENT_VERSION;
use floe_context::{AgentContext, CONFIRMED_INTERACTION_VIEW_ID, ConfirmedInteraction, ConfirmedInteractionView, validate_confirmed_interaction_view, AttentionView, CalendarContextView, ContextEvidence, InferencePolicyDecision, PeopleView, personal_context_evidence, validate_people_view};
use floe_conversation::ModelRunner;

use crate::prompts::{relationships_expert_prompt};
use crate::shared::{validate_summary, PersonalExpertInvocation, add_schedule_views, ensure_unique_source, extend_unique_handles, run_personal_model, valid_handle, validate_judgment};

#[derive(Clone, Debug)]
pub struct RelationshipsContextViews {
    pub people: PeopleView,
    pub confirmed_interactions: Vec<ConfirmedInteractionView>,
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Serialize)]
struct RelationshipMemoryLink {
    identity_handle: String,
    evidence_handle: String,
    target_id: Uuid,
    revision: u64,
    valid_until_unix_ms: Option<i64>,
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
