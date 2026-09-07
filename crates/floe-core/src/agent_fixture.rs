use floe_agent::*;
use floe_domain::PersonId;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

use crate::{CoreError, ErrorCode, FloeCore, TursoStore};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentFixturePrompt {
    Today,
    FollowUp,
    RepeatedCall,
    Unavailable,
}

impl AgentFixturePrompt {
    fn text(self) -> &'static str {
        match self {
            Self::Today => "Show the sample day briefing.",
            Self::FollowUp => "What can the sample assistant change?",
            Self::RepeatedCall => "Repeat the sample read without progress.",
            Self::Unavailable => "Show a sample model connection failure.",
        }
    }
}

#[derive(Serialize)]
pub struct AgentFixtureResult {
    pub session: AgentSession,
    pub events: Vec<AgentEvent>,
}

pub struct AgentFixtureTurn {
    pub person_id: PersonId,
    pub session_id: Uuid,
    pub expected_revision: u64,
    pub prompt: AgentFixturePrompt,
}

impl FloeCore {
    pub async fn resume_agent_fixture(
        &self,
        person_id: PersonId,
    ) -> Result<AgentSession, AgentFailure> {
        match self
            .store
            .latest_agent_fixture_session(person_id)
            .await
            .map_err(agent_error)?
        {
            Some(session) => Ok(session),
            None => self.start_agent_fixture(person_id).await,
        }
    }

    pub async fn start_agent_fixture(
        &self,
        person_id: PersonId,
    ) -> Result<AgentSession, AgentFailure> {
        let mut session = AgentSession::new(person_id);
        session.data_classes = vec![DataClass::Synthetic];
        self.store
            .save_agent_fixture_session(&session, None)
            .await
            .map_err(agent_error)?;
        Ok(session)
    }

    pub async fn agent_fixture_session(
        &self,
        person_id: PersonId,
        session_id: Uuid,
    ) -> Result<AgentSession, AgentFailure> {
        self.store
            .agent_fixture_session(person_id, session_id)
            .await
            .map_err(agent_error)
    }

    pub async fn run_agent_fixture(
        &self,
        person_id: PersonId,
        session_id: Uuid,
        expected_revision: u64,
        prompt: AgentFixturePrompt,
    ) -> Result<AgentFixtureResult, AgentFailure> {
        let mut events = vec![];
        let session = self
            .fixture_turn(
                AgentFixtureTurn {
                    person_id,
                    session_id,
                    expected_revision,
                    prompt,
                },
                Cancellation::default(),
                Duration::ZERO,
                |event| events.push(event),
            )
            .await?;
        Ok(AgentFixtureResult { session, events })
    }

    pub async fn stream_agent_fixture(
        &self,
        turn: AgentFixtureTurn,
        cancellation: Cancellation,
        emit: impl FnMut(AgentEvent) + Send,
    ) -> Result<AgentSession, AgentFailure> {
        self.fixture_turn(turn, cancellation, Duration::from_millis(500), emit)
            .await
    }

    async fn fixture_turn(
        &self,
        turn: AgentFixtureTurn,
        cancellation: Cancellation,
        latency: Duration,
        emit: impl FnMut(AgentEvent) + Send,
    ) -> Result<AgentSession, AgentFailure> {
        let store = FixtureStore(&self.store);
        run_agent_sample(&store, turn, cancellation, latency, emit).await
    }

    pub async fn recover_agent_fixture(
        &self,
        person_id: PersonId,
        session_id: Uuid,
        expected_revision: u64,
    ) -> Result<AgentSession, AgentFailure> {
        recover_agent_sample(
            &FixtureStore(&self.store),
            person_id,
            session_id,
            expected_revision,
        )
        .await
    }
}

pub async fn run_agent_sample(
    store: &impl SessionStore,
    turn: AgentFixtureTurn,
    cancellation: Cancellation,
    latency: Duration,
    emit: impl FnMut(AgentEvent) + Send,
) -> Result<AgentSession, AgentFailure> {
    let policy = fixture_policy();
    let model = FixtureModel { latency };
    let capabilities = FixtureCapabilities;
    let runtime = AgentRuntime {
        store,
        model: &model,
        capabilities: &capabilities,
        policy: &policy,
        budget: AgentBudget::default(),
    };
    runtime
        .run_turn(
            AgentCommand {
                schema_version: AGENT_VERSION,
                person_id: turn.person_id,
                session_id: turn.session_id,
                expected_revision: turn.expected_revision,
                text: turn.prompt.text().into(),
            },
            AgentContext {
                projection_version: 1,
                evidence: vec![],
            },
            cancellation,
            emit,
        )
        .await
}

