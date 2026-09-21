mod agent;
mod calendar;
mod commands;
mod day;
mod envelope;
mod errors;
mod events;
mod local_context;
mod queries;

pub const PROTOCOL_VERSION: u32 = 1;
pub const APP_WIRE_VERSION: u32 = 2;

pub use agent::{
    ActionAuthorityModeDto, AgentConversationSessionOperationDto, AgentEventDto, AgentFailureCategory, AgentFailureDomain, AgentFailureDto,
    AgentFailureSafeAction, AgentFixtureOperationDto, AgentFixturePromptDto,
    AgentFixtureRequestDto, AgentFixtureResultDto, AgentFixtureRunDto, AgentFixtureRunOperationDto,
    AgentFixtureRunRequestDto, AgentMemoryOriginDto, AgentMemoryOverviewDto,
    AgentMemoryReviewDecisionDto, AgentMemoryReviewDecisionKindDto, AgentMemoryReviewOverviewDto,
    AgentMemorySummaryDto, AgentProposalActionDto, AgentProposalInspectionDto,
    AgentProposalStatusDto, AgentRemoteCalendarConnectionDto, AgentRemotePairingDto,
    AgentRemoteRouteDto, AgentRetryPolicy, AgentSessionDto, AgentVaultActionDto,
    AgentVaultFailureDto, AgentVaultOperationDto, AgentVaultRecoveryActionDto,
    AgentVaultRequestDto, AgentVaultResultDto, AgentVaultStateDto, CalendarAccessChangeDto,
    CalendarAccessConfigurationDto, CalendarActionDecisionDto, CalendarActionOperationDto,
    CalendarActionRequestDto, CalendarExpertOverviewDto, CalendarExpertSetupDto,
    CalendarSubjectPreviewDto, CalendarSubjectPreviewRequestDto, ConnectorSnapshotDto,
    ContactsAccessChangeDto, ContactsAccessConfigurationDto, EpistemicStatusDto,
    FeasibilityGrantQueryDto, KnowledgeCandidateDto, KnowledgeDecisionResultDto,
    PersonalAccessChangeDto, PersonalAccessConfigurationDto, PersonalAccessOverviewDto,
    PersonalMemoryKindDto, RegistryConfigurationDto, RegistryConfigurationTargetDto,
    RegistryOverviewDto, RemoteAuthorityEnrollmentStatusDto, RemoteCalendarGrantOverviewDto,
    RemoteCalendarGrantPreviewDto, RemoteOwnerPublicKeyDto, RemotePairingChallengeDto,
    RemotePairingConfirmationDto, RemotePairingStatusDto, RemoteProducerIdentityDto,
    RemoteViewGrantOverviewDto, RemoteViewGrantPreviewDto,
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
    ClassificationDto, CommandDto, CommandRequestDto, DayQueryDto, DaySnapshotDto, DomainRefDto,
    EventDto, EventScheduleDto, LoadDayRequestDto, MutationResultDto, NoteDto, PriorityDto,
    SourceRefDto, TaskDto, TimelineItemDto,
};
pub use envelope::{ErrorCodeDto, ErrorDto, ResponseEnvelopeDto, ResponseOutcomeDto};
pub use errors::{AppResponseDto, AppResponseOutcomeDto, AppWireErrorCodeDto, AppWireErrorDto};
pub use events::{AppEventDto, AppEventKindDto, AppEventsRequestDto, AppEventsResultDto};
pub use local_context::{
    LocalContextAcquisitionModeDto, LocalContextAcquisitionRequestDto,
    LocalContextAcquisitionResultDto, LocalContextAttentionAcquisitionModeDto,
    LocalContextAttentionAcquisitionRequestDto, LocalContextAttentionAcquisitionResultDto,
    LocalContextOperationDto, LocalContextPersonalAcquisitionRequestDto,
    LocalContextPersonalAcquisitionResultDto, LocalContextPersonalDomainDto,
    LocalContextRequestDto, LocalContextResultDto,
};
pub use queries::{
    AppMessageDto, AppMessageRoleDto, AppQueryDto, AppQueryRequestDto, AppQueryResultDto,
    AppReplyStatusDto, AppRunSnapshotDto, AppRunStateDto, AppTurnExecutionDto, AppTurnReportDto,
};
