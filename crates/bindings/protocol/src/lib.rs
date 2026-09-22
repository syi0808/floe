pub mod conversion;
mod dto;
pub mod wire;

pub use dto::{
    APP_WIRE_VERSION, ActionAuthorityModeDto, ActionOperationResultDto,
    AgentConversationSessionOperationDto, AgentEventDto, AgentFailureCategory, AgentFailureDomain,
    AgentFailureDto, AgentFailureSafeAction, AgentFixtureOperationDto, AgentFixturePromptDto,
    AgentFixtureRequestDto, AgentFixtureResultDto, AgentFixtureRunDto, AgentFixtureRunOperationDto,
    AgentFixtureRunRequestDto, AgentMemoryOriginDto, AgentMemoryOverviewDto,
    AgentMemoryReviewDecisionDto, AgentMemoryReviewDecisionKindDto, AgentMemoryReviewOverviewDto,
    AgentMemorySummaryDto, AgentProposalActionDto, AgentProposalInspectionDto,
    AgentProposalStatusDto, AgentRetryPolicy, AgentSessionDto, AgentVaultActionDto,
    AgentVaultFailureDto, AgentVaultOperationDto, AgentVaultRecoveryActionDto,
    AgentVaultRequestDto, AgentVaultResultDto, AgentVaultStateDto, AppCancelRunOutcomeDto,
    AppCancelRunReasonDto, AppCommandDto, AppCommandReceiptDto, AppCommandRequestDto,
    AppCommandResultDto, AppCommandStatusDto, AppContinuationRefDto, AppEventDto, AppEventKindDto,
    AppEventsRequestDto, AppEventsResultDto, AppMessageDto, AppMessageRoleDto,
    AppProfileSelectionDto, AppQueryDto, AppQueryRequestDto, AppQueryResultDto, AppReplyStatusDto,
    AppResponseDto, AppResponseOutcomeDto, AppRunSnapshotDto, AppRunStateDto, AppTurnExecutionDto,
    AppTurnModeDto, AppTurnReportDto, AppWireErrorCodeDto, AppWireErrorDto, AttentionCompletionDto,
    CalendarAccessChangeDto, CalendarAccessConfigurationDto, CalendarActionDecisionDto,
    CalendarActionOperationDto, CalendarActionRequestDto, CalendarBatchDto, CalendarCompletionDto,
    CalendarConnectionDto, CalendarExpertInstallDto, CalendarExpertOverviewDto,
    CalendarExpertSetupDto, CalendarFailureDto, CalendarGrantChangeDto,
    CalendarGrantConfigurationDto, CalendarProviderDto, CalendarRangeDto, CalendarRecordDto,
    CalendarScopeDto, CalendarSelectionDto, CalendarSourceDto, CalendarSubjectIntentDto,
    CalendarSubjectPreviewDto, CalendarSubjectPreviewRequestDto, CalendarSyncStatusDto, CaptureDto,
    CaptureProcessingDto, CaptureSourceDto, ClassificationDto, CommandDto, CommandRequestDto,
    ConnectionsResultDto, ConnectorSnapshotDto, ContactsAccessChangeDto,
    ContactsAccessConfigurationDto, ContextCommandDto, ContextQueryDto,
    ConversationSessionResultDto, DayMutationDto, DayQueryDto, DaySnapshotDto, DomainRefDto,
    EpistemicStatusDto, ErrorCodeDto, ErrorDto, EventDto, EventScheduleDto,
    ExpertOperationResultDto, FeasibilityGrantQueryDto, KnowledgeCandidateDto,
    KnowledgeDecisionResultDto, KnowledgeOperationResultDto, LoadDayRequestDto,
    LocalAccessResultDto, LocalContextAcquisitionModeDto, LocalContextAcquisitionRequestDto,
    LocalContextAcquisitionResultDto, LocalContextAttentionAcquisitionModeDto,
    LocalContextAttentionAcquisitionRequestDto, LocalContextAttentionAcquisitionResultDto,
    LocalContextOperationDto, LocalContextPersonalAcquisitionRequestDto,
    LocalContextPersonalAcquisitionResultDto, LocalContextPersonalDomainDto,
    LocalContextRequestDto, LocalContextResultDto, MutationResultDto, NoteDto, PROTOCOL_VERSION,
    PairingOutcomeDto, PairingReportDto, PairingTargetDto, PersonalAccessChangeDto,
    PersonalAccessConfigurationDto, PersonalAccessOverviewDto, PersonalCompletionDto,
    PersonalMemoryKindDto, PriorityDto, RegistryConfigurationDto, RegistryConfigurationTargetDto,
    RegistryOverviewDto, RemoteAccessOperationDto, RemoteAccessRequestDto, RemoteAccessResultDto,
    RemoteAuthorityEnrollmentStatusDto, RemoteCalendarGrantOverviewDto,
    RemoteCalendarGrantPreviewDto, RemoteOwnerPublicKeyDto, RemotePairingChallengeDto,
    RemotePairingOperationDto, RemotePairingRequestDto, RemotePairingResultDto,
    RemoteProducerIdentityDto, RemoteViewGrantOverviewDto, RemoteViewGrantPreviewDto,
    ResponseEnvelopeDto, ResponseOutcomeDto, SourceRefDto, TaskDto, TimelineItemDto,
    VaultLifecycleResultDto,
};

/// The shared identity values this wire carries.
///
/// A binding names them to build one command or read one result; naming a
/// value here is not a way to reach past the wire into the module that owns it.
pub use floe_kernel::{AgentFailure, CaptureId, EventId, NoteId, PersonId, Revision, TaskId};
