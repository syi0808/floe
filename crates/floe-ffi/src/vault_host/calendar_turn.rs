use floe_agent::{
    AgentBudget, AgentCommand, AgentContext, AgentEvent, AgentFailure, AgentMessage, DataClass,
    InferencePolicyDecision, ModelPlacement, ModelRequest, ModelResponse, ModelRunner, ModelStep,
    SessionStore, TransferConsent, calendar_briefing_prompt, calendar_focus_proposal_prompt,
};
use floe_core::{
    CalendarAgentTurnRequest, CalendarReadAccess, CalendarReadAccessRequest,
    CalendarReadAccessStamp, CalendarTimelineGrant, EncryptedAgentVault, ExpertCalendarDestination,
    FloeCore, VaultKeyProvider,
};
use floe_domain::{CalendarProvider, PersonId};
use floe_protocol::{
    AgentCalendarInferenceRouteDto, AgentCalendarPromptDto, AgentCalendarProposalOutcomeDto,
    AgentCalendarTurnRequestDto, AgentCalendarTurnResultDto, AgentConversationTurnRequestDto,
    PROTOCOL_VERSION,
};

use crate::{
    local_model::FoundationModelRunner, native_calendar::NativeCalendar,
    remote_model::ServerModelRunner,
};

use super::{calendar_action, session_uuid};

pub(super) async fn run_conversation<Keys: VaultKeyProvider, Emit: FnMut(AgentEvent) + Send>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
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
    let mut ranges = binding.calendar_ids.iter().map(|calendar_id| {
        connection
            .source_statuses
            .get(calendar_id)
            .and_then(|status| (status.error.is_none()).then_some(status.last_range.as_ref()?))
            .ok_or(AgentFailure::StaleContext)
    });
    let range = ranges.next().ok_or(AgentFailure::StaleContext)??.clone();
    if ranges.any(|candidate| candidate.is_err() || candidate.is_ok_and(|value| value != &range)) {
        return Err(AgentFailure::StaleContext);
    }
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
                destination: None,
                cancellation,
                continuation: request.continuation,
            },
            chrono::Utc::now,
            |event| emit(event),
        )
        .await?;
    Ok(Some(result.session))
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

pub(super) async fn run<Keys: VaultKeyProvider>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    person_id: PersonId,
    request: &AgentCalendarTurnRequestDto,
    cancellation: floe_agent::Cancellation,
    emit: impl FnMut(AgentEvent) + Send,
) -> Result<(floe_agent::AgentSession, AgentCalendarTurnResultDto), AgentFailure> {
    let session_id = session_uuid(&request.session_id)?;
    let session = vault.calendar_session(session_id).await?;
    if session.person_id != person_id || session.revision != request.expected_revision {
        return Err(AgentFailure::Conflict);
    }
    let setup = vault.calendar_session_setup(&session).await?;
    let overview = vault.calendar_expert_overview().await?;
    let binding = overview
        .views
        .iter()
        .find(|binding| binding.handle == setup.view_handle)
        .ok_or(AgentFailure::Conflict)?;
    let connection = core
        .calendar_connection(person_id)
        .await
        .map_err(|_| AgentFailure::StorageUnavailable)?
        .ok_or(AgentFailure::StaleContext)?;
    if connection.disconnected || connection.provider != binding.provider {
        return Err(AgentFailure::StaleContext);
    }
    if let Some(destination) = &request.destination
        && (destination.provider != connection.provider
            || destination.connection_revision != connection.revision
            || !binding.calendar_ids.contains(&destination.calendar_id)
            || destination.timezone.trim().is_empty()
            || destination.timezone.len() > 128)
    {
        return Err(AgentFailure::CapabilityDenied);
    }
    if !matches!(
        (binding.provider, request.inference_route),
        (
            CalendarProvider::Fixture,
            AgentCalendarInferenceRouteDto::DeterministicFixture
        ) | (
            CalendarProvider::EventKit,
            AgentCalendarInferenceRouteDto::DeviceLocal | AgentCalendarInferenceRouteDto::Remote
        )
    ) {
        return Err(AgentFailure::PolicyDenied);
    }
    let range_days = (request.day.end_date_exclusive - request.day.start_date).num_days();
    if !(1..=floe_agent::MAX_TIMELINE_VIEW_DAYS).contains(&range_days)
        || request.starts_at >= request.ends_at
    {
        return Err(AgentFailure::InvalidInput);
    }
    let prompt = prompt_text(&request.prompt)?;
    let now = chrono::Utc::now();
    let data_class = match binding.provider {
        CalendarProvider::Fixture => DataClass::Synthetic,
        CalendarProvider::EventKit => DataClass::Personal,
    };
    if request.remote_route.is_some()
        && !matches!(request.prompt, AgentCalendarPromptDto::FreeText { .. })
    {
        return Err(AgentFailure::InvalidInput);
    }
    let model = Model::new(
        request.inference_route,
        request.prompt.clone(),
        request.remote_route.clone(),
    )?;
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
    let turn = CalendarAgentTurnRequest {
        command: AgentCommand {
            schema_version: PROTOCOL_VERSION,
            person_id,
            session_id,
            expected_revision: request.expected_revision,
            text: prompt,
        },
        context: AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            evidence: vec![],
        },
        policy: InferencePolicyDecision {
            purpose: match &request.prompt {
                AgentCalendarPromptDto::Briefing { .. } => "calendar-briefing",
                AgentCalendarPromptDto::ProposeFocus { .. } => "calendar-focus-proposal",
                AgentCalendarPromptDto::FreeText { .. } => "everyday-assistance",
            }
            .into(),
            data_classes: vec![data_class],
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
            day: request.day.clone(),
            starts_at: request.starts_at,
            ends_at: request.ends_at,
            expires_at: now + chrono::Duration::minutes(2),
        },
        assignment_id: setup.expert_assignment_id,
        destination: request
            .destination
            .as_ref()
            .map(|destination| ExpertCalendarDestination {
                provider: destination.provider,
                calendar_id: destination.calendar_id.clone(),
                connection_revision: destination.connection_revision,
                timezone: destination.timezone.clone(),
            }),
        cancellation,
        continuation: request.continuation,
    };
    let access = Access::new(binding.provider, binding.calendar_ids.clone());
    let result = core
        .run_calendar_agent_turn(vault, &access, &model, turn, chrono::Utc::now, emit)
        .await?;
    let proposals = result
        .proposals
        .into_iter()
        .map(|proposal| match proposal.result {
            Ok(action) => AgentCalendarProposalOutcomeDto {
                invocation_id: proposal.reference.invocation_id.to_string(),
                action: Some(calendar_action(action)),
                failure: None,
            },
            Err(failure) => AgentCalendarProposalOutcomeDto {
                invocation_id: proposal.reference.invocation_id.to_string(),
                action: None,
                failure: Some(failure),
            },
        })
        .collect();
    let response = AgentCalendarTurnResultDto {
        schema_version: PROTOCOL_VERSION,
        person_id: person_id.to_string(),
        session_id: result.session.id.to_string(),
        setup_id: setup.setup_id.to_string(),
        inference_route: request.inference_route,
        proposals,
    };
    Ok((result.session, response))
}

