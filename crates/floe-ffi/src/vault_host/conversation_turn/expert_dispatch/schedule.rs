use floe_agent::{
    AgentBudget, AgentEvent, AgentFailure, DataClass, InferencePolicyDecision, ModelPlacement,
    ModelRequest, ModelResponse, ModelRunner, SessionStore, TransferConsent,
};
use floe_core::{
    CalendarAgentTurnRequest, CalendarReadAccess, CalendarReadAccessRequest,
    CalendarReadAccessStamp, CalendarTimelineGrant, ProjectedCalendarItem,
    ProjectedCalendarObservation, VaultKeyProvider,
};
use floe_domain::{CalendarProvider, PersonId};
use floe_protocol::PROTOCOL_VERSION;

use crate::{
    local_context::LocalContextStore,
    local_model::FoundationModelRunner,
    remote_model::{CalendarContextRequest, ServerModelRunner},
};

use super::super::super::session_uuid;
use super::ConversationTurnInputs;

pub(in crate::vault_host::conversation_turn) async fn try_run<
    Keys: VaultKeyProvider,
    Emit: FnMut(AgentEvent) + Send,
>(
    inputs: &ConversationTurnInputs<'_, Keys>,
    context: floe_agent::AgentContext,
    cancellation: floe_agent::Cancellation,
    emit: &mut Emit,
) -> Result<Option<floe_agent::AgentSession>, AgentFailure> {
    let core = inputs.core;
    let vault = inputs.vault;
    let local_context = inputs.local_context;
    let person_id = inputs.person_id;
    let request = inputs.request;
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
        let binding = overview.views.iter().find(|binding| {
            binding.handle == setup.view_handle
                && binding.enabled
                && binding.device_id == request.device_id
        })?;
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
            &Access::new(
                binding.provider,
                binding.device_id.clone(),
                binding.calendar_ids.clone(),
                connection.revision,
                &model,
                local_context,
            ),
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
                    device_id: binding.device_id.clone(),
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
            emit,
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

enum Access<'model> {
    Fixture(FixtureAccess),
    Device(DeviceCalendarAccess<'model>),
    Remote(RemoteCalendarAccess<'model>),
}

impl<'model> Access<'model> {
    fn new(
        provider: CalendarProvider,
        device_id: String,
        calendar_ids: Vec<String>,
        connection_revision: u64,
        model: &'model Model,
        local_context: &'model LocalContextStore,
    ) -> Self {
        match provider {
            CalendarProvider::Fixture => Self::Fixture(FixtureAccess {
                device_id,
                calendar_ids,
            }),
            CalendarProvider::EventKit => Self::Device(DeviceCalendarAccess {
                local_context,
                provider,
                device_id,
                calendar_ids,
                connection_revision,
            }),
            CalendarProvider::Google | CalendarProvider::Microsoft => match model {
                Model::Server(model) => Self::Remote(RemoteCalendarAccess {
                    model: Some(model),
                    provider,
                    device_id,
                    calendar_ids,
                    connection_revision,
                }),
                Model::Foundation(_) => Self::Remote(RemoteCalendarAccess {
                    model: None,
                    provider,
                    device_id,
                    calendar_ids,
                    connection_revision,
                }),
            },
            CalendarProvider::Android => Self::Device(DeviceCalendarAccess {
                local_context,
                provider,
                device_id,
                calendar_ids,
                connection_revision,
            }),
        }
    }
}

impl CalendarReadAccess for Access<'_> {
    async fn check(
        &self,
        request: CalendarReadAccessRequest,
    ) -> Result<CalendarReadAccessStamp, AgentFailure> {
        match self {
            Self::Fixture(access) => access.check(request).await,
            Self::Device(access) => access.check(request).await,
            Self::Remote(access) => access.check(request).await,
        }
    }

    async fn observe(
        &self,
        request: floe_core::CalendarObserveRequest,
    ) -> Result<Option<floe_core::CalendarObservation>, AgentFailure> {
        match self {
            Self::Fixture(_) => Ok(None),
            Self::Device(access) => access.observe(request).await,
            Self::Remote(_) => Ok(None),
        }
    }

    async fn observe_projected(
        &self,
        request: floe_core::CalendarObserveRequest,
    ) -> Result<Option<ProjectedCalendarObservation>, AgentFailure> {
        match self {
            Self::Remote(access) => access.observe_projected(request).await,
            Self::Fixture(_) | Self::Device(_) => Ok(None),
        }
    }
}

struct DeviceCalendarAccess<'store> {
    local_context: &'store LocalContextStore,
    provider: CalendarProvider,
    device_id: String,
    calendar_ids: Vec<String>,
    connection_revision: u64,
}

