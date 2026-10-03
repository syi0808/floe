mod actions;
pub use actions::*;
mod connections;
pub use connections::*;
mod agent;
mod calendar;
mod commands;
mod conversation;
pub use conversation::*;
mod day;
mod day_mutation;
pub use day::*;
pub use day_mutation::DayMutationDto;
mod envelope;
mod errors;
mod events;
mod experts;
mod interactions;
pub use experts::*;
mod local_context;
mod native_host;
mod refs;
pub use native_host::{
    AttentionCompletionDto, CalendarCompletionDto, NativeCalendarBatchDto, NativeCalendarRecordDto,
    NativeEventScheduleDto, NativeHostCommandDto, NativeHostQueryDto, NativeHostRegistrationDto,
    PersonalCompletionDto,
};
pub use refs::{
    ActionRefDto, AssignmentRefDto, AttemptRefDto, CommandIdDto, ConnectionsSourceRefDto,
    DigestHex64Dto, GatewayRefDto, GatewaySetupRefDto, IntegrationRefDto, InteractionRefDto,
    LaunchActionRefDto, MessageRefDto, OperationRefDto, RequestIdDto, ResourceRefDto, ReviewRefDto,
    RunRefDto, SessionRefDto, TaskRefDto, UuidRefDto,
};
mod knowledge;
pub use knowledge::*;
mod queries;
mod vault;
pub use vault::VaultLifecycleResultDto;

pub const PROTOCOL_VERSION: u32 = 1;
pub const APP_WIRE_VERSION: u32 = 2;

pub use agent::{
    ActionAuthorityModeDto, AgentFailureCategory, AgentFailureDomain, AgentFailureSafeAction,
    AgentMemoryOriginDto, AgentMemoryOverviewDto, AgentMemoryReviewDecisionKindDto,
    AgentMemorySummaryDto, AgentRetryPolicy, AgentVaultFailureDto, AgentVaultRecoveryActionDto,
    AgentVaultStateDto,
};
pub use calendar::{
    CalendarFailureDto, CalendarProviderDto, CalendarRangeDto, CalendarScopeDto,
    CalendarSelectionDto,
};
pub use commands::{
    AppCancelRunOutcomeDto, AppCommandDto, AppCommandReceiptDto, AppCommandRequestDto,
    AppCommandResultDto, AppCommandStatusDto, AppProductCommandDto, ContinuationRefDto,
};

pub use envelope::{ErrorCodeDto, ErrorDto, ResponseEnvelopeDto, ResponseOutcomeDto};
pub use errors::{
    AppResponseDto, AppResponseOutcomeDto, AppWireErrorCodeDto, AppWireErrorDto, OwnerFailureDto,
    OwnerRecoveryDto,
};
pub use events::{AppEventDto, AppEventKindDto, AppEventsRequestDto, AppEventsResultDto};
pub use interactions::{
    AppInteractionActionDto, AppInteractionDecisionDto, AppInteractionKindDto,
    AppInteractionListDto, AppInteractionRefreshOutcomeDto, AppInteractionRefreshResultDto,
    AppInteractionResolveOutcomeDto, AppInteractionResolveResultDto, AppInteractionSnapshotDto,
    AppInteractionStateDto, AppInteractionTargetDto, AppNavigationDestinationDto,
    MAX_INTERACTIONS_PER_LIST,
};
pub use local_context::{
    LocalContextAcquisitionModeDto, LocalContextAcquisitionRequestDto,
    LocalContextAttentionAcquisitionModeDto, LocalContextAttentionAcquisitionRequestDto,
    LocalContextPersonalAcquisitionModeDto, LocalContextPersonalAcquisitionRequestDto,
    LocalContextPersonalDomainDto, NativeSourceResourceDto,
};
pub use queries::{
    AppMessageDto, AppMessageRoleDto, AppProductQueryDto, AppQueryDto, AppQueryRequestDto,
    AppQueryResultDto, AppReplyStatusDto, AppRunSnapshotDto, AppRunStateDto, AppTurnExecutionDto,
    AppTurnReportDto,
};
