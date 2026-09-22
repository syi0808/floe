use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AgentSessionDto, AgentVaultFailureDto, AgentVaultStateDto};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationSessionResultDto {
    pub operation_id: Uuid,
    pub done: bool,
    pub state: Option<AgentVaultStateDto>,
    pub session: Option<AgentSessionDto>,
    pub failure: Option<AgentVaultFailureDto>,
}