impl CalendarReadAccess for DeviceCalendarAccess<'_> {
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
        if request.device_id != self.device_id
            || request.provider != self.provider
            || request.calendar_ids != self.calendar_ids
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        match self.local_context.calendar_observation(
            request.person_id,
            &self.device_id,
            request.provider,
            &request.calendar_ids,
            self.connection_revision,
        ) {
            Ok(observation) => Ok(CalendarReadAccessStamp {
                schema_version: PROTOCOL_VERSION,
                person_id: request.person_id,
                device_id: request.device_id,
                provider: request.provider,
                calendar_ids: request.calendar_ids,
                generation: format!(
                    "device-{}-{}-{}",
                    observation.device_id,
                    observation.connection_revision,
                    observation.observed_at_unix_ms
                ),
            }),
            Err(error) => Err(error),
        }
    }

    async fn observe(
        &self,
        request: floe_core::CalendarObserveRequest,
    ) -> Result<Option<floe_core::CalendarObservation>, AgentFailure> {
        if request.device_id != self.device_id
            || request.provider != self.provider
            || request.calendar_ids != self.calendar_ids
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        match self.local_context.calendar_observation(
            request.person_id,
            &self.device_id,
            request.provider,
            &request.calendar_ids,
            self.connection_revision,
        ) {
            Ok(observation) => {
                if request.starts_at.timestamp_millis() < observation.range_start_unix_ms
                    || request.ends_at.timestamp_millis() > observation.range_end_unix_ms
                {
                    return Err(AgentFailure::StaleContext);
                }
                let observed_at =
                    chrono::DateTime::from_timestamp_millis(observation.observed_at_unix_ms)
                        .ok_or(AgentFailure::InvalidInput)?;
                Ok(Some(floe_core::CalendarObservation {
                    stamp: CalendarReadAccessStamp {
                        schema_version: PROTOCOL_VERSION,
                        person_id: request.person_id,
                        device_id: request.device_id,
                        provider: request.provider,
                        calendar_ids: request.calendar_ids,
                        generation: format!(
                            "device-{}-{}-{}",
                            observation.device_id,
                            observation.connection_revision,
                            observation.observed_at_unix_ms
                        ),
                    },
                    observed_at,
                    batches: observation.batches,
                }))
            }
            Err(error) => Err(error),
        }
    }
}

struct RemoteCalendarAccess<'model> {
    model: Option<&'model ServerModelRunner>,
    provider: CalendarProvider,
    device_id: String,
    calendar_ids: Vec<String>,
    connection_revision: u64,
}

