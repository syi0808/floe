use floe_execution::BoxFuture;
use floe_kernel::AgentFailure;
use floe_context_contract::GrantSourceBinding;
use crate::{ConnectionReview, ReviewRef, GrantSnapshot, GrantCommit, GrantCommitReceipt,
    GrantReceiptQuery, GrantOperationReceipt, GrantAbort, GrantAbortOutcome};
pub trait GrantRepository: Send + Sync {
    fn find_review<'a>(&'a self, person_id: floe_kernel::PersonId, command_id: uuid::Uuid, intent_digest: [u8; 32]) -> BoxFuture<'a, Result<Option<ConnectionReview>, AgentFailure>>;
    fn read_review<'a>(&'a self, reference: ReviewRef) -> BoxFuture<'a, Result<ConnectionReview, AgentFailure>>;
    fn store_review<'a>(&'a self, review: ConnectionReview) -> BoxFuture<'a, Result<ReviewRef, AgentFailure>>;
    fn snapshot<'a>(&'a self, source: GrantSourceBinding) -> BoxFuture<'a, Result<GrantSnapshot, AgentFailure>>;
    fn commit<'a>(&'a self, command: GrantCommit) -> BoxFuture<'a, Result<GrantCommitReceipt, AgentFailure>>;
    fn receipt<'a>(&'a self, query: GrantReceiptQuery) -> BoxFuture<'a, Result<Option<GrantOperationReceipt>, AgentFailure>>;
    fn abort<'a>(&'a self, command: GrantAbort) -> BoxFuture<'a, Result<GrantAbortOutcome, AgentFailure>>;
}
