pub mod conversion;
mod dto;
pub mod wire;

pub use dto::{
    APP_WIRE_VERSION, ActionAuthorityModeDto, ActionOperationResultDto, AgentEventDto,
    AgentFailureCategory, AgentFailureDomain, AgentFailureDto, AgentFailureSafeAction,
    AgentMemoryOriginDto, AgentMemoryOverviewDto, AgentMemoryReviewDecisionKindDto,
    AgentMemoryReviewOverviewDto, AgentMemorySummaryDto, AgentProposalActionDto,
    AgentProposalInspectionDto, AgentProposalStatusDto, AgentRetryPolicy, AgentSessionDto,
    AgentVaultFailureDto, AgentVaultRecoveryActionDto, AgentVaultStateDto, AppCancelRunOutcomeDto,
    AppCancelRunReasonDto, AppCommandDto, AppCommandReceiptDto, AppCommandRequestDto,
    AppCommandResultDto, AppCommandStatusDto, AppConsentScopeDto, AppContinuationRefDto,
    AppEventDto, AppEventKindDto, AppEventsRequestDto, AppEventsResultDto, AppInteractionActionDto,
    AppInteractionDecisionDto, AppInteractionKindDto, AppInteractionListDto,
    AppInteractionRefreshOutcomeDto, AppInteractionRefreshResultDto,
    AppInteractionResolveOutcomeDto, AppInteractionResolveResultDto, AppInteractionSnapshotDto,
    AppInteractionStateDto, AppInteractionTargetDto, AppMessageDto, AppMessageRoleDto,
    AppNavigationDestinationDto, AppObservedMemberDto, AppProfileSelectionDto, AppQueryDto,
    AppQueryRequestDto, AppQueryResultDto, AppReplyStatusDto, AppResponseDto,
    AppResponseOutcomeDto, AppRunSnapshotDto, AppRunStateDto, AppTurnExecutionDto, AppTurnModeDto,
    AppTurnReportDto, AppWireErrorCodeDto, AppWireErrorDto, AttentionCompletionDto,
    CalendarAccessChangeDto, CalendarAccessOverviewDto, CalendarActionDecisionDto,
    CalendarActionOperationDto, CalendarBatchDto, CalendarCompletionDto, CalendarConnectionDto,
    CalendarFailureDto, CalendarProviderDto, CalendarRangeDto, CalendarRecordDto, CalendarScopeDto,
    CalendarSelectionDto, CalendarSourceDto, CalendarSubjectIntentDto, CalendarSubjectPreviewDto,
    CalendarSyncStatusDto, CaptureDto, CaptureProcessingDto, CaptureSourceDto, ClassificationDto,
    ConnectionObserveExpectationDto, ConnectionObserveMemberDto, ConnectionsResultDto,
    ConnectorSnapshotDto, ContactsAccessChangeDto, ContextCommandDto, ContextQueryDto,
    ConversationSessionResultDto, DayMutationDto, DayQueryDto, DaySnapshotDto, DomainRefDto,
    EpistemicStatusDto, ErrorCodeDto, ErrorDto, EventDto, EventScheduleDto,
    ExpertBindingSelectionDto, ExpertCandidateCatalogDto, ExpertOperationResultDto,
    ExpertSourceCandidateDto, FeasibilityGrantQueryDto, KnowledgeCandidateDto,
    KnowledgeDecisionResultDto, KnowledgeOperationResultDto, LocalAccessResultDto,
    LocalContextAcquisitionModeDto, LocalContextAcquisitionRequestDto,
    LocalContextAttentionAcquisitionModeDto, LocalContextAttentionAcquisitionRequestDto,
    LocalContextPersonalAcquisitionRequestDto, LocalContextPersonalDomainDto,
    LocalContextResultDto, MAX_INTERACTIONS_PER_LIST, MutationResultDto, NoteDto, PROTOCOL_VERSION,
    PairingOutcomeDto, PairingReportDto, PairingTargetDto, PersonalAccessChangeDto,
    PersonalAccessOverviewDto, PersonalCompletionDto, PersonalMemoryKindDto, PriorityDto,
    RegistryConfigurationDto, RegistryConfigurationTargetDto, RegistryOverviewDto,
    RemoteAccessOperationDto, RemoteAccessRequestDto, RemoteAccessResultDto,
    RemoteAuthorityEnrollmentStatusDto, RemoteOwnerPublicKeyDto, RemotePairingChallengeDto,
    RemotePairingOperationDto, RemotePairingRequestDto, RemotePairingResultDto,
    RemoteProducerIdentityDto, ResponseEnvelopeDto, ResponseOutcomeDto, SourceRefDto, TaskDto,
    TimelineItemDto, VaultLifecycleResultDto,
};

/// The shared identity values this wire carries.
///
/// A binding names them to build one command or read one result; naming a
/// value here is not a way to reach past the wire into the module that owns it.
pub use floe_kernel::{AgentFailure, CaptureId, EventId, NoteId, PersonId, Revision, TaskId};