impl CalendarReadAccess for RemoteCalendarAccess<'_> {
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
        if request.device_id != self.device_id
            || request.provider != self.provider
            || request.calendar_ids != self.calendar_ids
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        if self.model.is_none() {
            return Err(AgentFailure::CapabilityUnavailable);
        }
        Ok(self.stamp(request.person_id))
    }

    async fn observe_projected(
        &self,
        request: floe_core::CalendarObserveRequest,
    ) -> Result<Option<ProjectedCalendarObservation>, AgentFailure> {
        if request.device_id != self.device_id
            || request.provider != self.provider
            || request.calendar_ids != self.calendar_ids
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let model = self.model.ok_or(AgentFailure::CapabilityUnavailable)?;
        let view = model
            .read_calendar_context_view(
                CalendarContextRequest {
                    connector_id: Some(match self.provider {
                        CalendarProvider::Google => "calendar.google",
                        CalendarProvider::Microsoft => "calendar.microsoft",
                        _ => return Err(AgentFailure::CapabilityDenied),
                    }),
                    range_start_unix_ms: request.starts_at.timestamp_millis(),
                    range_end_unix_ms: request.ends_at.timestamp_millis(),
                    cursor: "",
                    limit: floe_agent::MAX_CALENDAR_CONTEXT_ITEMS,
                },
                request.deadline,
                &request.cancellation,
            )
            .await?;
        let observed_at = chrono::DateTime::from_timestamp_millis(view.observed_at_unix_ms)
            .ok_or(AgentFailure::InvalidInput)?;
        let expires_at = chrono::DateTime::from_timestamp_millis(view.expires_at_unix_ms)
            .ok_or(AgentFailure::InvalidInput)?;
        let range_start = chrono::DateTime::from_timestamp_millis(view.range_start_unix_ms)
            .ok_or(AgentFailure::InvalidInput)?;
        let range_end = chrono::DateTime::from_timestamp_millis(view.range_end_unix_ms)
            .ok_or(AgentFailure::InvalidInput)?;
        let items = view
            .items
            .into_iter()
            .map(|item| {
                Ok(ProjectedCalendarItem {
                    evidence_handle: item.evidence_handle,
                    untrusted_title: item.untrusted_title,
                    starts_at: chrono::DateTime::from_timestamp_millis(item.starts_at_unix_ms)
                        .ok_or(AgentFailure::InvalidInput)?,
                    ends_at: chrono::DateTime::from_timestamp_millis(item.ends_at_unix_ms)
                        .ok_or(AgentFailure::InvalidInput)?,
                })
            })
            .collect::<Result<Vec<_>, AgentFailure>>()?;
        Ok(Some(ProjectedCalendarObservation {
            stamp: self.stamp(request.person_id),
            source_handle: view.source_handle,
            observed_at,
            expires_at,
            range_start,
            range_end,
            coverage_complete: view.coverage_complete,
            next_cursor: view.next_cursor,
            items,
        }))
    }
}

impl RemoteCalendarAccess<'_> {
    fn stamp(&self, person_id: PersonId) -> CalendarReadAccessStamp {
        CalendarReadAccessStamp {
            schema_version: PROTOCOL_VERSION,
            person_id,
            device_id: self.device_id.clone(),
            provider: self.provider,
            calendar_ids: self.calendar_ids.clone(),
            generation: format!("server-{}", self.connection_revision),
        }
    }
}

