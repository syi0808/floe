use floe_agent_contract::{Artifact, ArtifactPart, DependencyCoverage, PackageRef};
use floe_context_contract::DataClass;
use floe_kernel::{AgentFailure, PersonId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const EXPERT_CALENDAR_PROPOSAL_MEDIA_TYPE: &str =
    "application/vnd.floe.actions.calendar-proposal+json;version=1";

#[derive(Clone, Debug)]
pub struct ExpertProposalContext {
    pub person_id: PersonId,
    pub instance_id: Uuid,
    pub assignment_id: Uuid,
    pub package: PackageRef,
    pub task_id: floe_agent_contract::TaskId,
    pub invocation_id: Uuid,
    pub next_state_revision: u64,
    pub data_class: DataClass,
    pub captured_dependencies: Vec<floe_context_contract::ContextDependency>,
    pub admitted_selections: Vec<floe_context_contract::SourceSelectionReference>,
    pub now_unix_ms: i64,
}

/// Seals a proposal artifact from captured Task evidence. This is neither a
/// current grant lookup nor approval to dispatch an external effect.
pub fn seal_expert_calendar_proposal(
    context: ExpertProposalContext,
    draft: ExpertCalendarProposalDraft,
) -> Result<Artifact, AgentFailure> {
    draft.validate()?;
    let now = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(context.now_unix_ms)
        .ok_or(AgentFailure::InvalidInput)?;
    if !context.person_id.is_valid()
        || context.instance_id.is_nil()
        || context.assignment_id.is_nil()
        || !context.task_id.is_valid()
        || context.invocation_id.is_nil()
        || context.next_state_revision == 0
        || context.package.kind != floe_agent_contract::PackageKind::Expert
        || context.captured_dependencies.is_empty()
        || context.captured_dependencies.len() > floe_context_contract::MAX_CONTEXT_DEPENDENCIES
        || context.admitted_selections.is_empty()
        || context.admitted_selections.len() > floe_context_contract::MAX_CONTEXT_DEPENDENCIES
        || !matches!(
            context.data_class,
            DataClass::Personal | DataClass::Synthetic
        )
    {
        return Err(AgentFailure::PolicyDenied);
    }
    for dependency in &context.captured_dependencies {
        dependency
            .validate()
            .map_err(|_| AgentFailure::PolicyDenied)?;
        if dependency.person_id() != context.person_id {
            return Err(AgentFailure::PolicyDenied);
        }
    }
    for selection in &context.admitted_selections {
        selection
            .validate()
            .map_err(|_| AgentFailure::PolicyDenied)?;
    }
    let contributors: Vec<_> = context
        .captured_dependencies
        .iter()
        .filter(|dependency| {
            dependency.source().person_id() == context.person_id
                && dependency.consumer().identifier() == context.package.id
                && dependency.operation() == floe_context_contract::GrantOperation::Read
                && dependency.purpose() == floe_context_contract::GrantPurpose::Assistant
                && matches!(
                    dependency.source().connector().as_str(),
                    "calendar.event_kit"
                        | "calendar.android"
                        | "calendar.google"
                        | "calendar.microsoft"
                )
                && !dependency.resources().is_empty()
                && dependency.observed_at() <= now
                && dependency.expires_at() > now
        })
        .collect();
    let [contributor] = contributors.as_slice() else {
        return Err(AgentFailure::PolicyDenied);
    };
    if !context.admitted_selections.iter().any(|selected| {
        selected.capability_id == "calendar.timeline"
            && selected.connector_id == *contributor.source().connector()
            && selected.connection_id == contributor.source().connection_id()
            && selected.execution_owner_id == *contributor.source().execution_owner()
            && contributor.resources().contains(&selected.resource)
    }) {
        return Err(AgentFailure::PolicyDenied);
    }
    let contributor = (*contributor).clone();
    let coverage = DependencyCoverage::dependent(contributor.clone())
        .map_err(|_| AgentFailure::PolicyDenied)?;
    ExpertCalendarProposal {
        schema_version: 1,
        instance_id: context.instance_id,
        person_id: context.person_id,
        assignment_id: context.assignment_id,
        package: context.package,
        task_id: context.task_id.as_uuid(),
        invocation_id: context.invocation_id,
        state_revision: context.next_state_revision,
        evidence_id: contributor.observation_id(),
        data_class: context.data_class,
        expires_at_unix_ms: u64::try_from(contributor.expires_at().timestamp_millis())
            .map_err(|_| AgentFailure::StaleContext)?,
        draft,
    }
    .artifact(coverage)
}

/// Historical Task proof is independent of today's grant and write policy.
/// Storage calls this pure owner validator after proving the exact Task bytes.
pub fn validate_expert_action_evidence(
    record: &crate::ActionRecord,
    evidence: &crate::ExpertProposalEvidence,
) -> Result<(), AgentFailure> {
    record.validate()?;
    evidence.proposal.validate()?;
    let crate::ActionOrigin::Expert {
        task_id,
        invocation_id,
        package,
        installation_id,
        assignment_id,
        definition_revision,
        evidence_ref,
        artifact_id,
    } = &record.origin
    else {
        return Err(AgentFailure::PolicyDenied);
    };
    let proposal = &evidence.proposal;
    let dependency = record
        .dependency
        .as_ref()
        .ok_or(AgentFailure::PolicyDenied)?;
    evidence
        .coverage
        .validate()
        .map_err(|_| AgentFailure::PolicyDenied)?;
    let DependencyCoverage::Dependent { dependencies } = &evidence.coverage else {
        return Err(AgentFailure::PolicyDenied);
    };
    if !dependencies.contains(dependency)
        || dependencies.iter().any(|entry| {
            entry.person_id() != record.person_id
                || entry.observed_at() > record.created_at
                || record.expires_at > entry.expires_at()
        })
    {
        return Err(AgentFailure::PolicyDenied);
    }
    if evidence_ref != &evidence.receipt
        || artifact_id != &evidence.artifact_id
        || task_id != &proposal.task_id
        || invocation_id != &evidence.invocation_id
        || proposal.invocation_id != evidence.invocation_id
        || package != &proposal.package
        || installation_id != &evidence.installation_id
        || assignment_id != &evidence.assignment_id
        || proposal.assignment_id != evidence.assignment_id
        || definition_revision != &evidence.definition_revision
        || record.person_id != proposal.person_id
        || dependency != &evidence.dependency
        || dependency.observation_id() != proposal.evidence_id
        || proposal.data_class != DataClass::Personal
        || dependency.source().connector().as_str() != "calendar.event_kit"
        || dependency.consumer().identifier() != proposal.package.id
        || dependency.operation() != floe_context_contract::GrantOperation::Read
        || dependency.purpose() != floe_context_contract::GrantPurpose::Assistant
        || record.created_at < dependency.observed_at()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    let crate::CalendarEffect::Create {
        destination,
        title,
        schedule,
    } = &record.effect
    else {
        return Err(AgentFailure::PolicyDenied);
    };
    let timestamp = |value: u64| {
        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
            i64::try_from(value).map_err(|_| AgentFailure::InvalidInput)?,
        )
        .ok_or(AgentFailure::InvalidInput)
    };
    if title != "Focus time"
        || schedule.starts_at != timestamp(proposal.draft.starts_at_unix_ms)?
        || schedule.ends_at != timestamp(proposal.draft.ends_at_unix_ms)?
        || record.expires_at > timestamp(proposal.expires_at_unix_ms)?
        || destination.connection_id != dependency.source().connection_id()
        || !dependency
            .source_resources()
            .iter()
            .any(|resource| resource.as_str() == destination.calendar_id)
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertCalendarProposalDraft {
    pub starts_at_unix_ms: u64,
    pub ends_at_unix_ms: u64,
}

impl ExpertCalendarProposalDraft {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        if self.ends_at_unix_ms <= self.starts_at_unix_ms
            || self.ends_at_unix_ms - self.starts_at_unix_ms > 86_400_000
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertCalendarProposal {
    pub schema_version: u32,
    pub instance_id: Uuid,
    pub person_id: PersonId,
    pub assignment_id: Uuid,
    pub package: PackageRef,
    pub task_id: Uuid,
    pub invocation_id: Uuid,
    pub state_revision: u64,
    pub evidence_id: Uuid,
    pub data_class: DataClass,
    pub expires_at_unix_ms: u64,
    pub draft: ExpertCalendarProposalDraft,
}

impl ExpertCalendarProposal {
    pub fn validate(&self) -> Result<(), AgentFailure> {
        self.draft.validate()?;
        if self.schema_version != 1
            || self.instance_id.is_nil()
            || self.person_id.0.is_nil()
            || self.assignment_id.is_nil()
            || self.task_id.is_nil()
            || self.invocation_id.is_nil()
            || self.evidence_id.is_nil()
            || self.state_revision == 0
            || self.package.id.trim().is_empty()
            || self.package.version.trim().is_empty()
            || self.expires_at_unix_ms == 0
        {
            return Err(AgentFailure::InvalidModelOutput);
        }
        Ok(())
    }

    pub fn artifact(&self, coverage: DependencyCoverage) -> Result<Artifact, AgentFailure> {
        self.validate()?;
        if !matches!(coverage, DependencyCoverage::Dependent { .. }) {
            return Err(AgentFailure::InvalidModelOutput);
        }
        let digest = crate::action_digest(
            b"floe.actions.proposal-artifact.v1\0",
            &(self.person_id, self.task_id, self.invocation_id),
        )?;
        let mut bytes = [0; 16];
        bytes.copy_from_slice(&digest[..16]);
        bytes[6] = (bytes[6] & 15) | 0x50;
        bytes[8] = (bytes[8] & 63) | 0x80;
        let artifact = Artifact {
            artifact_id: Uuid::from_bytes(bytes),
            name: "Calendar proposal".into(),
            parts: vec![ArtifactPart::Data {
                media_type: EXPERT_CALENDAR_PROPOSAL_MEDIA_TYPE.into(),
                data: serde_json::to_string(self).map_err(|_| AgentFailure::InvalidModelOutput)?,
            }],
            coverage,
        };
        artifact.validate(floe_agent_contract::MAX_OUTPUT_BYTES)?;
        Ok(artifact)
    }
}
