use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::OwnerFailureDto;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeReadinessStateDto {
    Ready,
    PreparationRequired,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeReadinessDto {
    pub state: RuntimeReadinessStateDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<OwnerFailureDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimePreparationResultDto {
    pub operation_id: Uuid,
    pub done: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<floe_kernel::AgentFailure>,
}