struct FixtureAccess {
    device_id: String,
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
        if request.device_id != self.device_id
            || request.provider != CalendarProvider::Fixture
            || request.calendar_ids != self.calendar_ids
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        Ok(CalendarReadAccessStamp {
            schema_version: PROTOCOL_VERSION,
            person_id: request.person_id,
            device_id: request.device_id,
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

#[cfg(test)]
mod tests {
    use super::*;
    use floe_protocol::{CalendarBatchDto, LocalContextOperationDto};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn publish_device_calendar(
        store: &LocalContextStore,
        person_id: PersonId,
        device_id: &str,
        observed_at_unix_ms: i64,
    ) {
        store
            .request(
                person_id,
                LocalContextOperationDto::PublishCalendarObservation {
                    device_id: device_id.into(),
                    connection_revision: 7,
                    provider: CalendarProvider::EventKit,
                    calendar_ids: vec!["primary".into()],
                    observed_at_unix_ms,
                    expires_at_unix_ms: observed_at_unix_ms + 240_000,
                    range_start_unix_ms: observed_at_unix_ms - 60_000,
                    range_end_unix_ms: observed_at_unix_ms + 60_000,
                    batches: vec![CalendarBatchDto {
                        calendar_id: "primary".into(),
                        records: vec![],
                        failure: None,
                    }],
                },
            )
            .unwrap();
    }

    #[tokio::test]
    async fn device_calendar_access_never_consumes_another_devices_observation() {
        let store = LocalContextStore::default();
        let person_id = PersonId::new();
        let now = chrono::Utc::now().timestamp_millis();
        publish_device_calendar(&store, person_id, "iphone", now - 2);
        publish_device_calendar(&store, person_id, "ipad", now - 1);
        let access = DeviceCalendarAccess {
            local_context: &store,
            provider: CalendarProvider::EventKit,
            device_id: "iphone".into(),
            calendar_ids: vec!["primary".into()],
            connection_revision: 7,
        };
        let request = |device_id: &str| CalendarReadAccessRequest {
            person_id,
            device_id: device_id.into(),
            provider: CalendarProvider::EventKit,
            calendar_ids: vec!["primary".into()],
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            cancellation: floe_agent::Cancellation::default(),
        };

        let stamp = access.check(request("iphone")).await.unwrap();
        assert_eq!(stamp.device_id, "iphone");
        assert!(stamp.generation.starts_with("device-iphone-7-"));
        assert_eq!(
            access.check(request("ipad")).await,
            Err(AgentFailure::CapabilityDenied)
        );
    }

    async fn receive_request(mut socket: tokio::net::TcpStream) -> serde_json::Value {
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0_u8; 4096];
            let read = socket.read(&mut chunk).await.unwrap();
            bytes.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&bytes);
            let Some((headers, body)) = text.split_once("\r\n\r\n") else {
                continue;
            };
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|value| value.parse::<usize>().ok())
                })
                .unwrap();
            if body.len() < content_length {
                continue;
            }
            assert!(headers.starts_with("POST /v1/views/calendar.timeline "));
            let value: serde_json::Value =
                serde_json::from_slice(&body.as_bytes()[..content_length]).unwrap();
            let response = serde_json::json!({
                "schema_version": 1,
                "view": {
                    "schema_version": 1,
                    "view_id": "calendar.timeline",
                    "source_handle": "calendar.timeline:google",
                    "observed_at_unix_ms": value["range_start_unix_ms"].as_i64().unwrap(),
                    "expires_at_unix_ms": value["range_start_unix_ms"].as_i64().unwrap() + 300_000,
                    "range_start_unix_ms": value["range_start_unix_ms"],
                    "range_end_unix_ms": value["range_end_unix_ms"],
                    "coverage_complete": true,
                    "items": []
                }
            })
            .to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                        response.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            return value;
        }
    }

    #[tokio::test]
    async fn server_calendar_access_sends_bounded_contract_and_returns_projected_view() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            receive_request(socket).await
        });
        let model = ServerModelRunner::new(floe_protocol::AgentRemoteRouteDto {
            base_url: format!("http://{address}/"),
            bearer_token: "a".repeat(32),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
        })
        .unwrap();
        let access = RemoteCalendarAccess {
            model: Some(&model),
            provider: CalendarProvider::Google,
            device_id: "test-device".into(),
            calendar_ids: vec!["primary".into()],
            connection_revision: 9,
        };
        let now = chrono::Utc::now();
        let request = floe_core::CalendarObserveRequest {
            person_id: PersonId::new(),
            device_id: "test-device".into(),
            provider: CalendarProvider::Google,
            calendar_ids: vec!["primary".into()],
            starts_at: now - chrono::Duration::minutes(1),
            ends_at: now + chrono::Duration::minutes(1),
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(2),
            cancellation: floe_agent::Cancellation::default(),
        };
        let observation = access.observe_projected(request).await.unwrap().unwrap();
        assert_eq!(observation.stamp.generation, "server-9");
        assert_eq!(observation.source_handle, "calendar.timeline:google");
        let body = server.await.unwrap();
        assert_eq!(body["schema_version"], 1);
        assert_eq!(body["connector_id"], "calendar.google");
        assert_eq!(body["cursor"], "");
        assert_eq!(body["limit"], floe_agent::MAX_CALENDAR_CONTEXT_ITEMS);
        assert_eq!(body.as_object().unwrap().len(), 6);
    }

    #[tokio::test]
    async fn device_only_and_wrong_provider_access_keep_distinct_failures() {
        let access = RemoteCalendarAccess {
            model: None,
            provider: CalendarProvider::Android,
            device_id: "test-device".into(),
            calendar_ids: vec!["primary".into()],
            connection_revision: 1,
        };
        let person_id = PersonId::new();
        let unavailable = access
            .check(CalendarReadAccessRequest {
                person_id,
                device_id: "test-device".into(),
                provider: CalendarProvider::Android,
                calendar_ids: vec!["primary".into()],
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                cancellation: floe_agent::Cancellation::default(),
            })
            .await;
        assert_eq!(unavailable, Err(AgentFailure::CapabilityUnavailable));
        let denied = access
            .check(CalendarReadAccessRequest {
                person_id,
                device_id: "test-device".into(),
                provider: CalendarProvider::Google,
                calendar_ids: vec!["primary".into()],
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                cancellation: floe_agent::Cancellation::default(),
            })
            .await;
        assert_eq!(denied, Err(AgentFailure::CapabilityDenied));
    }
}
