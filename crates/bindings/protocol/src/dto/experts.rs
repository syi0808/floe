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
    pub failure: Option<AgentVaultFailureDto>,
}
