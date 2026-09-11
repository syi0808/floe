use floe_agent::{
    AgentBudget, AgentEvent, AgentFailure, DataClass, InferencePolicyDecision, ModelPlacement,
    ModelRequest, ModelResponse, ModelRunner, SessionStore, TransferConsent,
};
use floe_core::{
    CalendarAgentTurnRequest, CalendarReadAccess, CalendarReadAccessRequest,
    CalendarReadAccessStamp, CalendarTimelineGrant, EncryptedAgentVault, FloeCore,
    VaultKeyProvider,
};
use floe_domain::{CalendarProvider, PersonId};
use floe_protocol::{AgentConversationTurnRequestDto, PROTOCOL_VERSION};

use crate::{
    local_context::LocalContextStore, local_model::FoundationModelRunner,
    native_calendar::NativeCalendar, remote_model::ServerModelRunner,
};

use super::session_uuid;

pub(super) async fn try_run_with_schedule_expert<
    Keys: VaultKeyProvider,
    Emit: FnMut(AgentEvent) + Send,
>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    local_context: &LocalContextStore,
    person_id: PersonId,
    request: &AgentConversationTurnRequestDto,
    context: floe_agent::AgentContext,
    cancellation: floe_agent::Cancellation,
    emit: &mut Emit,
) -> Result<Option<floe_agent::AgentSession>, AgentFailure> {
    let session_id = session_uuid(&request.session_id)?;
    let session = vault.load(person_id, session_id).await?;
    if session.scope.is_some()
        || session.data_classes != [DataClass::Personal]
        || session.revision != request.expected_revision
    {
        return Err(AgentFailure::Conflict);
    }
    let overview = vault.calendar_expert_overview().await?;
    let Some((setup, binding)) = overview.setups.iter().find_map(|setup| {
        let binding = overview
            .views
            .iter()
            .find(|binding| binding.handle == setup.view_handle && binding.enabled)?;
        let installations_enabled = [setup.tool_installation_id, setup.expert_installation_id]
            .iter()
            .all(|id| {
                overview
                    .registry
                    .installations
                    .iter()
                    .any(|entry| entry.id == *id && entry.enabled)
            });
        let assignments_enabled = [setup.tool_assignment_id, setup.expert_assignment_id]
            .iter()
            .all(|id| {
                overview
                    .registry
                    .assignments
                    .iter()
                    .any(|entry| entry.id == *id && entry.enabled)
            });
        (installations_enabled && assignments_enabled).then_some((setup, binding))
    }) else {
        return Ok(None);
    };
    let connection = core
        .calendar_connection(person_id)
        .await
        .map_err(|_| AgentFailure::StorageUnavailable)?
        .ok_or(AgentFailure::StaleContext)?;
    if connection.disconnected || connection.provider != binding.provider {
        return Err(AgentFailure::StaleContext);
    }
    let local = chrono::Local::now();
    let offset = local.offset().local_minus_utc();
    let range = floe_domain::CalendarRange {
        start_date: local.date_naive(),
        end_date_exclusive: local.date_naive() + chrono::Duration::days(1),
        timezone_offset_seconds: offset,
        end_timezone_offset_seconds: None,
    };
    let (starts_at, ends_at) = range_bounds(&range)?;
    let now = chrono::Utc::now();
    let model = Model::conversation(request.remote_route.clone())?;
    let placement = model.placement();
    let external_consent = if request
        .remote_route
        .as_ref()
        .is_some_and(|route| route.external && route.allow_external)
    {
        TransferConsent::Granted
    } else {
        TransferConsent::NotGranted
    };
    let result = core
        .run_calendar_agent_turn(
            vault,
            &Access::new(binding.provider, binding.calendar_ids.clone()),
            &model,
            CalendarAgentTurnRequest {
                command: floe_agent::AgentCommand {
                    schema_version: PROTOCOL_VERSION,
                    person_id,
                    session_id,
                    expected_revision: request.expected_revision,
                    text: request.text.trim().to_owned(),
                },
                context,
                policy: InferencePolicyDecision {
                    purpose: "everyday-assistance".into(),
                    data_classes: vec![DataClass::Personal],
                    allowed_placements: vec![placement],
                    performance_class: "interactive".into(),
                    projection_version: 1,
                    external_transfer_consent: external_consent,
                    bounded_sensitive_projection: false,
                },
                budget: AgentBudget::default(),
                grant: CalendarTimelineGrant {
                    person_id,
                    handle: setup.view_handle,
                    provider: binding.provider,
                    calendar_ids: binding.calendar_ids.clone(),
                    connection_revision: connection.revision,
                    day: range,
                    starts_at,
                    ends_at,
                    expires_at: now + chrono::Duration::minutes(2),
                },
                assignment_id: setup.expert_assignment_id,
                feasibility: optional_local_view(local_context.feasibility(person_id))?,
                wellbeing: optional_local_view(local_context.wellbeing(person_id))?,
                destination: None,
                propose_focus: false,
                cancellation,
                continuation: request.continuation,
            },
            chrono::Utc::now,
            |event| emit(event),
        )
        .await?;
    Ok(Some(result.session))
}

