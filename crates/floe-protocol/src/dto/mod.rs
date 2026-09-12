mod agent;
mod day;
mod envelope;
mod local_context;

pub const PROTOCOL_VERSION: u32 = 1;

pub use agent::{
    ActionAuthorityModeDto, AgentConversationSessionOperationDto, AgentConversationTurnRequestDto,
    AgentEventDto, AgentFailureDto, AgentFixtureOperationDto, AgentFixturePromptDto,
    AgentFixtureRequestDto, AgentFixtureResultDto, AgentFixtureRunDto, AgentFixtureRunOperationDto,
    AgentFixtureRunRequestDto, AgentMemoryOriginDto, AgentMemoryOverviewDto,
    AgentMemoryReviewDecisionDto, AgentMemoryReviewDecisionKindDto, AgentMemoryReviewOverviewDto,
    AgentMemorySummaryDto, AgentProposalActionDto, AgentProposalInspectionDto,
    AgentProposalStatusDto, AgentRemoteCalendarConnectionDto, AgentRemotePairingDto,
    AgentRemoteRouteDto, AgentSessionDto, AgentVaultActionDto, AgentVaultFailureDto,
    AgentVaultOperationDto, AgentVaultRecoveryActionDto, AgentVaultRequestDto, AgentVaultResultDto,
    AgentVaultStateDto, CalendarAccessChangeDto, CalendarAccessConfigurationDto,
    CalendarActionDecisionDto, CalendarActionOperationDto, CalendarActionRequestDto,
    CalendarExpertOverviewDto, CalendarExpertSetupDto, CalendarSubjectPreviewDto,
    CalendarSubjectPreviewRequestDto, ConnectorSnapshotDto, ContactsAccessChangeDto,
    ContactsAccessConfigurationDto, EpistemicStatusDto, FeasibilityGrantQueryDto,
    KnowledgeCandidateDto, KnowledgeDecisionResultDto, PersonalAccessChangeDto,
    PersonalAccessConfigurationDto, PersonalAccessOverviewDto, PersonalMemoryKindDto,
    RegistryConfigurationDto, RegistryConfigurationTargetDto, RegistryOverviewDto,
    RemoteAuthorityEnrollmentStatusDto, RemoteCalendarGrantOverviewDto,
    RemoteCalendarGrantPreviewDto, RemoteOwnerPublicKeyDto, RemotePairingChallengeDto,
    RemotePairingConfirmationDto, RemotePairingStatusDto, RemoteProducerIdentityDto,
    RemoteViewGrantOverviewDto, RemoteViewGrantPreviewDto,
};
pub use day::{
    CalendarBatchDto, CalendarRecordDto, CaptureDto, CaptureProcessingDto, CaptureSourceDto,
    ClassificationDto, CommandDto, CommandRequestDto, DayQueryDto, DaySnapshotDto, DomainRefDto,
    EventDto, EventScheduleDto, LoadDayRequestDto, MutationResultDto, NoteDto, PriorityDto,
    SourceRefDto, TaskDto, TimelineItemDto,
};
pub use envelope::{ErrorCodeDto, ErrorDto, ResponseEnvelopeDto, ResponseOutcomeDto};
pub use local_context::{
    LocalContextAcquisitionModeDto, LocalContextAcquisitionRequestDto,
    LocalContextAcquisitionResultDto, LocalContextAttentionAcquisitionModeDto,
    LocalContextAttentionAcquisitionRequestDto, LocalContextAttentionAcquisitionResultDto,
    LocalContextOperationDto, LocalContextPersonalAcquisitionRequestDto,
    LocalContextPersonalAcquisitionResultDto, LocalContextPersonalDomainDto,
    LocalContextRequestDto, LocalContextResultDto,
};
