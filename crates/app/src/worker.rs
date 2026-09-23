//! What this host's vault worker is asked to do, and what it reports back.
//!
//! The worker queue carries the Person's own commands, stated in the words of
//! the owners that will run them. Nothing here is a wire shape: the binding
//! parses a request into one of these and projects the result back out, so that
//! the queue, the job map and the dispatcher never speak a protocol.

use floe_agent_contract::AgentFailure;
use floe_kernel::PersonId;
use uuid::Uuid;

use crate::ConversationTurnRequest;

/// The Person's vault, as this process currently holds it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VaultState {
    #[default]
    Missing,
    Locked,
    Ready,
    Unavailable,
}

/// What a Session command is asked to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConversationSessionOperation {
    Start,
    Resume,
    Get {
        session_id: Uuid,
    },
    Recover {
        session_id: Uuid,
        expected_revision: u64,
    },
}

/// What a calendar action command is asked to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CalendarActionOperation {
    Capabilities,
    GetAuthority,
    SetAuthority {
        calendar_create: floe_actions::ActionAuthorityMode,
    },
    Execute {
        action_id: Uuid,
    },
    Recover {
        action_id: Uuid,
    },
    List,
    Get {
        action_id: Uuid,
    },
    Propose(Box<CalendarActionProposal>),
    Direct(Box<CalendarActionProposal>),
    Decide {
        action_id: Uuid,
        approve: bool,
    },
}

/// One calendar write the Person is being asked about.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarActionProposal {
    pub calendar_id: String,
    pub title: String,
    pub starts_at: String,
    pub ends_at: String,
    pub timezone: String,
    pub event_id: Option<String>,
    pub event_revision: Option<u64>,
    pub delete: bool,
}

/// One decision the Person made about a learned memory, and what they are
/// shown afterwards.
///
/// Both are Knowledge's own values; the worker only carries them.
pub use floe_knowledge::{MemoryReviewDecision, MemoryReviewResult};

/// What the device reports about the calendar a grant would name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarSubjectRequest {
    pub provider: floe_context_contract::CalendarProvider,
    pub device_id: String,
    pub connection_id: String,
    pub calendar_ids: Vec<String>,
    pub connection_scope: floe_context_contract::CalendarScope,
    pub connection_revision: u64,
    pub source_authority: floe_context_contract::SourceAuthority,
}

/// The subject a device would answer for, as the Person is shown it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarSubjectPreview {
    pub provider: floe_context_contract::CalendarProvider,
    pub device_id: String,
    pub calendar_ids: Vec<String>,
    pub connection_scope: floe_context_contract::CalendarScope,
    pub connection_id: String,
    pub connection_revision: u64,
    pub source_authority: floe_context_contract::SourceAuthority,
    pub native_subject_fingerprint: String,
}

/// One producer pairing challenge, as the producer issued it.
///
/// The challenge is Access's: what a producer claims and what the owner key is
/// asked to sign over. The worker only carries it to the key holder.
pub use floe_access::RemotePairingChallenge;

/// One proposal the Person asked to inspect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CalendarProposalInspection {
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub invocation_id: Uuid,
    pub action: Option<floe_actions::CalendarAction>,
}

/// One remote grant, with the connection revision it was last previewed at.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteGrantOverview {
    pub grant: floe_access::DataAccessGrant,
    pub connection_revision: Option<u64>,
}

/// What the Person is shown before they grant a remote calendar source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteCalendarGrantPreview {
    pub person_id: PersonId,
    pub connector_id: String,
    pub connection_id: String,
    pub resource: String,
    pub source_authority: floe_context_contract::SourceAuthority,
    pub provider_identity: String,
    pub execution_owner: String,
    pub producer: floe_access::RemoteProducerIdentity,
    pub consumers: Vec<String>,
    /// Where the source's contents may be processed.
    pub recipient: String,
    /// The exact current grant for this source and resource, if any. All
    /// three are present or all three are absent; the review echoes them.
    pub grant_id: Option<floe_access::GrantId>,
    pub grant_authority: Option<floe_access::GrantAuthority>,
    pub consumer_policy: Option<floe_context_contract::ConsumerPolicyAuthority>,
}

/// One command the worker runs against this Person's vault.
///
/// Every variant names an owner's own request; none of them carries a wire.
pub enum WorkerAction {
    Status,
    Create,
    Unlock,
    Lock,
    Registry {
        change: Option<floe_experts::RegistryConfiguration>,
    },
    CalendarSubjectPreview {
        request: Box<CalendarSubjectRequest>,
    },
    CalendarAccess {
        change: Box<crate::CalendarAccessConfiguration>,
    },
    PersonalAccess {
        change: Box<floe_access::PersonalAccessConfiguration>,
    },
    ContactsAccess {
        change: Box<floe_access::ContactsAccessConfiguration>,
    },
    CalendarAction {
        operation: CalendarActionOperation,
    },
    InspectProposal {
        session_id: Uuid,
        invocation_id: Uuid,
    },
    ConversationSession {
        operation: ConversationSessionOperation,
    },
    ConversationTurn {
        request: Box<ConversationTurnRequest>,
    },
    MemoryReview {
        decision: Option<MemoryReviewDecision>,
    },
    Memory,
    Connections,
    RemoteAccess {
        caller: crate::CallerContext,
        command: crate::RemoteAccessCommand,
    },
    RemotePairing {
        caller: crate::CallerContext,
        command: crate::RemotePairingCommand,
    },
}

