use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentVaultRequestDto {
    pub schema_version: u32,
    pub person_id: String,
    pub request_id: String,
    pub operation: AgentVaultOperationDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentVaultOperationDto {
    Submit { action: AgentVaultActionDto },
    Poll { after_sequence: usize },
    Stop {},
    Release {},
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentVaultActionDto {
    Status {},
    Create {},
    Unlock {},
    Lock {},
    Session {
        operation: AgentFixtureOperationDto,
    },
    Registry {
        change: Option<floe_agent::RegistryConfiguration>,
    },
    CalendarExperts {
        setup: Option<floe_agent::CalendarExpertSetup>,
    },
    CalendarAccess {
        change: floe_agent::CalendarAccessConfiguration,
    },
    InspectProposal {
        session_id: String,
        invocation_id: String,
    },
    CalendarSession {
        operation: AgentCalendarSessionOperationDto,
    },
    CalendarTurn {
        request: AgentCalendarTurnRequestDto,
    },
    ConversationSession {
        operation: AgentConversationSessionOperationDto,
    },
    ConversationTurn {
        request: AgentConversationTurnRequestDto,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentConversationSessionOperationDto {
    Start {},
    Resume {},
    Get {
        session_id: String,
    },
    Recover {
        session_id: String,
        expected_revision: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConversationTurnRequestDto {
    pub session_id: String,
    pub expected_revision: u64,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_route: Option<AgentRemoteRouteDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentCalendarSessionOperationDto {
    Start {
        setup_id: String,
    },
    Resume {
        setup_id: String,
    },
    Get {
        session_id: String,
    },
    Recover {
        session_id: String,
        expected_revision: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCalendarTurnRequestDto {
    pub session_id: String,
    pub expected_revision: u64,
    pub prompt: AgentCalendarPromptDto,
    pub model: AgentCalendarModelDto,
    pub day: floe_domain::CalendarRange,
    pub starts_at: chrono::DateTime<chrono::Utc>,
    pub ends_at: chrono::DateTime<chrono::Utc>,
    pub destination: Option<AgentCalendarDestinationDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_route: Option<AgentRemoteRouteDto>,
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentRemoteRouteDto {
    pub base_url: String,
    pub bearer_token: String,
    pub purpose: String,
    pub external: bool,
    pub allow_external: bool,
}

impl std::fmt::Debug for AgentRemoteRouteDto {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AgentRemoteRouteDto")
            .field("base_url", &self.base_url)
            .field("bearer_token", &"[REDACTED]")
            .field("purpose", &self.purpose)
            .field("external", &self.external)
            .field("allow_external", &self.allow_external)
            .finish()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentCalendarPromptDto {
    Briefing { focus_minutes: u16 },
    ProposeFocus { focus_minutes: u16 },
    FreeText { text: String },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCalendarModelDto {
    DeterministicFixture,
    FoundationModels,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCalendarDestinationDto {
    pub provider: floe_domain::CalendarProvider,
    pub calendar_id: String,
    pub connection_revision: u64,
    pub timezone: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCalendarTurnResultDto {
    pub schema_version: u32,
    pub person_id: String,
    pub session_id: String,
    pub setup_id: String,
    pub model: AgentCalendarModelDto,
    pub proposals: Vec<AgentCalendarProposalOutcomeDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCalendarProposalOutcomeDto {
    pub invocation_id: String,
    pub action: Option<AgentProposalActionDto>,
    pub failure: Option<floe_agent::AgentFailure>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentProposalInspectionDto {
    pub schema_version: u32,
    pub person_id: String,
    pub session_id: String,
    pub invocation_id: String,
    pub action: Option<AgentProposalActionDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentProposalActionDto {
    pub action_id: String,
    pub execution_id: String,
    pub status: AgentProposalStatusDto,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProposalStatusDto {
    Pending,
    Approved,
    Rejected,
    Executing,
    Blocked,
    Unknown,
    Succeeded,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentVaultStateDto {
    Missing,
    Locked,
    Ready,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentVaultResultDto {
    pub request_id: String,
    pub events: Vec<floe_agent::AgentEvent>,
    pub next_sequence: usize,
    pub done: bool,
    pub state: Option<AgentVaultStateDto>,
    pub session: Option<floe_agent::AgentSession>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<floe_agent::RegistryOverview>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_experts: Option<floe_agent::CalendarExpertOverview>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_turn: Option<AgentCalendarTurnResultDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<AgentProposalInspectionDto>,
    pub failure: Option<floe_agent::AgentFailure>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentFixtureRequestDto {
    pub schema_version: u32,
    pub person_id: String,
    pub operation: AgentFixtureOperationDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentFixtureOperationDto {
    Start {},
    Resume {},
    Get {
        session_id: String,
    },
    Turn {
        session_id: String,
        expected_revision: u64,
        prompt: AgentFixturePromptDto,
    },
    Recover {
        session_id: String,
        expected_revision: u64,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentFixturePromptDto {
    Today,
    FollowUp,
    RepeatedCall,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentFixtureRunRequestDto {
    pub schema_version: u32,
    pub person_id: String,
    pub session_id: String,
    pub expected_revision: u64,
    pub operation: AgentFixtureRunOperationDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentFixtureRunOperationDto {
    Begin { prompt: AgentFixturePromptDto },
    Poll { after_sequence: usize },
    Stop {},
    Release {},
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentFixtureRunDto {
    pub session_id: String,
    pub expected_revision: u64,
    pub events: Vec<floe_agent::AgentEvent>,
    pub next_sequence: usize,
    pub done: bool,
    pub session: Option<floe_agent::AgentSession>,
    pub failure: Option<floe_agent::AgentFailure>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentFixtureResultDto {
    pub session: floe_agent::AgentSession,
    pub events: Vec<floe_agent::AgentEvent>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarActionRequestDto {
    pub schema_version: u32,
    pub person_id: String,
    pub operation: CalendarActionOperationDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CalendarActionOperationDto {
    Capabilities {},
    GetAuthority {},
    SetAuthority {
        calendar_create: ActionAuthorityModeDto,
    },
    Execute {
        action_id: String,
    },
    Recover {
        action_id: String,
    },
    List {},
    Get {
        action_id: String,
    },
    Propose {
        calendar_id: String,
        title: String,
        starts_at: String,
        ends_at: String,
        timezone: String,
    },
    Direct {
        calendar_id: String,
        title: String,
        starts_at: String,
        ends_at: String,
        timezone: String,
        event_id: Option<String>,
        event_revision: Option<u64>,
        delete: bool,
    },
    Decide {
        action_id: String,
        decision: CalendarActionDecisionDto,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionAuthorityModeDto {
    Allow,
    Ask,
    Deny,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarActionDecisionDto {
    Approve,
    Reject,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DayQueryDto {
    pub date: String,
    pub timezone_offset_seconds: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_timezone_offset_seconds: Option<i32>,
    pub now: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LoadDayRequestDto {
    pub schema_version: u32,
    pub person_id: String,
    pub day: DayQueryDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DaySnapshotDto {
    pub schema_version: u32,
    pub person_id: String,
    pub date: String,
    pub generated_at: String,
    pub timezone_offset_seconds: i32,
    pub now_event_id: Option<String>,
    pub next_event_id: Option<String>,
    pub overdue_task_count: u32,
    pub items: Vec<TimelineItemDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar: Option<floe_domain::CalendarConnection>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TimelineItemDto {
    Event(EventDto),
    Task(TaskDto),
    Note(NoteDto),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EventDto {
    pub id: String,
    pub person_id: String,
    pub title: String,
    pub schedule: EventScheduleDto,
    pub source: SourceRefDto,
    pub created_at: String,
    pub updated_at: String,
    pub revision: u64,
    pub deleted_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventScheduleDto {
    Timed {
        starts_at: String,
        ends_at: String,
        timezone: String,
    },
    AllDay {
        start_date: String,
        end_date_exclusive: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceRefDto {
    Manual,
    Capture { capture_id: String },
    Calendar { source: floe_domain::CalendarSource },
    External { source: floe_domain::ExternalSource },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityDto {
    Low,
    Normal,
    High,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TaskDto {
    pub id: String,
    pub person_id: String,
    pub title: String,
    pub deadline: Option<String>,
    pub priority: PriorityDto,
    pub completed_at: Option<String>,
    pub source: SourceRefDto,
    pub created_at: String,
    pub updated_at: String,
    pub revision: u64,
    pub deleted_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct NoteDto {
    pub id: String,
    pub person_id: String,
    pub content: String,
    pub source: SourceRefDto,
    pub created_at: String,
    pub updated_at: String,
    pub revision: u64,
    pub deleted_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CaptureDto {
    pub id: String,
    pub person_id: String,
    pub original_input: String,
    pub captured_at: String,
    pub source: CaptureSourceDto,
    pub processing: CaptureProcessingDto,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureSourceDto {
    Typed,
    Voice,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CaptureProcessingDto {
    Pending,
    Classified {
        target: DomainRefDto,
        classified_at: String,
    },
    Dismissed {
        dismissed_at: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DomainRefDto {
    Event { id: String },
    Task { id: String },
    Note { id: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommandRequestDto {
    pub schema_version: u32,
    pub person_id: String,
    pub day: DayQueryDto,
    pub command: CommandDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarRecordDto {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub can_modify: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calendar_id: Option<String>,
    pub external_id: String,
    pub external_revision: String,
    pub title: String,
    pub schedule: EventScheduleDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalendarBatchDto {
    pub calendar_id: String,
    pub records: Vec<CalendarRecordDto>,
    pub failure: Option<floe_domain::CalendarFailure>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommandDto {
    DisconnectCalendar {
        expected_revision: u64,
    },
    SetCalendarScope {
        provider: floe_domain::CalendarProvider,
        calendars: Vec<floe_domain::CalendarSelection>,
        scope: floe_domain::CalendarScope,
    },
    DiscoverCalendars {
        expected_revision: u64,
        calendars: Vec<floe_domain::CalendarSelection>,
    },
    ImportCalendarSources {
        expected_revision: u64,
        range: floe_domain::CalendarRange,
        batches: Vec<CalendarBatchDto>,
        occurred_at: String,
    },
    SelectCalendars {
        provider: floe_domain::CalendarProvider,
        calendars: Vec<floe_domain::CalendarSelection>,
    },
    SelectCalendar {
        provider: floe_domain::CalendarProvider,
        calendar_id: String,
        calendar_name: String,
    },
    ImportCalendar {
        expected_revision: u64,
        range: floe_domain::CalendarRange,
        records: Vec<CalendarRecordDto>,
        occurred_at: String,
    },
    CalendarFailed {
        expected_revision: u64,
        failure: floe_domain::CalendarFailure,
    },
    SubmitCapture {
        input: String,
        occurred_at: String,
    },
    ClassifyCapture {
        capture_id: String,
        expected_revision: u64,
        classification: ClassificationDto,
        occurred_at: String,
    },
    CreateEvent {
        title: String,
        schedule: EventScheduleDto,
        occurred_at: String,
    },
    CreateTask {
        title: String,
        deadline: Option<String>,
        priority: PriorityDto,
        occurred_at: String,
    },
    CreateNote {
        content: String,
        occurred_at: String,
    },
    UpdateEvent {
        event_id: String,
        expected_revision: u64,
        title: String,
        schedule: EventScheduleDto,
        occurred_at: String,
    },
    UpdateTask {
        task_id: String,
        expected_revision: u64,
        title: String,
        deadline: Option<String>,
        priority: PriorityDto,
        occurred_at: String,
    },
    UpdateNote {
        note_id: String,
        expected_revision: u64,
        content: String,
        occurred_at: String,
    },
    SetTaskCompletion {
        task_id: String,
        expected_revision: u64,
        completed: bool,
        occurred_at: String,
    },
    DeleteItem {
        target: DomainRefDto,
        expected_revision: u64,
        occurred_at: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClassificationDto {
    Event {
        title: String,
        schedule: EventScheduleDto,
    },
    Task {
        title: String,
        deadline: Option<String>,
        priority: PriorityDto,
    },
    Note {
        content: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MutationResultDto {
    pub snapshot: DaySnapshotDto,
    pub changed_item: Option<TimelineItemDto>,
    pub capture: Option<CaptureDto>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCodeDto {
    Validation,
    NotFound,
    Conflict,
    Storage,
    Internal,
    UnsupportedVersion,
    NoFocusSlot,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ErrorDto {
    pub code: ErrorCodeDto,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ResponseEnvelopeDto<T> {
    pub schema_version: u32,
    #[serde(flatten)]
    pub outcome: ResponseOutcomeDto<T>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ResponseOutcomeDto<T> {
    Ok { data: T },
    Error { error: ErrorDto },
}

impl<T> ResponseEnvelopeDto<T> {
    pub fn ok(data: T) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION,
            outcome: ResponseOutcomeDto::Ok { data },
        }
    }

    pub fn error(error: ErrorDto) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION,
            outcome: ResponseOutcomeDto::Error { error },
        }
    }
}
