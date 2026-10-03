mod actions;
pub use actions::ActionOperationResultDto;
mod connections;
pub use connections::*;
mod agent;
mod calendar;
mod commands;
mod conversation;
pub use conversation::*;
mod day;
mod day_mutation;
pub use day_mutation::DayMutationDto;
pub use day::DayRefreshStateDto;
mod envelope;
mod errors;
mod events;
mod experts;
mod interactions;
pub use experts::{
    ExpertBindingSelectionDto, ExpertCandidateCatalogDto, ExpertOperationResultDto,
    ExpertSourceCandidateDto,
};
mod local_context;
mod native_host;
mod refs;
pub use native_host::{
    AttentionCompletionDto, CalendarCompletionDto, NativeHostCommandDto, NativeHostQueryDto,
    NativeHostRegistrationDto, PersonalCompletionDto,
};
pub use refs::{
    ActionRefDto, AssignmentRefDto, AttemptRefDto, CommandIdDto, DigestHex64Dto, GatewayRefDto,
    ConnectionsSourceRefDto, GatewaySetupRefDto, InteractionRefDto, IntegrationRefDto,
    LaunchActionRefDto, MessageRefDto, OperationRefDto, RequestIdDto, ResourceRefDto,
    ReviewRefDto, RunRefDto, SessionRefDto, TaskRefDto, UuidRefDto,
};
mod knowledge;
pub use knowledge::KnowledgeOperationResultDto;
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
    AgentVaultFailureDto, AgentVaultRecoveryActionDto, AgentVaultStateDto,
    CalendarActionDecisionDto, CalendarActionOperationDto, ConnectorSnapshotDto,
    EpistemicStatusDto, KnowledgeCandidateDto, KnowledgeDecisionResultDto, PersonalMemoryKindDto,
    RegistryConfigurationDto, RegistryConfigurationTargetDto, RegistryOverviewDto,
};
pub use calendar::{
    CalendarFailureDto, CalendarMirrorStateDto, CalendarProviderDto, CalendarRangeDto,
    CalendarScopeDto, CalendarSelectionDto, CalendarSourceDto, CalendarSyncStatusDto,
};
pub use commands::{
    AppCancelRunOutcomeDto, AppCommandDto, AppCommandReceiptDto, AppCommandRequestDto,
    AppCommandResultDto, AppCommandStatusDto, AppProductCommandDto,
    ContinuationRefDto,
};
pub use day::{
    CalendarBatchDto, CalendarRecordDto, CaptureDto, CaptureProcessingDto, CaptureSourceDto,
    ClassificationDto, DayQueryDto, DaySnapshotDto, DomainRefDto, EventDto, EventScheduleDto,
    MutationResultDto, NoteDto, PriorityDto, SourceRefDto, TaskDto, TimelineItemDto,
};
pub use envelope::{ErrorCodeDto, ErrorDto, ResponseEnvelopeDto, ResponseOutcomeDto};
pub use errors::{AppResponseDto, AppResponseOutcomeDto, AppWireErrorCodeDto, AppWireErrorDto, OwnerFailureDto, OwnerRecoveryDto};
pub use events::{AppEventDto, AppEventKindDto, AppEventsRequestDto, AppEventsResultDto};
pub use interactions::{
    AppInteractionActionDto, AppInteractionDecisionDto, AppInteractionKindDto,
    AppInteractionListDto, AppInteractionRefreshOutcomeDto, AppInteractionRefreshResultDto,
    AppInteractionResolveOutcomeDto, AppInteractionResolveResultDto, AppInteractionSnapshotDto,
    AppInteractionStateDto, AppInteractionTargetDto, AppNavigationDestinationDto, MAX_INTERACTIONS_PER_LIST,
};
pub use local_context::{
    LocalContextAcquisitionModeDto, LocalContextAcquisitionRequestDto,
    LocalContextAttentionAcquisitionModeDto, LocalContextAttentionAcquisitionRequestDto,
    LocalContextPersonalAcquisitionRequestDto, LocalContextPersonalDomainDto,
    LocalContextPersonalAcquisitionModeDto, NativeSourceResourceDto,
};
pub use queries::{
    AppMessageDto, AppMessageRoleDto, AppProductQueryDto, AppQueryDto, AppQueryRequestDto,
    AppQueryResultDto, AppReplyStatusDto, AppRunSnapshotDto, AppRunStateDto,
    AppTurnExecutionDto, AppTurnReportDto,
};