/// What the worker queue is asked to do with one request.
///
/// This is scheduling only: submit a command, read how far it got, stop it, or
/// let the vault go.
#[cfg(test)]
pub enum WorkerOperation {
    Submit { action: Box<WorkerAction> },
    Poll { after_sequence: usize },
    Stop,
    Release,
}

impl WorkerAction {
    /// The stage name a failure on this command is reported under.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Create => "create",
            Self::Unlock => "unlock",
            Self::Lock => "lock",
            Self::Registry { .. } => "registry",
            Self::PersonalAccess { .. } => "personal_access",
            Self::ContactsAccess { .. } => "contacts_access",
            Self::CalendarAction { .. } => "calendar_action",
            Self::CalendarSubjectPreview { .. } => "calendar_subject_preview",
            Self::CalendarAccess { .. } => "calendar_access",
            Self::InspectProposal { .. } => "inspect_proposal",
            Self::ConversationSession { .. } => "conversation_session",
            Self::ConversationTurn { .. } => "conversation_turn",
            Self::MemoryReview { .. } => "memory_review",
            Self::Memory => "memory",
            Self::Connections => "connections",
            Self::RemoteAccess { command, .. } => command.name(),
            Self::RemotePairing { command, .. } => command.name(),
        }
    }

    /// Whether this command needs the host to itself, with no vault open.
    pub fn is_exclusive_host(&self) -> bool {
        matches!(self, Self::Create | Self::Unlock | Self::Lock)
    }

    /// Whether this command may run beside another on the open vault.
    pub fn is_concurrent_host(&self) -> bool {
        matches!(
            self,
            Self::Status
                | Self::Registry { .. }
                | Self::PersonalAccess { .. }
                | Self::ContactsAccess { .. }
                | Self::CalendarAccess { .. }
                | Self::CalendarSubjectPreview { .. }
                | Self::ConversationTurn { .. }
                | Self::ConversationSession {
                    operation: ConversationSessionOperation::Get { .. },
                }
                | Self::InspectProposal { .. }
                | Self::MemoryReview { .. }
                | Self::Memory
                | Self::Connections
                | Self::RemoteAccess { .. }
                | Self::RemotePairing { .. }
        )
    }
}

/// How far one command got, and everything it produced.
///
/// Every slot is an owner's own value; a caller decides how to say it.
#[derive(Clone, Debug)]
pub struct WorkerResult {
    pub request_id: Uuid,
    pub person_id: PersonId,
    pub stage: String,
    #[cfg(test)]
    pub events: Vec<floe_conversation::AgentEvent>,
    pub done: bool,
    pub state: Option<VaultState>,
    pub session: Option<floe_conversation::AgentSession>,
    pub registry: Option<floe_experts::RegistryOverview>,
    pub calendar_subject_preview: Option<CalendarSubjectPreview>,
    pub proposal: Option<CalendarProposalInspection>,
    pub memory_review: Option<MemoryReviewResult>,
    pub memory: Option<floe_knowledge::MemoryOverviewSnapshot>,
    pub connections: Option<Vec<floe_connections::ConnectorSnapshot>>,
    pub remote_producer: Option<floe_access::RemoteProducerIdentity>,
    pub remote_enrollment: Option<floe_access::RemoteEnrollmentStatus>,
    pub remote_pairing: Option<floe_connections::PairingStatus>,
    pub remote_owner: Option<floe_access::RemoteOwnerPublicKey>,
    pub remote_calendar_grant: Option<RemoteGrantOverview>,
    pub remote_calendar_preview: Option<RemoteCalendarGrantPreview>,
    pub remote_view_grant: Option<RemoteGrantOverview>,
    pub remote_view_preview: Option<floe_access::RemoteViewGrantPreview>,
    pub personal_access: Option<floe_access::PersonalAccessOverview>,
    pub calendar_access: Option<crate::CalendarAccessOverview>,
    pub calendar_actions: Option<crate::CalendarActionsResult>,
    pub failure: Option<AgentFailure>,
}

impl WorkerAction {
    pub(crate) fn remote_caller(&self) -> Option<&crate::CallerContext> {
        match self {
            Self::RemotePairing { caller, .. } | Self::RemoteAccess { caller, .. } => Some(caller),
            _ => None,
        }
    }

    pub(crate) fn same_remote_command(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::RemotePairing { caller, command },
                Self::RemotePairing {
                    caller: other_caller,
                    command: other_command,
                },
            ) => caller == other_caller && command == other_command,
            (
                Self::RemoteAccess { caller, command },
                Self::RemoteAccess {
                    caller: other_caller,
                    command: other_command,
                },
            ) => caller == other_caller && command == other_command,
            _ => false,
        }
    }
}
