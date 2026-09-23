use crate::{ActionInspection, CalendarActionOperation};
use crate::{CallerContext, ConversationSessionOperation, VaultLifecycleCommand, WorkerAction};
use crate::{ExpertCommand, ExpertInspection};
use crate::{KnowledgeInspection, MemoryReviewDecision};
use crate::{LocalAccessCommand, LocalAccessInspection};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LocalOperationIntent {
    VaultStatus,
    VaultCommand(VaultLifecycleCommand),
    ConversationSession(ConversationSessionOperation),
    ExpertCommand(ExpertCommand),
    ExpertInspection(ExpertInspection),
    LocalAccessCommand(LocalAccessCommand),
    LocalAccessInspection(LocalAccessInspection),
    KnowledgeInspection(KnowledgeInspection),
    MemoryDecision(MemoryReviewDecision),
    Connections,
    ActionCommand(CalendarActionOperation),
    ActionInspection(ActionInspection),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LocalOperationOwner {
    Vault,
    Conversation,
    Experts,
    Access,
    Knowledge,
    Connections,
    Actions,
}

impl LocalOperationIntent {
    pub(crate) fn owner(&self) -> LocalOperationOwner {
        match self {
            Self::VaultStatus | Self::VaultCommand(_) => LocalOperationOwner::Vault,
            Self::ConversationSession(_) => LocalOperationOwner::Conversation,
            Self::ExpertCommand(_) | Self::ExpertInspection(_) => LocalOperationOwner::Experts,
            Self::LocalAccessCommand(_) | Self::LocalAccessInspection(_) => {
                LocalOperationOwner::Access
            }
            Self::KnowledgeInspection(_) | Self::MemoryDecision(_) => {
                LocalOperationOwner::Knowledge
            }
            Self::Connections => LocalOperationOwner::Connections,
            Self::ActionCommand(_) | Self::ActionInspection(_) => LocalOperationOwner::Actions,
        }
    }

    pub(crate) fn action(&self, caller: &CallerContext) -> WorkerAction {
        match self {
            Self::VaultStatus => WorkerAction::Status,
            Self::VaultCommand(VaultLifecycleCommand::Create) => WorkerAction::Create,
            Self::VaultCommand(VaultLifecycleCommand::Unlock) => WorkerAction::Unlock,
            Self::VaultCommand(VaultLifecycleCommand::Lock) => WorkerAction::Lock,
            Self::ConversationSession(operation) => WorkerAction::ConversationSession {
                operation: operation.clone(),
            },
            Self::ExpertInspection(ExpertInspection::Registry) => {
                WorkerAction::Registry { change: None }
            }
            Self::ExpertCommand(ExpertCommand::ConfigureRegistry(change)) => {
                WorkerAction::Registry {
                    change: Some(change.clone()),
                }
            }
            Self::LocalAccessCommand(command) => command.action(caller),
            Self::LocalAccessInspection(inspection) => inspection.action(caller),
            Self::KnowledgeInspection(KnowledgeInspection::Memory) => WorkerAction::Memory,
            Self::KnowledgeInspection(KnowledgeInspection::Review) => {
                WorkerAction::MemoryReview { decision: None }
            }
            Self::MemoryDecision(decision) => WorkerAction::MemoryReview {
                decision: Some(*decision),
            },
            Self::Connections => WorkerAction::Connections,
            Self::ActionCommand(operation) => WorkerAction::CalendarAction {
                operation: operation.clone(),
            },
            Self::ActionInspection(inspection) => match inspection {
                ActionInspection::Capabilities => WorkerAction::CalendarAction {
                    operation: CalendarActionOperation::Capabilities,
                },
                ActionInspection::Authority => WorkerAction::CalendarAction {
                    operation: CalendarActionOperation::GetAuthority,
                },
                ActionInspection::List => WorkerAction::CalendarAction {
                    operation: CalendarActionOperation::List,
                },
                ActionInspection::Get { action_id } => WorkerAction::CalendarAction {
                    operation: CalendarActionOperation::Get {
                        action_id: *action_id,
                    },
                },
                ActionInspection::Proposal {
                    session_id,
                    invocation_id,
                } => WorkerAction::InspectProposal {
                    session_id: *session_id,
                    invocation_id: *invocation_id,
                },
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalOperationAdmission {
    pub caller: CallerContext,
    pub intent: LocalOperationIntent,
}
