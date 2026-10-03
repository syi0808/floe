//! Safe owner failure policy. Recovery affordances never reissue model work.
use floe_kernel::{AgentFailure, AgentFailureCategory, AgentFailureDomain, AgentFailureSafeAction};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationRecovery { None, Reobserve, Reconcile, Unlock, Reopen, NewReview }
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ConversationFailure {
    pub domain: AgentFailureDomain,
    pub category: AgentFailureCategory,
    pub reason: AgentFailure,
    pub incident_id: Uuid,
    pub correlation_id: Uuid,
    pub reload_required: bool,
    pub seal_session: bool,
    pub recovery: ConversationRecovery,
    pub safe_actions: Vec<AgentFailureSafeAction>,
}

pub fn project_conversation_failure(reason: AgentFailure, correlation_id: Uuid) -> ConversationFailure {
    use AgentFailure as F;
    let category = match reason {
        F::PolicyDenied | F::CapabilityDenied | F::ConsentRequired => AgentFailureCategory::Security,
        F::InvalidModelOutput | F::LocalModelInvalidOutput | F::ServerModelInvalidOutput | F::VaultUnavailable => AgentFailureCategory::Integrity,
        F::UnsupportedVersion | F::InvalidInput | F::NotFound | F::Conflict | F::CredentialExpired
            | F::AccessReviewRequired | F::BudgetExceeded | F::Stalled | F::StaleContext => AgentFailureCategory::UserConfiguration,
        F::StorageUnavailable | F::ModelUnavailable | F::LocalModelUnavailable | F::ServerModelUnavailable
            | F::ServerModelTimeout | F::ServerModelRequestRejected | F::QuotaExceeded | F::CapabilityUnavailable
            | F::Cancelled | F::DeadlineExceeded | F::Interrupted => AgentFailureCategory::Transient,
    };
    let (reload_required, seal_session, recovery, safe_actions) = match reason {
        F::Conflict | F::StaleContext | F::NotFound => (true, false, ConversationRecovery::Reobserve, vec![AgentFailureSafeAction::RefreshSession]),
        F::VaultUnavailable => (true, true, ConversationRecovery::Reopen, vec![AgentFailureSafeAction::ReopenVault]),
        _ => (false, false, ConversationRecovery::None, vec![]),
    };
    ConversationFailure {
        domain: AgentFailureDomain::Turn, category, reason,
        incident_id: Uuid::new_v5(&correlation_id, format!("floe.conversation.failure:{reason:?}").as_bytes()),
        correlation_id, reload_required, seal_session, recovery, safe_actions,
    }
}
