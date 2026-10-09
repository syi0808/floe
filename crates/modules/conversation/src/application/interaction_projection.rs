//! Interaction display and action policy, owned by Conversation.
use crate::{
    ConversationInteraction, InteractionRequirement, InteractionState, NavigationDestination,
    ReviewedTarget,
};
use floe_agent_contract::UserInteractionKind;
use floe_execution::ExecutionScope;
use floe_kernel::{AgentFailure, OwnerActor, RunId};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionStatus {
    Pending,
    Resolving,
    Resolved,
    Denied,
    Cancelled,
    Superseded,
    Expired,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionAction {
    Allow,
    Deny,
    Dismiss,
    Refresh,
    OpenConnection,
    ReviewSource,
    RequestPermission,
    OpenExpertSettings,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InteractionTarget {
    SourceReview {
        review: floe_connections::ObserveReview,
    },
    NavigationOnly {
        destination: NavigationDestination,
        source_label: String,
        source_ref: Option<Uuid>,
    },
    ExpertBinding {
        review: floe_experts::BindingReview,
    },
    OperationApproval {
        operation: floe_calendar_operations::ActionSnapshot,
    },
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct InteractionSnapshot {
    pub interaction_id: Uuid,
    pub session_id: Uuid,
    pub origin_run_id: RunId,
    pub interaction_kind: UserInteractionKind,
    pub state: InteractionStatus,
    pub revision: u64,
    pub target_digest: [u8; 32],
    pub created_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub requirement: InteractionRequirement,
    pub replacement_id: Option<Uuid>,
    pub target: InteractionTarget,
    pub allowed_actions: Vec<InteractionAction>,
}

pub(super) async fn project_interaction(
    connections: &floe_connections::ConnectionsService,
    experts: &dyn floe_experts::ExpertsOwner,
    operations: &floe_calendar_operations::CalendarOperationsService,
    actor: &OwnerActor,
    record: &ConversationInteraction,
    now_unix_ms: i64,
    scope: &ExecutionScope,
) -> Result<InteractionSnapshot, AgentFailure> {
    record.validate()?;
    if record.person_id != actor.person_id {
        return Err(AgentFailure::PolicyDenied);
    }
    let mut state = match record.state {
        InteractionState::Pending => InteractionStatus::Pending,
        InteractionState::Resolving { .. } => InteractionStatus::Resolving,
        InteractionState::Resolved { .. } => InteractionStatus::Resolved,
        InteractionState::Denied { .. } => InteractionStatus::Denied,
        InteractionState::Cancelled { .. } => InteractionStatus::Cancelled,
        InteractionState::Superseded { .. } => InteractionStatus::Superseded,
        InteractionState::Expired => InteractionStatus::Expired,
    };
    if record.projects_expired_at(now_unix_ms) {
        state = InteractionStatus::Expired;
    }
    let target = match &record.target {
        ReviewedTarget::SourceReview(reference) => InteractionTarget::SourceReview {
            review: connections
                .inspect_observe_review(actor, reference.clone(), scope)
                .await?,
        },
        ReviewedTarget::NavigationOnly(target) => InteractionTarget::NavigationOnly {
            destination: target.destination,
            source_label: source_label(&target.source_id).into(),
            source_ref: target
                .connection_id
                .as_deref()
                .map(|id| {
                    let connection = floe_context_contract::ConnectionId::try_new(id)
                        .map_err(|_| AgentFailure::StorageUnavailable)?;
                    Ok::<_, AgentFailure>(floe_context_contract::source_display_ref(
                        actor.person_id,
                        &connection,
                    ))
                })
                .transpose()?,
        },
        ReviewedTarget::ExpertBinding(reference) => InteractionTarget::ExpertBinding {
            review: experts
                .inspect_binding_review(actor, reference.clone(), scope)
                .await?,
        },
        ReviewedTarget::OperationApproval(reference) => {
            let operation = operations
                .inspect(actor, reference.operation_id, scope)
                .await?;
            if operation.review_ref != *reference || operation.action_ref != reference.operation_id
            {
                return Err(AgentFailure::Conflict);
            }
            InteractionTarget::OperationApproval { operation }
        }
    };
    let allowed_actions = match (state, &target) {
        (InteractionStatus::Pending, InteractionTarget::SourceReview { review }) => {
            let mut actions = vec![InteractionAction::Deny, InteractionAction::Dismiss];
            if review
                .allowed_actions
                .contains(&floe_connections::ConnectionAction::Allow)
            {
                actions.insert(0, InteractionAction::Allow);
            }
            actions
        }
        (InteractionStatus::Pending, InteractionTarget::NavigationOnly { destination, .. }) => {
            vec![
                match destination {
                    NavigationDestination::ConnectionSettings => InteractionAction::OpenConnection,
                    NavigationDestination::SystemPermission => InteractionAction::RequestPermission,
                    NavigationDestination::ResourcePicker => InteractionAction::ReviewSource,
                },
                InteractionAction::Dismiss,
            ]
        }
        (InteractionStatus::Pending, InteractionTarget::ExpertBinding { .. }) => vec![
            InteractionAction::OpenExpertSettings,
            InteractionAction::Refresh,
            InteractionAction::Dismiss,
        ],
        (InteractionStatus::Pending, InteractionTarget::OperationApproval { operation }) => {
            let mut actions = Vec::new();
            if operation
                .allowed_actions
                .contains(&floe_calendar_operations::ActionAllowedAction::Approve)
            {
                actions.push(InteractionAction::Allow);
            }
            if operation
                .allowed_actions
                .contains(&floe_calendar_operations::ActionAllowedAction::Reject)
            {
                actions.push(InteractionAction::Deny);
            }
            if operation
                .allowed_actions
                .contains(&floe_calendar_operations::ActionAllowedAction::Cancel)
            {
                actions.push(InteractionAction::Dismiss);
            }
            actions
        }
        (InteractionStatus::Resolving, _) => {
            vec![InteractionAction::Refresh, InteractionAction::Dismiss]
        }
        _ => vec![],
    };
    Ok(InteractionSnapshot {
        interaction_id: record.id,
        session_id: record.session_id,
        origin_run_id: record.origin_run_id,
        interaction_kind: record.kind,
        state,
        revision: record.revision,
        target_digest: record.target_digest,
        created_at_unix_ms: record.created_at_unix_ms,
        expires_at_unix_ms: record.expires_at_unix_ms,
        requirement: record.requirement.clone(),
        replacement_id: match record.state {
            InteractionState::Superseded { superseded_by } => superseded_by,
            _ => None,
        },
        target,
        allowed_actions,
    })
}

fn source_label(source: &str) -> &'static str {
    match source {
        "floe.source.calendar" => "Calendar",
        "floe.source.contacts" => "Contacts",
        "floe.source.attention" => "Attention",
        "floe.source.wellbeing" => "Health",
        "floe.source.mail" => "Mail",
        "floe.source.work-context" => "Work context",
        "floe.source.logistics" => "Logistics",
        "floe.source.tasks" => "Tasks",
        "floe.source.confirmed-memory" => "Confirmed memory",
        _ => "Connected source",
    }
}
