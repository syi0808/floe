use floe_agent::*;
use floe_domain::PersonId;
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};
use std::{sync::Mutex, time::Duration};
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
            Self::Today => fixture_today_prompt(),
            Self::FollowUp => fixture_follow_up_prompt(),
            Self::RepeatedCall => fixture_repeated_call_prompt(),
            Self::Unavailable => fixture_unavailable_prompt(),
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

async fn run_agent_sample(
    store: &impl SessionStore,
    turn: AgentFixtureTurn,
    cancellation: Cancellation,
    latency: Duration,
    emit: impl FnMut(AgentEvent) + Send,
) -> Result<AgentSession, AgentFailure> {
    let capabilities = FixtureCapabilities::new(turn.person_id)?;
    run_sample_with_capabilities(store, &capabilities, turn, cancellation, latency, emit).await
}

async fn run_sample_with_capabilities(
    store: &impl SessionStore,
    capabilities: &(impl CapabilityHost + InProcessAgent),
    turn: AgentFixtureTurn,
    cancellation: Cancellation,
    latency: Duration,
    emit: impl FnMut(AgentEvent) + Send,
) -> Result<AgentSession, AgentFailure> {
    let policy = fixture_policy();
    let model = FixtureModel { latency };
    let runtime = AgentRuntime {
        store,
        model: &model,
        capabilities,
        policy: &policy,
        budget: AgentBudget::default(),
    };
    let transport = InProcessA2ATransport::new(capabilities);
    let router = A2ARouter::new(&transport);
    runtime
        .run_turn_with_agents(
            AgentCommand {
                schema_version: AGENT_VERSION,
                person_id: turn.person_id,
                session_id: turn.session_id,
                expected_revision: turn.expected_revision,
                text: turn.prompt.text().into(),
            },
            AgentContext {
                projection_version: 1,
                persona: None,
                evidence: vec![],
            },
            &router,
            cancellation,
            emit,
        )
        .await
}

#[cfg(unix)]
impl<Keys: crate::VaultKeyProvider> crate::EncryptedAgentVault<Keys> {
    pub async fn run_persisted_agent_sample(
        &self,
        turn: AgentFixtureTurn,
        cancellation: Cancellation,
        latency: Duration,
        emit: impl FnMut(AgentEvent) + Send,
    ) -> Result<AgentSession, AgentFailure> {
        if cancellation.is_cancelled() {
            return Err(AgentFailure::Cancelled);
        }
        let session = self.load(turn.person_id, turn.session_id).await?;
        if session.scope.is_some() || session.data_classes != [DataClass::Synthetic] {
            return Err(AgentFailure::PolicyDenied);
        }
        if session.revision != turn.expected_revision || session.active_turn.is_some() {
            return Err(AgentFailure::Conflict);
        }
        let check = || {
            if cancellation.is_cancelled() {
                Err(AgentFailure::Cancelled)
            } else {
                Ok(())
            }
        };
        let snapshot = match self.expert_registry().await? {
            Some(previous) => {
                let revision = previous.revision;
                let snapshot = FixtureCapabilities::ensure_snapshot(turn.person_id, previous)?;
                if snapshot.revision != revision {
                    self.save_expert_registry_checked(revision, &snapshot, &check)
                        .await?;
                }
                snapshot
            }
            None => {
                let capabilities = FixtureCapabilities::new_with_instance(
                    turn.person_id,
                    self.registry_instance_id(),
                )?;
                let snapshot = capabilities.snapshot()?;
                self.initialize_expert_registry_checked(&snapshot, &check)
                    .await?;
                snapshot
            }
        };
        let revision = snapshot.revision;
        let capabilities = FixtureCapabilities::from_snapshot(turn.person_id, snapshot)?;
        let store = ExpertSessionStore {
            vault: self,
            registry: &capabilities.registry,
            persisted_revision: AtomicU64::new(revision),
        };
        let authorized = PersistedFixtureCapabilities {
            vault: self,
            capabilities: &capabilities,
            persisted_revision: &store.persisted_revision,
        };
        run_sample_with_capabilities(&store, &authorized, turn, cancellation, latency, emit).await
    }
}

#[cfg(unix)]
struct PersistedFixtureCapabilities<'host, Keys> {
    vault: &'host crate::EncryptedAgentVault<Keys>,
    capabilities: &'host FixtureCapabilities,
    persisted_revision: &'host AtomicU64,
}

