use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AgentVaultFailureDto, AgentVaultStateDto};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VaultLifecycleResultDto {
    pub operation_id: Uuid,
    pub done: bool,
    pub state: Option<AgentVaultStateDto>,
    pub failure: Option<AgentVaultFailureDto>,
}