fn optional_local_view<T>(result: Result<T, AgentFailure>) -> Result<Option<T>, AgentFailure> {
    match result {
        Ok(view) => Ok(Some(view)),
        Err(AgentFailure::CapabilityUnavailable | AgentFailure::StaleContext) => Ok(None),
        Err(error) => Err(error),
    }
}

fn range_bounds(
    range: &floe_domain::CalendarRange,
) -> Result<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>), AgentFailure> {
    if !range.is_valid() {
        return Err(AgentFailure::InvalidInput);
    }
    let start = range
        .start_date
        .and_hms_opt(0, 0, 0)
        .ok_or(AgentFailure::InvalidInput)?
        .and_utc()
        - chrono::Duration::seconds(i64::from(range.timezone_offset_seconds));
    let end = range
        .end_date_exclusive
        .and_hms_opt(0, 0, 0)
        .ok_or(AgentFailure::InvalidInput)?
        .and_utc()
        - chrono::Duration::seconds(i64::from(
            range
                .end_timezone_offset_seconds
                .unwrap_or(range.timezone_offset_seconds),
        ));
    Ok((start, end))
}

enum Access {
    Fixture(FixtureAccess),
    Native(NativeCalendar),
    Unsupported,
}

impl Access {
    fn new(provider: CalendarProvider, calendar_ids: Vec<String>) -> Self {
        match provider {
            CalendarProvider::Fixture => Self::Fixture(FixtureAccess { calendar_ids }),
            CalendarProvider::EventKit => Self::Native(NativeCalendar::new(calendar_ids)),
            CalendarProvider::Google | CalendarProvider::Microsoft | CalendarProvider::Android => {
                Self::Unsupported
            }
        }
    }
}

impl CalendarReadAccess for Access {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        match self {
            Self::Fixture(access) => access.check(request).await,
            Self::Native(access) => access.check(request).await,
            Self::Unsupported => Err(AgentFailure::CapabilityUnavailable),
        }
    }

    async fn observe(
        &self,
        request: floe_core::CalendarObserveRequest,
    ) -> Result<Option<floe_core::CalendarObservation>, AgentFailure> {
        match self {
            Self::Fixture(_) => Ok(None),
            Self::Native(access) => access.observe(request).await,
            Self::Unsupported => Err(AgentFailure::CapabilityUnavailable),
        }
    }
}

struct FixtureAccess {
    calendar_ids: Vec<String>,
}

impl CalendarReadAccess for FixtureAccess {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        if request.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if request.deadline <= tokio::time::Instant::now() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        if request.provider != CalendarProvider::Fixture
            || request.calendar_ids != self.calendar_ids
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        Ok(CalendarReadAccessStamp {
            schema_version: PROTOCOL_VERSION,
            person_id: request.person_id,
            provider: request.provider,
            calendar_ids: request.calendar_ids,
            generation: "bounded-fixture".into(),
        })
    }
}

enum Model {
    Foundation(FoundationModelRunner),
    Server(ServerModelRunner),
}

impl Model {
    fn conversation(
        remote_route: Option<floe_protocol::AgentRemoteRouteDto>,
    ) -> Result<Self, AgentFailure> {
        match remote_route {
            Some(route) => ServerModelRunner::new(route).map(Self::Server),
            None => Ok(Self::Foundation(FoundationModelRunner::encrypted())),
        }
    }
}

impl ModelRunner for Model {
    fn placement(&self) -> ModelPlacement {
        match self {
            Self::Foundation(model) => model.placement(),
            Self::Server(model) => model.placement(),
        }
    }

    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
        match self {
            Self::Foundation(model) => model.generate(request).await,
            Self::Server(model) => model.generate(request).await,
        }
    }
}
