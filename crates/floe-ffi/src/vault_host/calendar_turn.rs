use floe_agent::{
    AgentBudget, AgentCommand, AgentContext, AgentEvent, AgentFailure, AgentMessage, DataClass,
    ExpertInput, InferencePolicyDecision, ModelPlacement, ModelRequest, ModelResponse, ModelRunner,
    ModelStep, TransferConsent, calendar_briefing_prompt, calendar_focus_proposal_prompt,
};
use floe_core::{
    CalendarAgentTurnRequest, CalendarReadAccess, CalendarReadAccessRequest,
    CalendarReadAccessStamp, CalendarTimelineGrant, EncryptedAgentVault, ExpertCalendarDestination,
    FloeCore, VaultKeyProvider,
};
use floe_domain::{CalendarProvider, PersonId};
use floe_protocol::{
    AgentCalendarModelDto, AgentCalendarPromptDto, AgentCalendarProposalOutcomeDto,
    AgentCalendarTurnRequestDto, AgentCalendarTurnResultDto, PROTOCOL_VERSION,
};

use crate::{
    local_model::FoundationModelRunner, native_calendar::NativeCalendar,
    remote_model::ServerModelRunner,
};

use super::{calendar_action, session_uuid};

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
    if request.model == AgentCalendarModelDto::DeterministicFixture
        && binding.provider != CalendarProvider::Fixture
    {
        return Err(AgentFailure::PolicyDenied);
    }
    if (request.day.end_date_exclusive - request.day.start_date).num_days() != 1
        || request.starts_at >= request.ends_at
        || request.ends_at - request.starts_at > chrono::Duration::hours(24)
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
        request.model,
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
        model: request.model,
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
    fn new(
        selection: AgentCalendarModelDto,
        prompt: AgentCalendarPromptDto,
        remote_route: Option<floe_protocol::AgentRemoteRouteDto>,
    ) -> Result<Self, AgentFailure> {
        match selection {
            AgentCalendarModelDto::DeterministicFixture => {
                Ok(Self::Deterministic(DeterministicModel { prompt }))
            }
            AgentCalendarModelDto::FoundationModels => match remote_route {
                Some(route) => ServerModelRunner::new(route).map(Self::Server),
                None => Ok(Self::Foundation(FoundationModelRunner::encrypted())),
            },
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
        let schedule_expert =
            request.system_instructions == floe_agent::SCHEDULE_EXPERT_SYSTEM_INSTRUCTIONS;
        let step = if request
            .messages
            .iter()
            .any(|message| matches!(message, AgentMessage::Capability { .. }))
        {
            ModelStep::Answer {
                text: if schedule_expert {
                    "The deterministic schedule tool found the bounded Calendar result."
                } else {
                    "Synthetic Calendar result recorded. No live personal source was read."
                }
                .into(),
            }
        } else {
            let input = if schedule_expert {
                "{}".into()
            } else {
                serde_json::to_string(&match &self.prompt {
                    AgentCalendarPromptDto::Briefing { focus_minutes } => ExpertInput::Briefing {
                        focus_minutes: *focus_minutes,
                    },
                    AgentCalendarPromptDto::ProposeFocus { focus_minutes } => {
                        ExpertInput::ProposeFocus {
                            focus_minutes: *focus_minutes,
                        }
                    }
                    AgentCalendarPromptDto::FreeText { .. } => {
                        ExpertInput::Briefing { focus_minutes: 60 }
                    }
                })
                .map_err(|_| AgentFailure::InvalidInput)?
            };
            ModelStep::Call {
                capability_id: request
                    .capabilities
                    .first()
                    .ok_or(AgentFailure::CapabilityDenied)?
                    .id
                    .clone(),
                input,
            }
        };
        Ok(ModelResponse {
            schema_version: PROTOCOL_VERSION,
            step,
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
            AgentCalendarModelDto::FoundationModels,
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
}
