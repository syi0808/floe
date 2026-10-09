use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostError {
    InvalidIdentity,
    IdentityUnavailable,
    InvalidRequest,
    Closing,
    Shutdown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalIdentityClaim {
    pub person_id: Uuid,
    pub device_id: String,
}

pub trait LocalIdentityProvider {
    fn verified_local_identity(&self) -> Result<LocalIdentityClaim, HostError>;
}

pub trait HostServices: Send + Sync + 'static {
    fn shutdown(&self) -> Result<(), HostError>;
}

pub use floe_access::{OperationAuthorizationPolicy, OperationPolicyMode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpertCommand {
    SetInstallationEnabled {
        installation_ref: Uuid,
        expected_revision: u64,
        enabled: bool,
    },
    PrepareBindingReview {
        assignment_ref: Uuid,
        requirement_ref: String,
        expected_binding_revision: u64,
    },
    ReplaceBinding {
        review_ref: floe_experts::BindingReviewRef,
        expected_binding_revision: u64,
        candidate_refs: Vec<Uuid>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpertQuery {
    Directory,
    InspectBinding {
        assignment_ref: Uuid,
        requirement_ref: String,
    },
    InspectBindingReview {
        review_ref: floe_experts::BindingReviewRef,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpertCommandResult {
    Directory(floe_experts::ExpertDirectorySnapshot),
    BindingReview(floe_experts::BindingReview),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpertQueryResult {
    Directory(floe_experts::ExpertDirectorySnapshot),
    Binding(floe_experts::BindingInspection),
    BindingReview(floe_experts::BindingReview),
}

/// Product command groups admitted through the App runtime dispatch path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProductCommand {
    Conversation(ConversationCommand),
    Connections(ConnectionsCommand),
    Day(DayCommand),
    Memory(MemoryCommand),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductCommandRequest {
    pub command_id: floe_kernel::CommandId,
    pub command: ProductCommand,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConversationCommand {
    StartSession,
    Expert(ExpertCommand),
    SetCalendarOperationPolicy {
        mode: OperationPolicyMode,
        expected_revision: u64,
    },
    StartTurn {
        session_id: Uuid,
        expected_revision: u64,
        text: String,
        continuation_id: Option<Uuid>,
        retry_of: Option<floe_kernel::RunId>,
    },
    SubmitCalendarProposal {
        session_id: Uuid,
        origin_run_id: floe_kernel::RunId,
        receipt: floe_agent_contract::TaskExecutionReceiptRef,
        artifact_id: Uuid,
        destination_ref: Uuid,
    },
    CancelRun {
        run_id: floe_kernel::RunId,
    },
    ResolveInteraction {
        interaction_id: Uuid,
        session_id: Uuid,
        expected_revision: u64,
        decision: floe_conversation::InteractionDecisionKind,
        target_digest: [u8; 32],
    },
    RefreshInteraction {
        interaction_id: Uuid,
        session_id: Uuid,
        expected_revision: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectionsCommand {
    PairingStart {
        address_text: String,
    },
    PairingCancel {
        operation_ref: Uuid,
        expected_revision: u64,
    },
    GatewayForget {
        gateway_ref: Uuid,
        expected_revision: u64,
    },
    IntegrationPrepareReview {
        integration_ref: Uuid,
        expected_revision: u64,
    },
    IntegrationStart {
        integration_ref: Uuid,
        review_ref: floe_access::ReviewRef,
        expected_revision: u64,
    },
    OperationCancel {
        operation_ref: Uuid,
        expected_revision: u64,
    },
    SourcePrepareReview {
        source_ref: Uuid,
        expected_revision: u64,
    },
    SourceConfigure {
        source_ref: Uuid,
        review_ref: floe_access::ReviewRef,
        selected_resource_refs: Vec<Uuid>,
        expected_revision: u64,
    },
    Disconnect {
        source_ref: Uuid,
        expected_revision: u64,
    },
    ObservePrepareReview {
        source_ref: Uuid,
        expected_revision: u64,
        requested_processing: floe_connections::ProcessingChoice,
    },
    ObserveEnable {
        source_ref: Uuid,
        review_ref: floe_access::ReviewRef,
        expected_revision: u64,
    },
    ObservePause {
        source_ref: Uuid,
        expected_revision: u64,
    },
    GatewayManagementLaunch {
        gateway_ref: Uuid,
        expected_revision: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DayCommand {
    Refresh(floe_day::DayQuery),
    Mutate {
        day: floe_day::DayQuery,
        mutation: floe_day::DayMutation,
    },
    ExternalCalendarOperation {
        operation: floe_day::ManualCalendarOperation,
    },
    ReconcileExternalCalendarOperation {
        operation_ref: Uuid,
        expected_revision: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryCommand {
    Decide {
        candidate_id: Uuid,
        decision: floe_knowledge::KnowledgeDecisionKind,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProductQuery {
    Conversation(ConversationQuery),
    Connections(ConnectionsQuery),
    Day(DayProductQuery),
    Memory(MemoryQuery),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConversationQuery {
    CalendarOperationPolicy,
    Expert(ExpertQuery),
    ResumeSession,
    GetSession {
        session_id: Uuid,
        before_message_id: Option<Uuid>,
    },
    GetCommand {
        command_id: floe_kernel::CommandId,
    },
    GetRun {
        run_id: floe_kernel::RunId,
    },
    GetMessage {
        message_id: Uuid,
    },
    GetInteraction {
        interaction_id: Uuid,
    },
    ListInteractions {
        session_id: Uuid,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectionsQuery {
    Overview,
    PairingGet { operation_ref: Uuid },
    GatewayGet { gateway_ref: Uuid },
    IntegrationInspectReview { review_ref: floe_access::ReviewRef },
    OperationGet { operation_ref: Uuid },
    SourceInspectReview { review_ref: floe_access::ReviewRef },
    ObserveInspectReview { review_ref: floe_access::ReviewRef },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DayProductQuery {
    Snapshot(floe_day::DayQuery),
    RefreshGet { operation_ref: Uuid },
    ExternalCalendarOperationGet { operation_ref: Uuid },
    ExternalCalendarOperations { cursor: Option<Uuid>, limit: u16 },
    ExternalCalendarDestinations,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryQuery {
    Overview { limit: usize },
    Review,
}

#[derive(Clone, Debug)]
pub enum ProductCommandOutcome {
    Conversation(ConversationCommandOutcome),
    Connections(ConnectionsCommandOutcome),
    Day(DayCommandOutcome),
    Memory(floe_knowledge::MemoryDecisionAcknowledgement),
}

#[derive(Clone, Debug)]
pub enum ConversationCommandOutcome {
    Session(floe_conversation::SessionSnapshot),
    CalendarOperationPolicy(OperationAuthorizationPolicy),
    Expert(ExpertCommandResult),
    Turn(floe_conversation::CommandReceipt),
    CalendarProposal(floe_conversation::CalendarProposalResult),
    CancelRun(floe_conversation::CancelRunReceipt),
    Interaction(floe_conversation::InteractionResult),
    InteractionRefresh(floe_conversation::InteractionResult),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectionsCommandOutcome {
    Pairing(floe_connections::PairingSnapshot),
    Gateway(floe_connections::GatewaySummary),
    IntegrationReview(floe_connections::IntegrationReview),
    Operation(floe_connections::ConnectionOperationSnapshot),
    SourceReview(floe_connections::SourceReview),
    ObserveReview(floe_connections::ObserveReview),
    SourceConfiguration(floe_connections::SourceConfigurationResult),
    Source(floe_connections::SourceSummary),
    Launch(floe_connections::ValidatedManagementLaunch),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DayCommandOutcome {
    Refresh(floe_day::DayRefreshSnapshot),
    Mutation(floe_day::DayMutationResult),
    ExternalCalendarOperation(floe_day::ManualCalendarOperationReceipt),
    ReconciledExternalCalendarOperation(floe_day::ManualCalendarOperationReceipt),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProductQueryOutcome {
    Conversation(ConversationQueryOutcome),
    Connections(ConnectionsQueryOutcome),
    Day(DayQueryOutcome),
    Memory(MemoryQueryResult),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConversationQueryOutcome {
    CalendarOperationPolicy(OperationAuthorizationPolicy),
    Expert(ExpertQueryResult),
    Session(Option<floe_conversation::SessionSnapshot>),
    Command(Option<floe_conversation::RunReceipt>),
    Run(Option<floe_conversation::RunReceipt>),
    Message(Option<floe_conversation::MessageSnapshot>),
    Interaction(Option<floe_conversation::InteractionSnapshot>),
    Interactions(Vec<floe_conversation::InteractionSnapshot>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectionsQueryOutcome {
    Overview(floe_connections::ConnectionsOverview),
    Pairing(floe_connections::PairingSnapshot),
    Gateway(floe_connections::GatewaySummary),
    IntegrationReview(floe_connections::IntegrationReview),
    Operation(floe_connections::ConnectionOperationSnapshot),
    SourceReview(floe_connections::SourceReview),
    ObserveReview(floe_connections::ObserveReview),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DayQueryOutcome {
    Snapshot(floe_day::DaySnapshot),
    Refresh(floe_day::DayRefreshSnapshot),
    ExternalCalendarDestinations(Vec<floe_day::ManualCalendarDestination>),
    ExternalCalendarOperation(floe_day::ManualCalendarOperationReceipt),
    ExternalCalendarOperations(floe_day::ManualCalendarOperationPage),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryQueryResult {
    Overview(floe_knowledge::MemoryOverviewSnapshot),
    Review(floe_knowledge::MemoryReviewDisplay),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductObservation {
    pub runtime_epoch: Option<u64>,
    pub cursor: Option<u64>,
    pub limit: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProductObservationOutcome {
    Conversation(floe_conversation::EventRead),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductCommandDisposition {
    NotAdmitted,
    NotApplied,
    Admitted,
    Indeterminate,
}

#[derive(Debug)]
pub enum ProductFailure {
    Conversation(floe_kernel::AgentFailure),
    Connections(floe_kernel::AgentFailure),
    Memory(floe_kernel::AgentFailure),
    Day(crate::CoreError),
}

#[derive(Debug)]
pub struct ProductCommandFailure {
    pub disposition: ProductCommandDisposition,
    pub failure: ProductFailure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallerContext {
    person_id: Uuid,
    device_id: String,
    runtime_epoch: u64,
}

impl CallerContext {
    pub fn owner_actor(&self) -> floe_kernel::OwnerActor {
        floe_kernel::OwnerActor {
            person_id: floe_kernel::PersonId(self.person_id),
            device_id: self.device_id.clone(),
            runtime_epoch: self.runtime_epoch,
        }
    }

    pub fn person_id(&self) -> Uuid {
        self.person_id
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn runtime_epoch(&self) -> u64 {
        self.runtime_epoch
    }

    pub(crate) fn verified(
        claim: LocalIdentityClaim,
        runtime_epoch: u64,
    ) -> Result<Self, HostError> {
        if claim.person_id.is_nil()
            || claim.device_id.trim() != claim.device_id
            || claim.device_id.is_empty()
            || claim.device_id.len() > 128
            || claim.device_id.chars().any(char::is_control)
            || runtime_epoch == 0
            || runtime_epoch > i64::MAX as u64
        {
            return Err(HostError::InvalidIdentity);
        }
        Ok(Self {
            person_id: claim.person_id,
            device_id: claim.device_id,
            runtime_epoch,
        })
    }
}
