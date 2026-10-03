use floe_agent_contract::UserInteractionStatus;
pub(super) fn interaction_status(state: &crate::InteractionState) -> UserInteractionStatus {
    match state {
        crate::InteractionState::Pending => UserInteractionStatus::Pending,
        crate::InteractionState::Resolving { .. } => UserInteractionStatus::Resolving,
        crate::InteractionState::Resolved { .. } => UserInteractionStatus::Resolved,
        crate::InteractionState::Denied { .. } => UserInteractionStatus::Denied,
        crate::InteractionState::Cancelled { .. } => UserInteractionStatus::Cancelled,
        crate::InteractionState::Superseded { .. } => UserInteractionStatus::Superseded,
        crate::InteractionState::Expired => UserInteractionStatus::Expired,
    }
}