pub async fn recover_agent_sample(
    store: &impl SessionStore,
    person_id: PersonId,
    session_id: Uuid,
    expected_revision: u64,
) -> Result<AgentSession, AgentFailure> {
    let policy = fixture_policy();
    AgentRuntime {
        store,
        model: &FixtureModel {
            latency: Duration::ZERO,
        },
        capabilities: &FixtureCapabilities,
        policy: &policy,
        budget: AgentBudget::default(),
    }
    .recover_interrupted(person_id, session_id, expected_revision)
    .await
}
struct FixtureStore<'store>(&'store TursoStore);

impl SessionStore for FixtureStore<'_> {
    fn protection(&self) -> SessionProtection {
        SessionProtection::SyntheticOnly
    }

    async fn load(
        &self,
        person_id: PersonId,
        session_id: Uuid,
    ) -> Result<AgentSession, AgentFailure> {
        self.0
            .agent_fixture_session(person_id, session_id)
            .await
            .map_err(agent_error)
    }

    async fn compare_and_swap(
        &self,
        session: &AgentSession,
        previous_revision: u64,
    ) -> Result<(), AgentFailure> {
        let previous = self.load(session.person_id, session.id).await?;
        if previous.revision != previous_revision {
            return Err(AgentFailure::Conflict);
        }
        self.0
            .save_agent_fixture_session(session, Some(&previous))
            .await
            .map_err(agent_error)
    }
}

struct FixtureModel {
    latency: Duration,
}

impl ModelRunner for FixtureModel {
    fn placement(&self) -> ModelPlacement {
        ModelPlacement::DeviceLocal
    }

    async fn generate(&self, request: ModelRequest) -> Result<ModelResponse, AgentFailure> {
        if !self.latency.is_zero() {
            tokio::time::sleep(self.latency).await;
        }
        let prompt = request
            .messages
            .iter()
            .rev()
            .find_map(|message| match message {
                AgentMessage::User { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .ok_or(AgentFailure::InvalidInput)?;
        if prompt == AgentFixturePrompt::Unavailable.text() {
            return Err(AgentFailure::ModelUnavailable);
        }
        let step = if prompt == AgentFixturePrompt::FollowUp.text() {
            ModelStep::Answer { text: "This is synthetic evidence only. Calendar changes still require the existing Review and action authority boundary.".into() }
        } else if prompt == AgentFixturePrompt::RepeatedCall.text()
            || !matches!(
                request.messages.last(),
                Some(AgentMessage::Capability { .. })
            )
        {
            ModelStep::Call {
                capability_id: "fixture.schedule.read".into(),
                input: "sample-day".into(),
            }
        } else {
            ModelStep::Answer { text: "Sample briefing: Design review is at 10:00. There is a free hour afterward. No connected Calendar, mail, health or location data was read.".into() }
        };
        Ok(ModelResponse {
            schema_version: AGENT_VERSION,
            step,
            used_tokens: 32,
            cost_micros: 0,
        })
    }
}

struct FixtureCapabilities;

impl CapabilityHost for FixtureCapabilities {
    fn descriptors(&self, _person_id: PersonId) -> Vec<CapabilityDescriptor> {
        vec![CapabilityDescriptor {
            schema_version: AGENT_VERSION,
            id: "fixture.schedule.read".into(),
            version: "1.0.0".into(),
            read_only: true,
            output_data_class: DataClass::Synthetic,
        }]
    }

    async fn invoke(&self, invocation: CapabilityInvocation) -> Result<String, AgentFailure> {
        if invocation.capability_id != "fixture.schedule.read" || invocation.input != "sample-day" {
            return Err(AgentFailure::CapabilityDenied);
        }
        Ok("Synthetic timeline: Design review 10:00–11:00; free 11:00–12:00.".into())
    }
}

fn fixture_policy() -> InferencePolicyDecision {
    InferencePolicyDecision {
        purpose: "s4-contract-fixture".into(),
        data_classes: vec![DataClass::Synthetic],
        allowed_placements: vec![ModelPlacement::DeviceLocal],
        performance_class: "fixture".into(),
        projection_version: 1,
        external_transfer_consent: TransferConsent::NotGranted,
        bounded_sensitive_projection: false,
    }
}

fn agent_error(error: CoreError) -> AgentFailure {
    match error.code {
        ErrorCode::NotFound => AgentFailure::NotFound,
        ErrorCode::Conflict => AgentFailure::Conflict,
        _ => AgentFailure::StorageUnavailable,
    }
}