#[cfg(unix)]
impl<Keys: crate::VaultKeyProvider> CapabilityHost for PersistedFixtureCapabilities<'_, Keys> {
    fn descriptors(&self, person_id: PersonId) -> Vec<CapabilityDescriptor> {
        self.capabilities.descriptors(person_id)
    }

    async fn invoke(&self, invocation: CapabilityInvocation) -> Result<String, AgentFailure> {
        let current = self
            .vault
            .expert_registry()
            .await?
            .ok_or(AgentFailure::VaultUnavailable)?;
        if current.revision != self.persisted_revision.load(Ordering::Acquire) {
            return Err(AgentFailure::Conflict);
        }
        self.capabilities.invoke(invocation).await
    }
}

#[cfg(unix)]
impl<Keys: crate::VaultKeyProvider> InProcessAgent for PersistedFixtureCapabilities<'_, Keys> {
    fn agent_cards(&self, person_id: PersonId) -> Vec<AgentCard> {
        self.capabilities.agent_cards(person_id)
    }

    async fn handle_message(
        &self,
        request: A2ASendMessageRequest,
    ) -> Result<A2ATask, AgentFailure> {
        let current = self
            .vault
            .expert_registry()
            .await?
            .ok_or(AgentFailure::VaultUnavailable)?;
        if current.revision != self.persisted_revision.load(Ordering::Acquire) {
            return Err(AgentFailure::Conflict);
        }
        self.capabilities.handle_message(request).await
    }
}

#[cfg(unix)]
struct ExpertSessionStore<'store, Keys> {
    vault: &'store crate::EncryptedAgentVault<Keys>,
    registry: &'store Mutex<AgentRegistry>,
    persisted_revision: AtomicU64,
}

#[cfg(unix)]
impl<Keys: crate::VaultKeyProvider> SessionStore for ExpertSessionStore<'_, Keys> {
    fn protection(&self) -> SessionProtection {
        self.vault.protection()
    }

    async fn load(
        &self,
        person_id: PersonId,
        session_id: Uuid,
    ) -> Result<AgentSession, AgentFailure> {
        self.vault.load(person_id, session_id).await
    }

    async fn compare_and_swap(
        &self,
        session: &AgentSession,
        previous_revision: u64,
    ) -> Result<(), AgentFailure> {
        let snapshot = self
            .registry
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .snapshot();
        let committed = self
            .vault
            .commit_expert_session(
                session,
                previous_revision,
                self.persisted_revision.load(Ordering::Acquire),
                &snapshot,
            )
            .await?;
        let revision = committed.revision;
        *self
            .registry
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)? =
            AgentRegistry::restore(committed, self.vault.registry_instance_id())?;
        self.persisted_revision.store(revision, Ordering::Release);
        Ok(())
    }
}

pub async fn recover_agent_sample(
    store: &impl SessionStore,
    person_id: PersonId,
    session_id: Uuid,
    expected_revision: u64,
) -> Result<AgentSession, AgentFailure> {
    if store.load(person_id, session_id).await?.scope.is_some() {
        return Err(AgentFailure::PolicyDenied);
    }
    let policy = fixture_policy();
    let capabilities = FixtureCapabilities::new(person_id)?;
    AgentRuntime {
        store,
        model: &FixtureModel {
            latency: Duration::ZERO,
        },
        capabilities: &capabilities,
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
                Some(AgentMessage::Delegation { .. })
            )
        {
            ModelStep::Delegate {
                agent_id: "floe.schedule".into(),
                message: "Review the synthetic sample day and identify relevant commitments and availability.".into(),
            }
        } else {
            let Some(AgentMessage::Delegation { task, .. }) = request.messages.last() else {
                return Ok(ModelResponse { replay: None, schema_version: AGENT_VERSION,
                    output: vec![ ModelStep::Answer { text: "The sample Schedule Expert is unavailable. No connected sources were read or changed.".into() }],
                    used_tokens: 32, cost_micros: 0 });
            };
            let result: ExpertResult = serde_json::from_str(
                task.data_part(EXPERT_RESULT_MEDIA_TYPE)
                    .ok_or(AgentFailure::InvalidModelOutput)?,
            )
            .map_err(|_| AgentFailure::InvalidModelOutput)?;
            if result.data_class != DataClass::Synthetic
                || !result
                    .insights
                    .iter()
                    .any(|insight| matches!(insight, ExpertInsight::FocusWindow { .. }))
            {
                return Err(AgentFailure::InvalidModelOutput);
            }
            ModelStep::Answer { text: "Sample briefing: Design review is at 10:00. There is a free hour afterward. No connected Calendar, mail, health or location data was read.".into() }
        };
        Ok(ModelResponse {
            replay: None,
            schema_version: AGENT_VERSION,
            output: vec![step],
            used_tokens: 32,
            cost_micros: 0,
        })
    }
}

