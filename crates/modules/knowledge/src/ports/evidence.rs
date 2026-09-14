use std::future::Future;

use floe_kernel::AgentFailure;
use uuid::Uuid;

use crate::LearningEvidenceSnapshot;

pub trait EvidenceReader: Send + Sync {
    fn read_learning_evidence(
        &self,
        person_id: floe_kernel::PersonId,
        session_id: Uuid,
        turn_ids: &[Uuid],
    ) -> impl Future<Output = Result<LearningEvidenceSnapshot, AgentFailure>> + Send;
}