fn prompt_text(prompt: &AgentCalendarPromptDto) -> Result<String, AgentFailure> {
    match prompt {
        AgentCalendarPromptDto::Briefing { focus_minutes } => {
            Ok(calendar_briefing_prompt(*focus_minutes))
        }
        AgentCalendarPromptDto::ProposeFocus { focus_minutes } => {
            Ok(calendar_focus_proposal_prompt(*focus_minutes))
        }
        AgentCalendarPromptDto::FreeText { text }
            if !text.trim().is_empty() && text.len() <= 8_192 =>
        {
            Ok(text.trim().to_owned())
        }
        AgentCalendarPromptDto::FreeText { .. } => Err(AgentFailure::InvalidInput),
    }
}

enum Access {
    Fixture(FixtureAccess),
    Native(NativeCalendar),
}

impl Access {
    fn new(provider: CalendarProvider, calendar_ids: Vec<String>) -> Self {
        match provider {
            CalendarProvider::Fixture => Self::Fixture(FixtureAccess { calendar_ids }),
            CalendarProvider::EventKit => Self::Native(NativeCalendar::new(calendar_ids)),
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
    Deterministic(DeterministicModel),
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

    fn new(
        route: AgentCalendarInferenceRouteDto,
        prompt: AgentCalendarPromptDto,
        remote_route: Option<floe_protocol::AgentRemoteRouteDto>,
    ) -> Result<Self, AgentFailure> {
        match (route, remote_route) {
            (AgentCalendarInferenceRouteDto::DeterministicFixture, None) => {
                Ok(Self::Deterministic(DeterministicModel { prompt }))
            }
            (AgentCalendarInferenceRouteDto::DeviceLocal, None) => {
                Ok(Self::Foundation(FoundationModelRunner::encrypted()))
            }
            (AgentCalendarInferenceRouteDto::Remote, Some(route)) => {
                ServerModelRunner::new(route).map(Self::Server)
            }
            _ => Err(AgentFailure::InvalidInput),
        }
    }
}

impl ModelRunner for Model {
    fn placement(&self) -> ModelPlacement {
        match self {
            Self::Deterministic(model) => model.placement(),
            Self::Foundation(model) => model.placement(),
            Self::Server(model) => model.placement(),
        }
    }

    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
        match self {
            Self::Deterministic(model) => model.generate(request).await,
            Self::Foundation(model) => model.generate(request).await,
            Self::Server(model) => model.generate(request).await,
        }
    }
}

struct DeterministicModel {
    prompt: AgentCalendarPromptDto,
}

impl ModelRunner for DeterministicModel {
    fn placement(&self) -> ModelPlacement {
        ModelPlacement::DeviceLocal
    }

    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
        if request.cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        if request.deadline <= tokio::time::Instant::now() {
            return Err(AgentFailure::DeadlineExceeded);
        }
        if request.policy.data_classes != [DataClass::Synthetic] {
            return Err(AgentFailure::PolicyDenied);
        }
        let schedule_expert = request.prompt.role == floe_agent::PromptRole::ScheduleExpert;
        let capability_results: Vec<_> = request
            .messages
            .iter()
            .filter_map(|message| match message {
                AgentMessage::Capability {
                    capability_id,
                    result: Ok(output),
                    ..
                } => Some((capability_id.as_str(), output.as_str())),
                _ => None,
            })
            .collect();
        let step = if !schedule_expert {
            if request
                .messages
                .iter()
                .any(|message| matches!(message, AgentMessage::Delegation { .. }))
            {
                ModelStep::Answer {
                    text: "Synthetic Calendar result recorded. No live personal source was read."
                        .into(),
                }
            } else {
                let agent_id = request
                    .active_agents
                    .first()
                    .ok_or(AgentFailure::CapabilityDenied)?
                    .id
                    .clone();
                let message = match &self.prompt {
                    AgentCalendarPromptDto::Briefing { focus_minutes } => format!(
                        "Review today's calendar and provide a briefing, considering whether a {focus_minutes}-minute open window exists."
                    ),
                    AgentCalendarPromptDto::ProposeFocus { focus_minutes } => format!(
                        "Find an available {focus_minutes}-minute window and return a typed proposal for the best option."
                    ),
                    AgentCalendarPromptDto::FreeText { text } => text.clone(),
                };
                ModelStep::Delegate { agent_id, message }
            }
        } else if capability_results.is_empty() {
            match self.prompt {
                AgentCalendarPromptDto::ProposeFocus { focus_minutes }
                | AgentCalendarPromptDto::Briefing { focus_minutes } => ModelStep::Call {
                    capability_id: "schedule.find_free_windows".into(),
                    input: serde_json::json!({"minimum_minutes": focus_minutes}).to_string(),
                },
                AgentCalendarPromptDto::FreeText { .. } => ModelStep::Call {
                    capability_id: "calendar.read".into(),
                    input: "{}".into(),
                },
            }
        } else if matches!(self.prompt, AgentCalendarPromptDto::ProposeFocus { .. })
            && capability_results.len() == 1
        {
            let insights: Vec<serde_json::Value> = serde_json::from_str(capability_results[0].1)
                .map_err(|_| AgentFailure::InvalidModelOutput)?;
            let window = insights.iter().find_map(|insight| {
                if insight["kind"] == "focus_window" {
                    Some((
                        insight["starts_at_unix_ms"].as_u64()?,
                        insight["ends_at_unix_ms"].as_u64()?,
                    ))
                } else {
                    None
                }
            });
            match window {
                Some((starts_at_unix_ms, ends_at_unix_ms)) => ModelStep::Call {
                    capability_id: "schedule.propose_window".into(),
                    input: serde_json::json!({
                        "starts_at_unix_ms": starts_at_unix_ms,
                        "ends_at_unix_ms": ends_at_unix_ms
                    })
                    .to_string(),
                },
                None => ModelStep::Answer {
                    text: "No suitable Calendar window was found.".into(),
                },
            }
        } else {
            ModelStep::Answer {
                text: "The deterministic Schedule Expert completed its bounded calendar review."
                    .into(),
            }
        };
        Ok(ModelResponse {
            replay: None,
            schema_version: PROTOCOL_VERSION,
            output: vec![step],
            used_tokens: 32,
            cost_micros: 0,
        })
    }
}

#[cfg(test)]
mod routing_tests {
    use super::*;

    #[test]
    fn free_text_calendar_chat_prefers_the_configured_daily_route() {
        let model = Model::new(
            AgentCalendarInferenceRouteDto::Remote,
            AgentCalendarPromptDto::FreeText {
                text: "What is next?".into(),
            },
            Some(floe_protocol::AgentRemoteRouteDto {
                base_url: "http://127.0.0.1:8431".into(),
                bearer_token: "daily_route_token_that_is_long_enough".into(),
                purpose: "everyday_assistance".into(),
                external: false,
                allow_external: false,
            }),
        )
        .unwrap();
        assert!(matches!(model, Model::Server(_)));
    }

    #[test]
    fn inference_route_requires_its_matching_configuration() {
        let prompt = AgentCalendarPromptDto::FreeText {
            text: "What is next?".into(),
        };
        assert!(matches!(
            Model::new(
                AgentCalendarInferenceRouteDto::DeviceLocal,
                prompt.clone(),
                None,
            )
            .unwrap(),
            Model::Foundation(_)
        ));
        assert!(matches!(
            Model::new(AgentCalendarInferenceRouteDto::Remote, prompt, None),
            Err(AgentFailure::InvalidInput)
        ));
    }
}