pub(crate) struct FixtureCapabilities {
    person_id: PersonId,
    instance_id: Uuid,
    assignment_id: Uuid,
    registry: Mutex<AgentRegistry>,
    view: ExpertTimelineView,
}

impl FixtureCapabilities {
    fn new(person_id: PersonId) -> Result<Self, AgentFailure> {
        Self::new_with_instance(person_id, Uuid::new_v4())
    }

    #[cfg(unix)]
    pub(crate) fn snapshot(&self) -> Result<RegistrySnapshot, AgentFailure> {
        Ok(self
            .registry
            .lock()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .snapshot())
    }

    pub(crate) fn new_with_instance(
        person_id: PersonId,
        instance_id: Uuid,
    ) -> Result<Self, AgentFailure> {
        let handle = Uuid::new_v4();
        let mut registry = AgentRegistry::new(instance_id);
        let tool = PackageRef {
            kind: PackageKind::Tool,
            id: "floe.timeline.read".into(),
            version: "1.0.0".into(),
        };
        let expert = PackageRef {
            kind: PackageKind::Expert,
            id: "floe.schedule".into(),
            version: "1.0.0".into(),
        };
        registry.register(
            registry.revision(),
            AgentPackage {
                schema_version: 1,
                reference: tool.clone(),
                publisher: "floe".into(),
                implementation: PackageImplementation::TimelineRead {
                    data_class: DataClass::Synthetic,
                },
                expert_metadata: None,
                required_tools: vec![],
                state_schema_version: 1,
            },
        )?;
        registry.register(
            registry.revision(),
            AgentPackage {
                schema_version: 1,
                reference: expert.clone(),
                publisher: "floe".into(),
                implementation: PackageImplementation::Schedule,
                expert_metadata: Some(ExpertMetadata {
                    name: "Schedule Expert".into(),
                    description: "Reviews calendars, availability, conflicts, and the realism of plans from a scheduling perspective.".into(),
                    domain_tags: vec!["schedule".into(), "calendar".into()],
                    skills: vec!["Provide independent scheduling judgment".into()],
                }),
                required_tools: vec![tool.clone()],
                state_schema_version: 1,
            },
        )?;
        let tool_installation = registry.install(registry.revision(), &tool)?;
        let expert_installation = registry.install(registry.revision(), &expert)?;
        let tool_assignment = registry.assign(
            registry.revision(),
            person_id,
            tool_installation,
            vec![],
            vec![handle],
        )?;
        let assignment_id = registry.assign(
            registry.revision(),
            person_id,
            expert_installation,
            vec![tool_assignment],
            vec![handle],
        )?;
        for installation in [tool_installation, expert_installation] {
            registry.set_installation_enabled(registry.revision(), installation, true)?;
        }
        for assignment in [tool_assignment, assignment_id] {
            registry.set_assignment_enabled(registry.revision(), person_id, assignment, true)?;
        }
        Self::from_snapshot(person_id, registry.snapshot())
    }

    pub(crate) fn from_snapshot(
        person_id: PersonId,
        snapshot: RegistrySnapshot,
    ) -> Result<Self, AgentFailure> {
        let instance_id = snapshot.instance_id;
        let registry = AgentRegistry::restore(snapshot, instance_id)?;
        let snapshot = registry.snapshot();
        let installations: Vec<_> = snapshot
            .installations
            .iter()
            .filter(|entry| {
                entry.package.kind == PackageKind::Expert
                    && entry.package.id == "floe.schedule"
                    && entry.package.version == "1.0.0"
            })
            .map(|entry| entry.id)
            .collect();
        let assignments: Vec<_> = snapshot
            .assignments
            .iter()
            .filter(|entry| {
                entry.person_id == person_id && installations.contains(&entry.installation_id)
            })
            .collect();
        let [assignment] = assignments.as_slice() else {
            return Err(AgentFailure::Conflict);
        };
        let [handle] = assignment.granted_view_handles.as_slice() else {
            return Err(AgentFailure::CapabilityDenied);
        };
        let assignment_id = assignment.id;
        let handle = *handle;
        Ok(Self {
            person_id,
            instance_id,
            assignment_id,
            registry: Mutex::new(registry),
            view: ExpertTimelineView {
                schema_version: 1,
                handle,
                person_id,
                data_class: DataClass::Synthetic,
                source_handle: "fixture.synthetic.timeline".into(),
                range_start_unix_ms: 36_000_000,
                range_end_unix_ms: 43_200_000,
                expires_at_unix_ms: u64::MAX,
                items: vec![TimelineViewItem {
                    evidence_handle: Uuid::from_u128(handle.as_u128() ^ 1),
                    untrusted_title: "Design review".into(),
                    starts_at_unix_ms: 36_000_000,
                    ends_at_unix_ms: 39_600_000,
                }],
            },
        })
    }

