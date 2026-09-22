use crate::local_operations::{LocalOperationIntent, LocalOperationOwner};
use crate::{
    AgentFailure, AppComposition, CallerContext, MemoryOverviewSnapshot, MemoryReviewDecision,
    MemoryReviewResult, ServiceError, VaultState,
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeInspection {
    Memory,
    Review,
}

#[derive(Clone, Debug)]
pub struct KnowledgeOperationResult {
    pub operation_id: Uuid,
    pub stage: String,
    pub done: bool,
    pub state: Option<VaultState>,
    pub memory: Option<MemoryOverviewSnapshot>,
    pub memory_review: Option<MemoryReviewResult>,
    pub failure: Option<AgentFailure>,
}

pub trait KnowledgeCommands {
    fn decide_memory(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        decision: MemoryReviewDecision,
    ) -> Result<KnowledgeOperationResult, ServiceError>;
}

pub trait KnowledgeQueries {
    fn inspect_knowledge(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        inspection: KnowledgeInspection,
    ) -> Result<KnowledgeOperationResult, ServiceError>;
    fn read_knowledge_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<KnowledgeOperationResult, ServiceError>;
}

impl KnowledgeCommands for AppComposition {
    fn decide_memory(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        decision: MemoryReviewDecision,
    ) -> Result<KnowledgeOperationResult, ServiceError> {
        if decision.candidate_id.is_nil() {
            return Err(ServiceError::InvalidInput);
        }
        self.knowledge_operation(
            caller,
            operation_id,
            Some(LocalOperationIntent::MemoryDecision(decision)),
            false,
        )
    }
}

impl KnowledgeQueries for AppComposition {
    fn inspect_knowledge(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        inspection: KnowledgeInspection,
    ) -> Result<KnowledgeOperationResult, ServiceError> {
        self.knowledge_operation(
            caller,
            operation_id,
            Some(LocalOperationIntent::KnowledgeInspection(inspection)),
            false,
        )
    }
    fn read_knowledge_result(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        release: bool,
    ) -> Result<KnowledgeOperationResult, ServiceError> {
        self.knowledge_operation(caller, operation_id, None, release)
    }
}

impl AppComposition {
    fn knowledge_operation(
        &self,
        caller: &CallerContext,
        operation_id: Uuid,
        intent: Option<LocalOperationIntent>,
        release: bool,
    ) -> Result<KnowledgeOperationResult, ServiceError> {
        let result = self
            .agent_vault
            .local_request(
                caller,
                operation_id,
                intent,
                LocalOperationOwner::Knowledge,
                release,
            )
            .map_err(crate::composition::service_failure)?;
        Ok(KnowledgeOperationResult {
            operation_id: result.request_id,
            stage: result.stage,
            done: result.done,
            state: result.state,
            memory: result.memory,
            memory_review: result.memory_review,
            failure: result.failure,
        })
    }
}
