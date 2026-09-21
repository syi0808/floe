pub mod conversion;
mod dto;
pub mod wire;

pub use dto::{
    APP_WIRE_VERSION, ActionAuthorityModeDto, AgentConversationSessionOperationDto,
    AgentEventDto, AgentFailureCategory, AgentFailureDomain,
    AgentFailureDto, AgentFailureSafeAction, AgentFixtureOperationDto, AgentFixturePromptDto,
    AgentFixtureRequestDto, AgentFixtureResultDto, AgentFixtureRunDto, AgentFixtureRunOperationDto,
    AgentFixtureRunRequestDto, AgentMemoryOriginDto, AgentMemoryOverviewDto,
    AgentMemoryReviewDecisionDto, AgentMemoryReviewDecisionKindDto, AgentMemoryReviewOverviewDto,
    AgentMemorySummaryDto, AgentProposalActionDto, AgentProposalInspectionDto,
    AgentProposalStatusDto, AgentRemoteCalendarConnectionDto, AgentRemotePairingDto,
    AgentRemoteRouteDto, AgentRetryPolicy, AgentSessionDto, AgentVaultActionDto,
    AgentVaultFailureDto, AgentVaultOperationDto, AgentVaultRecoveryActionDto,
    AgentVaultRequestDto, AgentVaultResultDto, AgentVaultStateDto, AppCancelRunOutcomeDto,
    AppCancelRunReasonDto, AppCommandDto, AppCommandReceiptDto, AppCommandRequestDto,
    AppCommandResultDto, AppCommandStatusDto, AppContinuationRefDto, AppEventDto, AppEventKindDto,
    AppEventsRequestDto, AppEventsResultDto, AppMessageDto, AppMessageRoleDto,
    AppProfileSelectionDto, AppQueryDto, AppQueryRequestDto, AppQueryResultDto, AppReplyStatusDto,
    AppResponseDto, AppResponseOutcomeDto, AppRunSnapshotDto, AppRunStateDto, AppTurnExecutionDto,
    AppTurnModeDto, AppTurnReportDto, AppWireErrorCodeDto, AppWireErrorDto,
    CalendarAccessChangeDto, CalendarAccessConfigurationDto, CalendarActionDecisionDto,
    CalendarActionOperationDto, CalendarActionRequestDto, CalendarBatchDto, CalendarConnectionDto,
    CalendarExpertOverviewDto, CalendarExpertSetupDto, CalendarFailureDto, CalendarProviderDto,
    CalendarRangeDto, CalendarRecordDto, CalendarScopeDto, CalendarSelectionDto, CalendarSourceDto,
    CalendarSubjectPreviewDto, CalendarSubjectPreviewRequestDto, CalendarSyncStatusDto, CaptureDto,
    CaptureProcessingDto, CaptureSourceDto, ClassificationDto, CommandDto, CommandRequestDto,
    ConnectorSnapshotDto, ContactsAccessChangeDto, ContactsAccessConfigurationDto, DayQueryDto,
    DaySnapshotDto, DomainRefDto, EpistemicStatusDto, ErrorCodeDto, ErrorDto, EventDto,
    EventScheduleDto, FeasibilityGrantQueryDto, KnowledgeCandidateDto, KnowledgeDecisionResultDto,
    LoadDayRequestDto, LocalContextAcquisitionModeDto, LocalContextAcquisitionRequestDto,
    LocalContextAcquisitionResultDto, LocalContextAttentionAcquisitionModeDto,
    LocalContextAttentionAcquisitionRequestDto, LocalContextAttentionAcquisitionResultDto,
    LocalContextOperationDto, LocalContextPersonalAcquisitionRequestDto,
    LocalContextPersonalAcquisitionResultDto, LocalContextPersonalDomainDto,
    LocalContextRequestDto, LocalContextResultDto, MutationResultDto, NoteDto, PROTOCOL_VERSION,
    PersonalAccessChangeDto, PersonalAccessConfigurationDto, PersonalAccessOverviewDto,
    PersonalMemoryKindDto, PriorityDto, RegistryConfigurationDto, RegistryConfigurationTargetDto,
    RegistryOverviewDto, RemoteAuthorityEnrollmentStatusDto, RemoteCalendarGrantOverviewDto,
    RemoteCalendarGrantPreviewDto, RemoteOwnerPublicKeyDto, RemotePairingChallengeDto,
    RemotePairingConfirmationDto, RemotePairingStatusDto, RemoteProducerIdentityDto,
    RemoteViewGrantOverviewDto, RemoteViewGrantPreviewDto, ResponseEnvelopeDto, ResponseOutcomeDto,
    SourceRefDto, TaskDto, TimelineItemDto,
};

/// The shared identity values this wire carries.
///
/// A binding names them to build one command or read one result; naming a
/// value here is not a way to reach past the wire into the module that owns it.
pub use floe_kernel::{AgentFailure, CaptureId, EventId, NoteId, PersonId, Revision, TaskId};