    #[cfg(unix)]
    fn ensure_snapshot(
        person_id: PersonId,
        mut snapshot: RegistrySnapshot,
    ) -> Result<RegistrySnapshot, AgentFailure> {
        if snapshot.packages.iter().any(|package| {
            matches!(
                package.reference.id.as_str(),
                "floe.timeline.read" | "floe.schedule"
            )
        }) {
            return Ok(snapshot);
        }
        let sample = Self::new_with_instance(person_id, snapshot.instance_id)?.snapshot()?;
        snapshot.packages.extend(sample.packages);
        snapshot.installations.extend(sample.installations);
        snapshot.assignments.extend(sample.assignments);
        snapshot.revision = snapshot
            .revision
            .checked_add(1)
            .ok_or(AgentFailure::BudgetExceeded)?;
        let instance_id = snapshot.instance_id;
        Ok(AgentRegistry::restore(snapshot, instance_id)?.snapshot())
    }
}

impl CapabilityHost for FixtureCapabilities {
    fn descriptors(&self, _: PersonId) -> Vec<CapabilityDescriptor> {
        vec![]
    }

    async fn invoke(&self, _: CapabilityInvocation) -> Result<String, AgentFailure> {
        Err(AgentFailure::CapabilityDenied)
    }
}

impl InProcessAgent for FixtureCapabilities {
    fn agent_cards(&self, person_id: PersonId) -> Vec<AgentCard> {
        if person_id != self.person_id {
            return vec![];
        }
        self.registry
            .lock()
            .ok()
            .and_then(|registry| {
                registry
                    .expert_card(
                        person_id,
                        self.assignment_id,
                        registry.revision(),
                        self.view.handle,
                    )
                    .ok()
            })
            .into_iter()
            .collect()
    }

    async fn handle_message(
        &self,
        request: A2ASendMessageRequest,
    ) -> Result<A2ATask, AgentFailure> {
        if request.person_id != self.person_id
            || request.agent_id != "floe.schedule"
            || request.message.role != A2AMessageRole::User
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let task_id = request.message.task_id.ok_or(AgentFailure::InvalidInput)?;
        let assignment = request.message.text()?.to_owned();
        let expected_registry_revision = self
            .registry
            .lock()
            .map_err(|_| AgentFailure::CapabilityUnavailable)?
            .revision();
        let result = ExpertHost {
            registry: &self.registry,
            views: self,
        }
        .invoke(ExpertInvocation {
            usage: request.usage,
            schema_version: request.schema_version,
            invocation_id: task_id,
            instance_id: self.instance_id,
            person_id: request.person_id,
            assignment_id: self.assignment_id,
            expected_registry_revision,
            granted_view_handles: vec![self.view.handle],
            allowed_data_classes: vec![DataClass::Synthetic],
            current_time_unix_ms: self.view.range_start_unix_ms,
            timezone_offset_seconds: 0,
            input: ExpertInput::Analyze {
                request: assignment,
                focus_minutes: Some(60),
            },
            budget: ExpertBudget {
                max_output_bytes: request.max_output_bytes,
                ..ExpertBudget::default()
            },
            deadline: request.deadline,
            cancellation: request.cancellation,
        })
        .await?;
        let data = serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?;
        Ok(A2ATask {
            id: task_id,
            context_id: request.message.context_id,
            agent_id: request.agent_id,
            state: A2ATaskState::Completed,
            history: vec![request.message],
            artifacts: vec![A2AArtifact {
                artifact_id: Uuid::new_v4(),
                name: "Synthetic schedule result".into(),
                parts: vec![
                    A2APart::Text {
                        text: "The synthetic sample contains a commitment and an available window."
                            .into(),
                    },
                    A2APart::Data {
                        media_type: EXPERT_RESULT_MEDIA_TYPE.into(),
                        data,
                    },
                ],
            }],
            failure: None,
        })
    }
}

impl ExpertViews for FixtureCapabilities {
    async fn timeline(
        &self,
        request: TimelineViewRead,
    ) -> Result<ExpertTimelineView, AgentFailure> {
        if request.person_id != self.person_id || request.handle != self.view.handle {
            return Err(AgentFailure::CapabilityDenied);
        }
        if self.view.items.len() > request.max_items
            || serde_json::to_vec(&self.view)
                .map_err(|_| AgentFailure::InvalidInput)?
                .len()
                > request.max_bytes
        {
            return Err(AgentFailure::BudgetExceeded);
        }
        Ok(self.view.clone())
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
