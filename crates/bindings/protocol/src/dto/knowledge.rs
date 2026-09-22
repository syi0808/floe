use super::{
    AgentMemoryOverviewDto, AgentMemoryReviewOverviewDto, AgentVaultFailureDto, AgentVaultStateDto,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeOperationResultDto {
    pub operation_id: Uuid,
    pub done: bool,
    pub state: Option<AgentVaultStateDto>,
    pub memory: Option<AgentMemoryOverviewDto>,
    pub memory_review: Option<AgentMemoryReviewOverviewDto>,
    pub failure: Option<AgentVaultFailureDto>,
}
