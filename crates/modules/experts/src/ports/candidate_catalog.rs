use floe_agent_contract::{AgentFailure, BoxFuture, ExecutionScope, OwnerActor, PackageRef};
use floe_context_contract::{SourceAuthority, SourceSelectionReference};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateAvailability {
    Available,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub candidate_id: String,
    pub label: String,
    pub detail: String,
    pub availability: CandidateAvailability,
    pub reference: SourceSelectionReference,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateSourceExpectation {
    pub reference: SourceSelectionReference,
    pub source_revision: u64,
    pub source_authority: SourceAuthority,
}

#[derive(Clone, Debug)]
pub struct CandidateQuery {
    pub actor: OwnerActor,
    pub package_ref: PackageRef,
    pub definition_revision: u64,
    pub requirement_key: String,
    pub current_candidate_refs: Vec<SourceSelectionReference>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateSnapshot {
    pub revision: u64,
    pub digest: [u8; 32],
    pub candidates: Vec<Candidate>,
    pub source_expectations: Vec<CandidateSourceExpectation>,
}

pub trait CandidateCatalog: Send + Sync {
    fn inspect<'a>(
        &'a self,
        query: CandidateQuery,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<CandidateSnapshot, AgentFailure>>;
}
