mod access;
mod actions;
pub use access::{
    ConnectionObserveExpectationDto, ConnectionObserveMemberDto, RemoteAccessOperationDto,
    RemoteAccessRequestDto, RemoteAccessResultDto, RemoteAuthorityEnrollmentStatusDto,
    RemoteOwnerPublicKeyDto, RemoteProducerIdentityDto,
};
pub use actions::ActionOperationResultDto;
pub use connections::RemotePairingChallengeDto;
mod connections;
pub use connections::{
    ConnectionsResultDto, PairingOutcomeDto, PairingReportDto, PairingTargetDto,
    RemotePairingOperationDto, RemotePairingRequestDto, RemotePairingResultDto,
};
mod agent;
mod calendar;
mod commands;
mod conversation;
pub use conversation::ConversationSessionResultDto;
mod day;
mod day_mutation;
pub use day_mutation::DayMutationDto;
mod envelope;
mod errors;
mod events;
mod experts;
mod interactions;
pub use experts::ExpertOperationResultDto;
mod context;
mod local_context;
pub use context::{
    AttentionCompletionDto, CalendarCompletionDto, ContextCommandDto, ContextQueryDto,
    PersonalCompletionDto,
};
mod knowledge;
pub use knowledge::KnowledgeOperationResultDto;
mod local_access;
pub use local_access::{
    CalendarAccessChangeDto, CalendarAccessOverviewDto, CalendarSubjectIntentDto,
    LocalAccessResultDto,
};
mod queries;
mod vault;
pub use vault::VaultLifecycleResultDto;

pub const PROTOCOL_VERSION: u32 = 1;
pub const APP_WIRE_VERSION: u32 = 2;

pub use agent::{
    ActionAuthorityModeDto, AgentEventDto, AgentFailureCategory, AgentFailureDomain,
    AgentFailureDto, AgentFailureSafeAction, AgentMemoryOriginDto, AgentMemoryOverviewDto,
    AgentMemoryReviewDecisionKindDto, AgentMemoryReviewOverviewDto, AgentMemorySummaryDto,
    AgentProposalActionDto, AgentProposalInspectionDto, AgentProposalStatusDto, AgentRetryPolicy,
    AgentSessionDto, AgentVaultFailureDto, AgentVaultRecoveryActionDto, AgentVaultStateDto,
    CalendarActionDecisionDto, CalendarActionOperationDto, CalendarSubjectPreviewDto,
    ConnectorSnapshotDto, ContactsAccessChangeDto, EpistemicStatusDto, FeasibilityGrantQueryDto,
    KnowledgeCandidateDto, KnowledgeDecisionResultDto, PersonalAccessChangeDto,
    PersonalAccessOverviewDto, PersonalMemoryKindDto, RegistryConfigurationDto,
    RegistryConfigurationTargetDto, RegistryOverviewDto,
};
pub use calendar::{
    CalendarConnectionDto, CalendarFailureDto, CalendarProviderDto, CalendarRangeDto,
    CalendarScopeDto, CalendarSelectionDto, CalendarSourceDto, CalendarSyncStatusDto,
};
pub use commands::{
    AppCancelRunOutcomeDto, AppCancelRunReasonDto, AppCommandDto, AppCommandReceiptDto,
    AppCommandRequestDto, AppCommandResultDto, AppCommandStatusDto, AppContinuationRefDto,
    AppProfileSelectionDto, AppTurnModeDto,
};
pub use day::{
    CalendarBatchDto, CalendarRecordDto, CaptureDto, CaptureProcessingDto, CaptureSourceDto,
    ClassificationDto, DayQueryDto, DaySnapshotDto, DomainRefDto, EventDto, EventScheduleDto,
    MutationResultDto, NoteDto, PriorityDto, SourceRefDto, TaskDto, TimelineItemDto,
};
pub use envelope::{ErrorCodeDto, ErrorDto, ResponseEnvelopeDto, ResponseOutcomeDto};
pub use errors::{AppResponseDto, AppResponseOutcomeDto, AppWireErrorCodeDto, AppWireErrorDto};
pub use events::{AppEventDto, AppEventKindDto, AppEventsRequestDto, AppEventsResultDto};
pub use interactions::{
    AppConsentScopeDto, AppInteractionActionDto, AppInteractionDecisionDto, AppInteractionKindDto,
    AppInteractionListDto, AppInteractionRefreshOutcomeDto, AppInteractionRefreshResultDto,
    AppInteractionResolveOutcomeDto, AppInteractionResolveResultDto, AppInteractionSnapshotDto,
    AppInteractionStateDto, AppInteractionTargetDto, AppNavigationDestinationDto,
    AppObservedMemberDto, MAX_INTERACTIONS_PER_LIST,
};
pub use local_context::{
    LocalContextAcquisitionModeDto, LocalContextAcquisitionRequestDto,
    LocalContextAttentionAcquisitionModeDto, LocalContextAttentionAcquisitionRequestDto,
    LocalContextPersonalAcquisitionRequestDto, LocalContextPersonalDomainDto,
    LocalContextResultDto,
};
pub use queries::{
    AppMessageDto, AppMessageRoleDto, AppQueryDto, AppQueryRequestDto, AppQueryResultDto,
    AppReplyStatusDto, AppRunSnapshotDto, AppRunStateDto, AppTurnExecutionDto, AppTurnReportDto,
};
