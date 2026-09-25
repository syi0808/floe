use floe_agent_contract::{Artifact, ArtifactPart, DependencyCoverage, PackageRef};
use floe_context_contract::DataClass;
use floe_kernel::{AgentFailure, PersonId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const EXPERT_CALENDAR_PROPOSAL_MEDIA_TYPE: &str =
    "application/vnd.floe.actions.calendar-proposal+json;version=1";

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
        let artifact = Artifact {
            artifact_id: Uuid::new_v4(),
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
