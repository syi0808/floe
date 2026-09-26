use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AgentVaultFailureDto, AgentVaultStateDto, RegistryOverviewDto};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertOperationResultDto {
    pub operation_id: Uuid,
    pub done: bool,
    pub state: Option<AgentVaultStateDto>,
    pub registry: Option<RegistryOverviewDto>,
    pub candidates: Option<ExpertCandidateCatalogDto>,
    pub failure: Option<AgentVaultFailureDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertBindingSelectionDto {
    pub assignment_id: Uuid,
    pub package_id: String,
    pub package_version: String,
    pub definition_revision: u64,
    pub requirement_key: String,
    pub expected_binding_revision: u64,
    pub candidate_ids: Vec<String>,
}

impl ExpertBindingSelectionDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        let identifier = |value: &str| {
            !value.is_empty()
                && value.len() <= 128
                && value.trim() == value
                && !value.chars().any(char::is_control)
        };
        if self.assignment_id.is_nil()
            || !identifier(&self.package_id)
            || !identifier(&self.package_version)
            || !identifier(&self.requirement_key)
            || self.definition_revision == 0
            || self.expected_binding_revision == 0
            || self.candidate_ids.len() > 16
            || self.candidate_ids.iter().any(|id| {
                id.len() != 64
                    || !id
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            })
        {
            return Err("command.binding");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertSourceCandidateDto {
    pub candidate_id: String,
    pub title: String,
    pub detail: String,
    pub availability: String,
    pub selected: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertCandidateCatalogDto {
    pub assignment_id: Uuid,
    pub requirement_key: String,
    pub binding_revision: u64,
    pub candidates: Vec<ExpertSourceCandidateDto>,
}
