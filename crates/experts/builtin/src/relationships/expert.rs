//! Relationship follow-ups from confirmed interactions.

use std::collections::HashMap;

use floe_agent_contract::{AGENT_VERSION, AgentContext, AgentFailure, Artifact};
use floe_context_contract::{
    ConfirmedInteractionView, PeopleView, validate_confirmed_interaction_view, validate_people_view,
};
use floe_experts::{ExpertProgram, ExpertProgramRequest, ExpertProgramSpec, ExpertToolObservation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::prompts::relationships_expert_prompt;

const OUTPUT_CONTRACT: &str = r#"{
  "type":"object",
  "additionalProperties":false,
  "properties":{
    "summary":{"type":"string","minLength":1,"maxLength":2048},
    "follow_ups":{
      "type":"array","maxItems":16,
      "items":{
        "type":"object","additionalProperties":false,
        "properties":{
          "identity_handle":{"type":"string"},
          "reason":{"type":"string","minLength":1,"maxLength":512},
          "evidence_handles":{
            "type":"array","minItems":1,"maxItems":16,
            "items":{"type":"string"}
          },
          "confidence_millis":{"type":"integer","minimum":1,"maximum":1000}
        },
        "required":["identity_handle","reason","evidence_handles","confidence_millis"]
      }
    }
  },
  "required":["summary","follow_ups"]
}"#;

const REQUIRED_EVIDENCE_INSTRUCTION: &str = "\n\nRead every required evidence tool before forming a final judgment. Return only one JSON object that conforms to this package's supplied output contract.";
const ROOT_MEMORY_LINK_INSTRUCTION: &str = "\nConfirmed Memory evidence comes from the admitted root context. Match a memory to a People identity only when target_id equals identity_handle or identity_handle is person:{target_id}; cite it as memory:{target_id}:{revision}.";
const UNAVAILABLE_SUMMARY: &str =
    "Contacts are temporarily unavailable, so there is no relationship assessment.";

#[derive(Clone, Copy, Debug, Default)]
pub struct RelationshipsProgram;

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

#[derive(Serialize)]
struct RelationshipMemoryLink {
    identity_handle: String,
    evidence_handle: String,
    target_id: Uuid,
    revision: u64,
    valid_until_unix_ms: Option<i64>,
}

impl ExpertProgram for RelationshipsProgram {
    fn specification(
        &self,
        request: &ExpertProgramRequest,
    ) -> Result<ExpertProgramSpec, AgentFailure> {
        let mut prompt = relationships_expert_prompt();
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

        let contacts_key = crate::BuiltinContextSource::Contacts.source_id();
        let people =
            match crate::program_support::read_one::<PeopleView>(observations, contacts_key)? {
                Some(view) => view,
                None if crate::program_support::was_unavailable(observations, contacts_key) => {
                    return crate::program_support::unavailable(
                        request,
                        observations,
                        contacts_key,
                        crate::BuiltinExpertKind::Relationships.result_artifact_name(),
                        super::RESULT_MEDIA_TYPE,
                        UNAVAILABLE_SUMMARY,
                    );
                }
                None => return Err(AgentFailure::InvalidModelOutput),
            };
        validate_people_view(&people, request.now_unix_ms)?;

        let interactions_key = crate::BuiltinContextSource::ConfirmedInteractions.source_id();
        let interactions = crate::program_support::read_one::<Vec<ConfirmedInteractionView>>(
            observations,
            interactions_key,
        )?
        .unwrap_or_default();

        let mut source_handles = vec![people.source_handle.clone()];
        let mut expires_at_unix_ms = people.expires_at_unix_ms;
        let mut support_by_identity = HashMap::<String, Vec<String>>::new();
        for view in &interactions {
            validate_confirmed_interaction_view(view, &people, request.now_unix_ms)?;
            ensure_unique_source(&source_handles, &view.source_handle)?;
            for interaction in &view.interactions {
                support_by_identity
                    .entry(interaction.identity_handle.clone())
                    .or_default()
                    .push(interaction.evidence_handle.clone());
            }
            source_handles.push(view.source_handle.clone());
            expires_at_unix_ms = expires_at_unix_ms.min(view.expires_at_unix_ms);
        }

        let root_context = &request.context;
        let memory_links = relationship_memory_links(root_context, &people)?;
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
        }

        if text.len() > request.request.execution_context.max_output_bytes.min(8192) {
            return Err(AgentFailure::BudgetExceeded);
        }
        let output: RelationshipsOutput =
            serde_json::from_str(text).map_err(|_| AgentFailure::InvalidModelOutput)?;
        validate_summary(&output.summary)?;
        if output.follow_ups.len() > 16 {
            return Err(AgentFailure::BudgetExceeded);
        }
        for (index, follow_up) in output.follow_ups.iter().enumerate() {
            let Some(identity) = people
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

        let result = RelationshipsExpertResult {
            schema_version: AGENT_VERSION,
            invocation_id: request.request.invocation_key.as_uuid(),
            source_handle: people.source_handle,
            source_handles,
            expires_at_unix_ms,
            summary: output.summary,
            follow_ups: output.follow_ups,
        };
        crate::program_support::result(
            request,
            observations,
            crate::BuiltinExpertKind::Relationships.result_artifact_name(),
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
    role.content.push_str(ROOT_MEMORY_LINK_INSTRUCTION);
    prompt.validate()
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

fn ensure_unique_source(source_handles: &[String], source: &str) -> Result<(), AgentFailure> {
    if source_handles.iter().any(|value| value == source) {
        Err(AgentFailure::InvalidInput)
    } else {
        Ok(())
    }
}

fn valid_handle(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128
}

fn validate_summary(summary: &str) -> Result<(), AgentFailure> {
    if !summary.trim().is_empty() && summary.len() <= 2048 {
        Ok(())
    } else {
        Err(AgentFailure::InvalidModelOutput)
    }
}
