use crate::{ActionInspection, CalendarActionOperation};
use crate::{CallerContext, VaultLifecycleCommand, WorkerAction};
use crate::{ExpertCommand, ExpertInspection};
use crate::{KnowledgeInspection, MemoryReviewDecision};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LocalOperationIntent {
    VaultStatus,
    VaultCommand(VaultLifecycleCommand),
    ExpertCommand(ExpertCommand),
    ExpertInspection(ExpertInspection),
    KnowledgeInspection(KnowledgeInspection),
    MemoryDecision(MemoryReviewDecision),
    ActionCommand(CalendarActionOperation),
    ActionInspection(ActionInspection),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LocalOperationOwner {
    Vault,
    Experts,
    Knowledge,
    Actions,
}

impl LocalOperationIntent {
    pub(crate) fn owner(&self) -> LocalOperationOwner {
        match self {
            Self::VaultStatus | Self::VaultCommand(_) => LocalOperationOwner::Vault,
            Self::ExpertCommand(_) | Self::ExpertInspection(_) => LocalOperationOwner::Experts,
            Self::KnowledgeInspection(_) | Self::MemoryDecision(_) => {
                LocalOperationOwner::Knowledge
            }
            Self::ActionCommand(_) | Self::ActionInspection(_) => LocalOperationOwner::Actions,
        }
    }

    pub(crate) fn action(&self, caller: &CallerContext) -> WorkerAction {
        match self {
            Self::VaultStatus => WorkerAction::Status,
            Self::VaultCommand(VaultLifecycleCommand::Create) => WorkerAction::Create,
            Self::VaultCommand(VaultLifecycleCommand::Unlock) => WorkerAction::Unlock,
            Self::VaultCommand(VaultLifecycleCommand::Lock) => WorkerAction::Lock,
            Self::ExpertInspection(ExpertInspection::Registry) => {
                WorkerAction::Registry { change: None }
            }
            Self::ExpertInspection(ExpertInspection::Candidates {
                assignment_id,
                requirement_key,
            }) => WorkerAction::ExpertCandidates {
                assignment_id: *assignment_id,
                requirement_key: requirement_key.clone(),
                device_id: caller.device_id().to_owned(),
            },
            Self::ExpertCommand(ExpertCommand::ConfigureRegistry(change)) => {
                WorkerAction::Registry {
                    change: Some(change.clone()),
                }
            }
            Self::ExpertCommand(ExpertCommand::ReplaceBinding(change)) => {
                WorkerAction::ExpertReplaceBinding {
                    change: change.clone(),
                    device_id: caller.device_id().to_owned(),
                }
            }
            Self::KnowledgeInspection(KnowledgeInspection::Memory) => WorkerAction::Memory,
            Self::KnowledgeInspection(KnowledgeInspection::Review) => {
                WorkerAction::MemoryReview { decision: None }
            }
            Self::MemoryDecision(decision) => WorkerAction::MemoryReview {
                decision: Some(*decision),
            },
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
