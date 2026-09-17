//! What this host's vault worker is asked to do, and what it reports back.
//!
//! The worker queue carries the Person's own commands, stated in the words of
//! the owners that will run them. Nothing here is a wire shape: the binding
//! parses a request into one of these and projects the result back out, so that
//! the queue, the job map and the dispatcher never speak a protocol.

use std::collections::BTreeMap;

use floe_agent_contract::AgentFailure;
use floe_kernel::PersonId;
use uuid::Uuid;

use crate::{ConversationTurnRequest, RemoteTurnRoute};

/// The Person's vault, as this process currently holds it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VaultState {
    #[default]
    Missing,
    Locked,
    Ready,
    Unavailable,
}

/// What a scripted fixture run is asked to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FixtureOperation {
    Start,
    Resume,
    Get {
        session_id: Uuid,
    },
    Turn {
        session_id: Uuid,
        expected_revision: u64,
        prompt: crate::AgentFixturePrompt,
    },
    Recover {
        session_id: Uuid,
        expected_revision: u64,
    },
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
    pub consumer: String,
    /// Where the source's contents may be processed.
    pub recipient: String,
}

/// One command the worker runs against this Person's vault.
///
/// Every variant names an owner's own request; none of them carries a wire.
pub enum WorkerAction {
    Status,
    Create,
    Unlock,
    Lock,
    Session {
        operation: FixtureOperation,
    },
    Registry {
        change: Option<floe_experts::RegistryConfiguration>,
    },
    CalendarExperts {
        setup: Option<Box<floe_experts::CalendarExpertSetup>>,
    },
    CalendarAccess {
        change: Box<floe_experts::CalendarAccessConfiguration>,
    },
    CalendarSubjectPreview {
        request: Box<CalendarSubjectRequest>,
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
    RemoteAuthorityInspectProducer {
        route: Box<RemoteTurnRoute>,
    },
    RemoteAuthorityReviewAndEnroll {
        route: Box<RemoteTurnRoute>,
        producer: Box<floe_access::RemoteProducerIdentity>,
    },
    RemoteAuthorityEnrollmentStatus {
        route: Box<RemoteTurnRoute>,
        enrollment_id: String,
    },
    RemotePairingPrepare,
    RemotePairingConfirm {
        route: Box<RemoteTurnRoute>,
        challenge: Box<RemotePairingChallenge>,
        polling_proof: String,
    },
    RemotePairingStatus {
        route: Box<RemoteTurnRoute>,
        pairing_id: String,
        polling_proof: String,
    },
    RemotePairingFinalize {
        route: Box<RemoteTurnRoute>,
        pairing_id: String,
        polling_proof: String,
        challenge: Box<RemotePairingChallenge>,
    },
    RemoteCalendarGrantPreview {
        route: Box<RemoteTurnRoute>,
        connector_id: String,
        connection_id: String,
        resource: String,
    },
    RemoteCalendarGrantReview {
        route: Box<RemoteTurnRoute>,
        connector_id: String,
        connection_id: String,
        resource: String,
        expected_producer_fingerprint: String,
    },
    RemoteCalendarGrantStatus {
        grant_id: floe_access::GrantId,
    },
    RemoteCalendarGrantPause {
        grant_id: floe_access::GrantId,
        expected_authority: floe_access::GrantAuthority,
    },
    RemoteViewGrantPreview {
        route: Box<RemoteTurnRoute>,
        view_id: String,
        connector_id: String,
        connection_id: String,
        resource: String,
        consumer: String,
    },
    RemoteViewGrantReview {
        route: Box<RemoteTurnRoute>,
        view_id: String,
        connector_id: String,
        connection_id: String,
        resource: String,
        consumer: String,
        expected_producer_fingerprint: String,
        expected_source_authority: floe_context_contract::SourceAuthority,
        expected_connection_revision: u64,
        expected_provider_identity: String,
        expected_recipient: String,
    },
    RemoteViewGrantStatus {
        grant_id: floe_access::GrantId,
    },
    RemoteViewGrantPause {
        grant_id: floe_access::GrantId,
        expected_authority: floe_access::GrantAuthority,
    },
}

/// What the worker queue is asked to do with one request.
///
/// This is scheduling only: submit a command, read how far it got, stop it, or
/// let the vault go.
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
            Self::Session { .. } => "session",
            Self::Registry { .. } => "registry",
            Self::CalendarExperts { .. } => "calendar_experts",
            Self::CalendarAccess { .. } => "calendar_access",
            Self::PersonalAccess { .. } => "personal_access",
            Self::ContactsAccess { .. } => "contacts_access",
            Self::CalendarAction { .. } => "calendar_action",
            Self::CalendarSubjectPreview { .. } => "calendar_subject_preview",
            Self::InspectProposal { .. } => "inspect_proposal",
            Self::ConversationSession { .. } => "conversation_session",
            Self::ConversationTurn { .. } => "conversation_turn",
            Self::MemoryReview { .. } => "memory_review",
            Self::Memory => "memory",
            Self::Connections => "connections",
            Self::RemoteAuthorityInspectProducer { .. } => "remote_authority_inspect_producer",
            Self::RemoteAuthorityReviewAndEnroll { .. } => "remote_authority_review_and_enroll",
            Self::RemoteAuthorityEnrollmentStatus { .. } => "remote_authority_enrollment_status",
            Self::RemotePairingPrepare => "remote_pairing_prepare",
            Self::RemotePairingConfirm { .. } => "remote_pairing_confirm",
            Self::RemotePairingStatus { .. } => "remote_pairing_status",
            Self::RemotePairingFinalize { .. } => "remote_pairing_finalize",
            Self::RemoteCalendarGrantPreview { .. } => "remote_calendar_grant_preview",
            Self::RemoteCalendarGrantReview { .. } => "remote_calendar_grant_review",
            Self::RemoteCalendarGrantStatus { .. } => "remote_calendar_grant_status",
            Self::RemoteCalendarGrantPause { .. } => "remote_calendar_grant_pause",
            Self::RemoteViewGrantPreview { .. } => "remote_view_grant_preview",
            Self::RemoteViewGrantReview { .. } => "remote_view_grant_review",
            Self::RemoteViewGrantStatus { .. } => "remote_view_grant_status",
            Self::RemoteViewGrantPause { .. } => "remote_view_grant_pause",
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
                | Self::CalendarExperts { .. }
                | Self::CalendarAccess { .. }
                | Self::PersonalAccess { .. }
                | Self::ContactsAccess { .. }
                | Self::CalendarSubjectPreview { .. }
                | Self::ConversationTurn { .. }
                | Self::InspectProposal { .. }
                | Self::MemoryReview { .. }
                | Self::Memory
                | Self::Connections
                | Self::RemoteAuthorityInspectProducer { .. }
                | Self::RemoteAuthorityReviewAndEnroll { .. }
                | Self::RemoteAuthorityEnrollmentStatus { .. }
                | Self::RemotePairingPrepare
                | Self::RemotePairingConfirm { .. }
                | Self::RemotePairingStatus { .. }
                | Self::RemotePairingFinalize { .. }
                | Self::RemoteCalendarGrantPreview { .. }
                | Self::RemoteCalendarGrantReview { .. }
                | Self::RemoteCalendarGrantStatus { .. }
                | Self::RemoteCalendarGrantPause { .. }
                | Self::RemoteViewGrantPreview { .. }
                | Self::RemoteViewGrantReview { .. }
                | Self::RemoteViewGrantStatus { .. }
                | Self::RemoteViewGrantPause { .. }
        )
    }
}

impl WorkerOperation {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Submit { action } => action.name(),
            Self::Poll { .. } => "poll",
            Self::Stop => "stop",
            Self::Release => "release",
        }
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
    pub events: Vec<floe_conversation::AgentEvent>,
    pub next_sequence: usize,
    pub done: bool,
    pub state: Option<VaultState>,
    pub session: Option<floe_conversation::AgentSession>,
    pub registry: Option<floe_experts::RegistryOverview>,
    pub calendar_experts: Option<floe_experts::CalendarExpertOverview>,
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
    pub calendar_actions: Option<crate::CalendarActionsResult>,
    pub failure: Option<AgentFailure>,
}

/// One trace field a worker result reports about itself.
pub type WorkerTrace = BTreeMap<&'static str, String>;
