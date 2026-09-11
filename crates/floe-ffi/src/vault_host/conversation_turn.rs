use floe_agent::{
    A2AArtifact, A2AMessageRole, A2APart, A2ASendMessageRequest, A2ATask, A2ATaskState,
    AGENT_VERSION, AgentBudget, AgentCard, AgentCommand, AgentContext, AgentEvent, AgentFailure,
    AgentRuntime, AttentionView, BuiltinContextSource, BuiltinExpertKind,
    BuiltinExpertSetupReceipt, CalendarContextView, CapabilityDescriptor, CapabilityHost,
    CapabilityInvocation, CommitmentsContextViews, CommitmentsExpertResult,
    CommunicationExpertResult, DataClass, EXPERT_RESULT_MEDIA_TYPE, FeasibilityView,
    FocusContextViews, FocusExpertResult, InProcessA2ATransport, InProcessAgent,
    InferencePolicyDecision, LifeLogisticsExpertResult, MailExpertInvocation, ModelPlacement,
    ModelRequest, ModelResponse, ModelRunner, NativeContextView, PeopleView,
    PersonalExpertInvocation, PortfolioExpertInvocation, RelationshipsContextViews,
    RelationshipsExpertResult, SessionStore, TransferConsent, WellbeingContextViews,
    WellbeingExpertResult, WellbeingView, WorkContextExpertResult,
    run_commitments_expert_with_views, run_communication_expert, run_focus_expert_with_views,
    run_life_logistics_expert, run_relationships_expert_with_views,
    run_wellbeing_expert_with_views, run_work_context_expert,
};
use floe_core::{EncryptedAgentVault, FloeCore, VaultKeyProvider};
use floe_domain::PersonId;
use floe_protocol::{
    AgentConversationTurnRequestDto, AgentRemoteCalendarConnectionDto, AgentRemoteRouteDto,
};

use crate::local_context::LocalContextStore;
use crate::{
    local_model::FoundationModelRunner,
    remote_model::{CalendarContextRequest, ServerModelRunner},
};

use super::session_uuid;

struct ConversationTurnInputs<'a, Keys: VaultKeyProvider> {
    core: &'a FloeCore,
    vault: &'a EncryptedAgentVault<Keys>,
    local_context: &'a LocalContextStore,
    person_id: PersonId,
    request: &'a AgentConversationTurnRequestDto,
}

pub(super) async fn run<Keys: VaultKeyProvider>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    local_context: &LocalContextStore,
    person_id: PersonId,
    request: &AgentConversationTurnRequestDto,
    cancellation: floe_agent::Cancellation,
    emit: impl FnMut(AgentEvent) + Send,
) -> Result<floe_agent::AgentSession, AgentFailure> {
    let text = request.text.trim();
    if text.is_empty()
        || text.len() > 8_192
        || request.device_id.trim().is_empty()
        || request.device_id.len() > 128
    {
        return Err(AgentFailure::InvalidInput);
    }
    let session_id = session_uuid(&request.session_id)?;
    let session = vault.load(person_id, session_id).await?;
    if session.scope.is_some()
        || session.data_classes != [DataClass::Personal]
        || session.revision != request.expected_revision
    {
        return Err(AgentFailure::Conflict);
    }
    let context = AgentContext {
        projection_version: 1,
        persona: None,
        memories: vault.personal_memory_context(chrono::Utc::now()).await?,
        evidence: vec![],
    };
    let inputs = ConversationTurnInputs {
        core,
        vault,
        local_context,
        person_id,
        request,
    };
    expert_dispatch::run(&inputs, context, cancellation, emit).await
}

async fn run_general_turn<Keys: VaultKeyProvider>(
    inputs: &ConversationTurnInputs<'_, Keys>,
    context: AgentContext,
    cancellation: floe_agent::Cancellation,
    emit: impl FnMut(AgentEvent) + Send,
) -> Result<floe_agent::AgentSession, AgentFailure> {
    let core = inputs.core;
    let vault = inputs.vault;
    let local_context = inputs.local_context;
    let person_id = inputs.person_id;
    let request = inputs.request;
    let session_id = session_uuid(&request.session_id)?;
    let model = Model::new(request.remote_route.clone())?;
    let policy = policy(&model, request.remote_route.as_ref());
    let task_views = optional_task_views(core, person_id).await?;
    let expert_cards = vault.enabled_expert_cards().await?;
    let builtin_setup = vault
        .builtin_expert_overview()
        .await?
        .ok_or(AgentFailure::VaultUnavailable)?
        .setup;
    let capabilities = ConversationCapabilities {
        model: &model,
        policy: &policy,
        local_context,
    };
    let experts = ConversationExperts {
        model: &model,
        policy: &policy,
        context: &context,
        local_context,
        task_views: &task_views,
        cards: expert_cards,
        builtin_setup: Some(builtin_setup),
    };
    let agents = InProcessA2ATransport::new(&experts);
    let runtime = AgentRuntime {
        store: vault,
        model: &model,
        capabilities: &capabilities,
        policy: &policy,
        budget: AgentBudget::default(),
    };
    if request.continuation {
        runtime
            .continue_turn_with_agents(
                person_id,
                session_id,
                request.expected_revision,
                context.clone(),
                &agents,
                cancellation,
                emit,
            )
            .await
    } else {
        runtime
            .run_turn_with_agents(
                AgentCommand {
                    schema_version: AGENT_VERSION,
                    person_id,
                    session_id,
                    expected_revision: request.expected_revision,
                    text: request.text.trim().into(),
                },
                context.clone(),
                &agents,
                cancellation,
                emit,
            )
            .await
    }
}

async fn optional_task_views(
    core: &FloeCore,
    person_id: PersonId,
) -> Result<Vec<NativeContextView>, AgentFailure> {
    let handle = uuid::Uuid::new_v5(&person_id.0, b"floe.tasks");
    match core
        .task_context_view(person_id, handle, chrono::Utc::now(), 16, 8 * 1024)
        .await
    {
        Ok(view) => Ok(vec![view]),
        Err(AgentFailure::CapabilityUnavailable) => Ok(vec![]),
        Err(error) => Err(error),
    }
}

pub(super) async fn recover<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    person_id: PersonId,
    session_id: &str,
    expected_revision: u64,
) -> Result<floe_agent::AgentSession, AgentFailure> {
    let session_id = session_uuid(session_id)?;
    let session = vault.load(person_id, session_id).await?;
    if session.scope.is_some() || session.data_classes != [DataClass::Personal] {
        return Err(AgentFailure::PolicyDenied);
    }
    let model = Model::Foundation(FoundationModelRunner::encrypted());
    let policy = policy(&model, None);
    AgentRuntime {
        store: vault,
        model: &model,
        capabilities: &NoCapabilities,
        policy: &policy,
        budget: AgentBudget::default(),
    }
    .recover_interrupted(person_id, session_id, expected_revision)
    .await
}

fn policy(model: &Model, route: Option<&AgentRemoteRouteDto>) -> InferencePolicyDecision {
    InferencePolicyDecision {
        purpose: "everyday-assistance".into(),
        data_classes: vec![DataClass::Personal],
        allowed_placements: vec![model.placement()],
        performance_class: "interactive".into(),
        projection_version: 1,
        external_transfer_consent: if route
            .is_some_and(|route| route.external && route.allow_external)
        {
            TransferConsent::Granted
        } else {
            TransferConsent::NotGranted
        },
        bounded_sensitive_projection: false,
    }
}

enum Model {
    Foundation(FoundationModelRunner),
    Server(ServerModelRunner),
}

impl Model {
    fn new(route: Option<AgentRemoteRouteDto>) -> Result<Self, AgentFailure> {
        match route {
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

struct PersonalViewSource<'a> {
    model: &'a Model,
    policy: &'a InferencePolicyDecision,
    local_context: &'a LocalContextStore,
    person_id: PersonId,
}

impl PersonalViewSource<'_> {
    async fn people_view(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_agent::Cancellation,
    ) -> Result<PeopleView, AgentFailure> {
        match self.local_context.people(self.person_id) {
            Ok(view) => Ok(view),
            Err(AgentFailure::CapabilityUnavailable) if self.server_fallback_allowed() => {
                let Model::Server(model) = self.model else {
                    unreachable!()
                };
                model.read_people_view(deadline, cancellation).await
            }
            Err(error) => Err(error),
        }
    }

    async fn feasibility_view(&self) -> Result<FeasibilityView, AgentFailure> {
        self.local_context.feasibility(self.person_id)
    }

    async fn attention_view(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_agent::Cancellation,
    ) -> Result<AttentionView, AgentFailure> {
        match self.local_context.attention(self.person_id) {
            Ok(view) => Ok(view),
            Err(AgentFailure::CapabilityUnavailable) if self.server_fallback_allowed() => {
                let Model::Server(model) = self.model else {
                    unreachable!()
                };
                model.read_attention_view(deadline, cancellation).await
            }
            Err(error) => Err(error),
        }
    }

    async fn wellbeing_view(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_agent::Cancellation,
    ) -> Result<WellbeingView, AgentFailure> {
        match self.local_context.wellbeing(self.person_id) {
            Ok(view) => Ok(view),
            Err(AgentFailure::CapabilityUnavailable) if self.server_fallback_allowed() => {
                let Model::Server(model) = self.model else {
                    unreachable!()
                };
                model.read_wellbeing_view(deadline, cancellation).await
            }
            Err(error) => Err(error),
        }
    }

    async fn calendar_views(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_agent::Cancellation,
    ) -> Result<Vec<CalendarContextView>, AgentFailure> {
        let Model::Server(model) = self.model else {
            return Ok(vec![]);
        };
        if !self.server_fallback_allowed() {
            return Ok(vec![]);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map_err(|_| AgentFailure::StaleContext)?;
        let now = i64::try_from(now.as_millis()).map_err(|_| AgentFailure::StaleContext)?;
        let mut views = Vec::new();
        for connection in model.calendar_connections() {
            match model
                .read_calendar_context_view(
                    CalendarContextRequest {
                        connector_id: &connection.connector_id,
                        connection_id: &connection.connection_id,
                        connection_revision: connection.connection_revision,
                        range_start_unix_ms: now.saturating_sub(86_400_000),
                        range_end_unix_ms: now.saturating_add(86_400_000),
                        cursor: "",
                        limit: floe_agent::MAX_CALENDAR_CONTEXT_ITEMS,
                    },
                    deadline,
                    cancellation,
                )
                .await
            {
                Ok(view) => views.push(view),
                Err(AgentFailure::CapabilityUnavailable) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(views)
    }

    async fn confirmed_interaction_views(
        &self,
        people: &PeopleView,
        deadline: tokio::time::Instant,
        cancellation: &floe_agent::Cancellation,
    ) -> Result<Vec<floe_agent::ConfirmedInteractionView>, AgentFailure> {
        let Model::Server(model) = self.model else {
            return Ok(vec![]);
        };
        if !self.server_fallback_allowed() {
            return Ok(vec![]);
        }
        match model
            .read_confirmed_interaction_view(people, deadline, cancellation)
            .await
        {
            Ok(view) => Ok(vec![view]),
            Err(AgentFailure::CapabilityUnavailable) => Ok(vec![]),
            Err(error) => Err(error),
        }
    }

    async fn work_context_views(
        &self,
        deadline: tokio::time::Instant,
        cancellation: &floe_agent::Cancellation,
    ) -> Result<Vec<floe_agent::WorkContextView>, AgentFailure> {
        let Model::Server(model) = self.model else {
            return Ok(vec![]);
        };
        if !self.server_fallback_allowed() {
            return Ok(vec![]);
        }
        match model.read_work_context_view(deadline, cancellation).await {
            Ok(view) => Ok(vec![view]),
            Err(AgentFailure::CapabilityUnavailable) => Ok(vec![]),
            Err(error) => Err(error),
        }
    }

    fn server_fallback_allowed(&self) -> bool {
        matches!(self.model, Model::Server(_))
            && (self.model.placement() == ModelPlacement::DeviceLocal
                || self.policy.external_transfer_consent == TransferConsent::Granted)
    }
}

struct NoCapabilities;

impl CapabilityHost for NoCapabilities {
    fn descriptors(&self, _: PersonId) -> Vec<CapabilityDescriptor> {
        vec![]
    }

    async fn invoke(&self, _: CapabilityInvocation) -> Result<String, AgentFailure> {
        Err(AgentFailure::CapabilityDenied)
    }
}

struct ConversationCapabilities<'model> {
    model: &'model Model,
    policy: &'model InferencePolicyDecision,
    local_context: &'model LocalContextStore,
}

impl CapabilityHost for ConversationCapabilities<'_> {
    fn descriptors(&self, _: PersonId) -> Vec<CapabilityDescriptor> {
        let mut descriptors = vec![
            read_capability("people.identity.read"),
            read_capability("schedule.feasibility.read"),
            read_capability("attention.coarse.read"),
            read_capability("wellbeing.derived.read"),
        ];
        if matches!(self.model, Model::Server(_)) {
            descriptors.extend([
                CapabilityDescriptor {
                    schema_version: AGENT_VERSION,
                    id: "mail.communication.read".into(),
                    version: "1.0.0".into(),
                    read_only: true,
                    output_data_class: DataClass::Personal,
                    input_schema: Some(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "query": {"type": "string", "maxLength": 512},
                            "cursor": {"type": "integer", "minimum": 0, "maximum": 10000},
                            "limit": {"type": "integer", "minimum": 1, "maximum": 100}
                        },
                        "additionalProperties": false
                    })),
                },
                read_capability("work.context.read"),
                read_capability("life.logistics.read"),
            ]);
        }
        descriptors
    }

    async fn invoke(&self, invocation: CapabilityInvocation) -> Result<String, AgentFailure> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            #[serde(default)]
            query: String,
            #[serde(default)]
            cursor: usize,
            #[serde(default = "default_communication_limit")]
            limit: usize,
        }
        let output = match invocation.capability_id.as_str() {
            "mail.communication.read" => {
                let Model::Server(model) = self.model else {
                    return Err(AgentFailure::CapabilityUnavailable);
                };
                let input: Input = serde_json::from_str(&invocation.input)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                serde_json::to_value(
                    model
                        .read_communication_view(
                            &input.query,
                            input.cursor,
                            input.limit,
                            invocation.deadline,
                            &invocation.cancellation,
                        )
                        .await?,
                )
            }
            "work.context.read"
            | "life.logistics.read"
            | "people.identity.read"
            | "schedule.feasibility.read"
            | "attention.coarse.read"
            | "wellbeing.derived.read" => {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Empty {}
                serde_json::from_str::<Empty>(&invocation.input)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                let personal = PersonalViewSource {
                    model: self.model,
                    policy: self.policy,
                    local_context: self.local_context,
                    person_id: invocation.person_id,
                };
                match invocation.capability_id.as_str() {
                    "work.context.read" => {
                        let Model::Server(model) = self.model else {
                            return Err(AgentFailure::CapabilityUnavailable);
                        };
                        serde_json::to_value(
                            model
                                .read_work_context_view(
                                    invocation.deadline,
                                    &invocation.cancellation,
                                )
                                .await?,
                        )
                    }
                    "life.logistics.read" => {
                        let Model::Server(model) = self.model else {
                            return Err(AgentFailure::CapabilityUnavailable);
                        };
                        serde_json::to_value(
                            model
                                .read_logistics_view(invocation.deadline, &invocation.cancellation)
                                .await?,
                        )
                    }
                    "people.identity.read" => serde_json::to_value(
                        personal
                            .people_view(invocation.deadline, &invocation.cancellation)
                            .await?,
                    ),
                    "schedule.feasibility.read" => {
                        serde_json::to_value(personal.feasibility_view().await?)
                    }
                    "attention.coarse.read" => serde_json::to_value(
                        personal
                            .attention_view(invocation.deadline, &invocation.cancellation)
                            .await?,
                    ),
                    "wellbeing.derived.read" => serde_json::to_value(
                        personal
                            .wellbeing_view(invocation.deadline, &invocation.cancellation)
                            .await?,
                    ),
                    _ => unreachable!(),
                }
            }
            _ => return Err(AgentFailure::CapabilityDenied),
        }
        .map_err(|_| AgentFailure::InvalidInput)?;
        serde_json::to_string(&output).map_err(|_| AgentFailure::InvalidInput)
    }
}

fn read_capability(id: &str) -> CapabilityDescriptor {
    CapabilityDescriptor {
        schema_version: AGENT_VERSION,
        id: id.into(),
        version: "1.0.0".into(),
        read_only: true,
        output_data_class: DataClass::Personal,
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })),
    }
}

fn default_communication_limit() -> usize {
    25
}

mod expert_dispatch;
use expert_dispatch::ConversationExperts;

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    const COMMITMENTS_AGENT_ID: &str = BuiltinExpertKind::Commitments.package_id();
    const COMMUNICATION_AGENT_ID: &str = BuiltinExpertKind::Communication.package_id();
    const WORK_CONTEXT_AGENT_ID: &str = BuiltinExpertKind::WorkContext.package_id();
    const LIFE_LOGISTICS_AGENT_ID: &str = BuiltinExpertKind::LifeLogistics.package_id();
    const RELATIONSHIPS_AGENT_ID: &str = BuiltinExpertKind::Relationships.package_id();
    const FOCUS_AGENT_ID: &str = BuiltinExpertKind::FocusAttention.package_id();
    const WELLBEING_AGENT_ID: &str = BuiltinExpertKind::Wellbeing.package_id();

    fn test_expert_cards() -> Vec<AgentCard> {
        [
            (COMMITMENTS_AGENT_ID, "Commitments Expert"),
            (COMMUNICATION_AGENT_ID, "Communication Expert"),
            (WORK_CONTEXT_AGENT_ID, "Work Context Expert"),
            (LIFE_LOGISTICS_AGENT_ID, "Life Logistics Expert"),
            (RELATIONSHIPS_AGENT_ID, "Relationships Expert"),
            (FOCUS_AGENT_ID, "Focus & Attention Expert"),
            (WELLBEING_AGENT_ID, "Wellbeing Expert"),
        ]
        .into_iter()
        .map(|(id, name)| AgentCard {
            schema_version: AGENT_VERSION,
            protocol_version: floe_agent::A2A_PROTOCOL_VERSION.into(),
            id: id.into(),
            version: "1.0.0".into(),
            name: name.into(),
            description: format!("Bounded {name} fixture."),
            domain_tags: vec!["test".into()],
            skills: vec!["Read bounded context".into()],
        })
        .collect()
    }

    fn test_builtin_setup(
        person_id: PersonId,
        source: BuiltinContextSource,
    ) -> BuiltinExpertSetupReceipt {
        let instance_id = uuid::Uuid::new_v4();
        let mut registry = floe_agent::AgentRegistry::new(instance_id);
        registry
            .install_builtin_experts(
                person_id,
                &floe_agent::BuiltinExpertSetup {
                    instance_id,
                    expected_revision: 0,
                    setup_id: uuid::Uuid::new_v4(),
                    sources: vec![floe_agent::BuiltinSourceBinding {
                        source,
                        view_handle: uuid::Uuid::new_v4(),
                        state: floe_agent::BuiltinSourceState::Available,
                    }],
                },
            )
            .unwrap()
    }

    async fn request(mut socket: tokio::net::TcpStream) -> (String, tokio::net::TcpStream) {
        let mut bytes = Vec::new();
        let length = loop {
            let mut chunk = [0_u8; 4096];
            let read = socket.read(&mut chunk).await.unwrap();
            bytes.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&bytes);
            if let Some(header_end) = text.find("\r\n\r\n") {
                let content_length = text[..header_end]
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|value| value.parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= header_end + 4 + content_length {
                    break header_end + 4 + content_length;
                }
            }
        };
        (String::from_utf8(bytes[..length].to_vec()).unwrap(), socket)
    }

    fn assert_calendar_request_contract(request: &str) {
        assert!(request.starts_with("POST /v1/views/calendar.timeline "));
        let body: serde_json::Value =
            serde_json::from_str(request.split_once("\r\n\r\n").expect("HTTP request body").1)
                .unwrap();
        assert_eq!(body["schema_version"], 1);
        assert_eq!(body["connector_id"], "calendar.google");
        assert_eq!(
            body["connection_id"],
            "00000000-0000-4000-8000-000000000010"
        );
        assert_eq!(body["connection_revision"], 7);
        assert!(body["range_start_unix_ms"].as_i64().unwrap() >= 0);
        assert!(
            body["range_end_unix_ms"].as_i64().unwrap()
                > body["range_start_unix_ms"].as_i64().unwrap()
        );
        assert_eq!(body["cursor"], "");
        assert_eq!(body["limit"], floe_agent::MAX_CALENDAR_CONTEXT_ITEMS);
        assert_eq!(body.as_object().unwrap().len(), 8);
    }

    fn test_calendar_connections() -> Vec<AgentRemoteCalendarConnectionDto> {
        vec![AgentRemoteCalendarConnectionDto {
            connector_id: "calendar.google".into(),
            connection_id: "00000000-0000-4000-8000-000000000010".into(),
            connection_revision: 7,
        }]
    }

    async fn respond(mut socket: tokio::net::TcpStream, body: String) {
        socket
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(), body
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    }

    async fn respond_not_found(mut socket: tokio::net::TcpStream) {
        socket
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
    }

    #[test]
    fn configured_daily_route_takes_priority_over_the_device_model() {
        let model = Model::new(Some(AgentRemoteRouteDto {
            base_url: "http://127.0.0.1:8431".into(),
            bearer_token: "daily_route_token_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
            calendar_connections: vec![],
        }))
        .unwrap();
        assert!(matches!(model, Model::Server(_)));
    }

    #[test]
    fn missing_daily_route_uses_the_device_model() {
        assert!(matches!(Model::new(None).unwrap(), Model::Foundation(_)));
    }

    #[tokio::test]
    async fn device_model_has_no_implicit_cross_device_personal_view_route() {
        let model = Model::new(None).unwrap();
        let policy = policy(&model, None);
        let local_context = LocalContextStore::default();
        let result = PersonalViewSource {
            model: &model,
            policy: &policy,
            local_context: &local_context,
            person_id: PersonId::new(),
        }
        .attention_view(
            tokio::time::Instant::now() + std::time::Duration::from_secs(1),
            &floe_agent::Cancellation::default(),
        )
        .await;
        assert_eq!(result, Err(AgentFailure::CapabilityUnavailable));
    }

    #[tokio::test]
    async fn expert_cannot_read_a_source_missing_from_its_durable_grant() {
        let model = Model::new(Some(AgentRemoteRouteDto {
            base_url: "http://127.0.0.1:1".into(),
            bearer_token: "test_token_that_is_long_enough_to_validate".into(),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
            calendar_connections: vec![],
        }))
        .unwrap();
        let policy = policy(&model, None);
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            evidence: vec![],
        };
        let local_context = LocalContextStore::default();
        let person_id = PersonId::new();
        let experts = ConversationExperts {
            model: &model,
            policy: &policy,
            context: &context,
            local_context: &local_context,
            task_views: &[],
            cards: test_expert_cards(),
            builtin_setup: Some(test_builtin_setup(person_id, BuiltinContextSource::Tasks)),
        };
        let result = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_agent::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                person_id,
                session_id: uuid::Uuid::new_v4(),
                parent_turn_id: uuid::Uuid::new_v4(),
                agent_id: COMMITMENTS_AGENT_ID.into(),
                message: floe_agent::A2AMessage {
                    message_id: uuid::Uuid::new_v4(),
                    context_id: uuid::Uuid::new_v4(),
                    task_id: Some(uuid::Uuid::new_v4()),
                    role: A2AMessageRole::User,
                    parts: vec![A2APart::Text {
                        text: "review commitments".into(),
                    }],
                },
                max_output_bytes: 16 * 1024,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                cancellation: floe_agent::Cancellation::default(),
            })
            .await;
        assert_eq!(result, Err(AgentFailure::CapabilityDenied));
    }

    #[tokio::test]
    async fn device_model_reads_person_bound_local_context() {
        let model = Model::new(None).unwrap();
        let policy = policy(&model, None);
        let local_context = LocalContextStore::default();
        let person_id = PersonId::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        local_context
            .request(
                person_id,
                floe_protocol::LocalContextOperationDto::Publish {
                    device_id: "mac-local".into(),
                    view: serde_json::json!({
                        "schema_version": AGENT_VERSION,
                        "view_id": "attention.coarse",
                        "source_handle": "attention:macos_local",
                        "observed_at_unix_ms": now - 1,
                        "expires_at_unix_ms": now + 60_000,
                        "state": "focused",
                        "confidence_millis": 750,
                        "evidence_handles": ["activity:coarse"]
                    }),
                },
            )
            .unwrap();
        let capabilities = ConversationCapabilities {
            model: &model,
            policy: &policy,
            local_context: &local_context,
        };

        let output = capabilities
            .invoke(CapabilityInvocation {
                usage: floe_agent::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                call_id: uuid::Uuid::new_v4(),
                person_id,
                session_id: uuid::Uuid::new_v4(),
                turn_id: uuid::Uuid::new_v4(),
                capability_id: "attention.coarse.read".into(),
                input: "{}".into(),
                max_output_bytes: 65_536,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                cancellation: floe_agent::Cancellation::default(),
            })
            .await
            .unwrap();
        let view: AttentionView = serde_json::from_str(&output).unwrap();
        assert_eq!(view.source_handle, "attention:macos_local");
    }

    #[test]
    fn server_route_exposes_only_bounded_context_observe_capabilities() {
        let model = Model::new(Some(AgentRemoteRouteDto {
            base_url: "http://127.0.0.1:8431".into(),
            bearer_token: "daily_route_token_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
            calendar_connections: vec![],
        }))
        .unwrap();
        let policy = policy(&model, None);
        let local_context = LocalContextStore::default();
        let capabilities = ConversationCapabilities {
            model: &model,
            policy: &policy,
            local_context: &local_context,
        };
        let descriptors = capabilities.descriptors(PersonId::new());
        assert_eq!(descriptors.len(), 7);
        assert_eq!(
            descriptors
                .iter()
                .map(|descriptor| descriptor.id.as_str())
                .collect::<Vec<_>>(),
            [
                "people.identity.read",
                "schedule.feasibility.read",
                "attention.coarse.read",
                "wellbeing.derived.read",
                "mail.communication.read",
                "work.context.read",
                "life.logistics.read"
            ]
        );
        for descriptor in &descriptors {
            assert!(descriptor.read_only);
            assert_eq!(descriptor.output_data_class, DataClass::Personal);
            let schema = descriptor.input_schema.as_ref().unwrap();
            assert_eq!(schema["additionalProperties"], false);
            assert!(schema.to_string().len() < 1024);
        }
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            evidence: vec![],
        };
        let experts = ConversationExperts {
            model: &model,
            policy: &policy,
            context: &context,
            local_context: &local_context,
            task_views: &[],
            cards: test_expert_cards(),
            builtin_setup: None,
        };
        let cards = experts.agent_cards(PersonId::new());
        assert_eq!(cards.len(), 7);
        assert_eq!(cards[0].id, COMMITMENTS_AGENT_ID);
        assert_eq!(cards[1].id, COMMUNICATION_AGENT_ID);
        assert_eq!(cards[2].id, WORK_CONTEXT_AGENT_ID);
        assert_eq!(cards[3].id, LIFE_LOGISTICS_AGENT_ID);
        assert_eq!(cards[4].id, RELATIONSHIPS_AGENT_ID);
        assert_eq!(cards[5].id, FOCUS_AGENT_ID);
        assert_eq!(cards[6].id, WELLBEING_AGENT_ID);
        assert!(cards.iter().all(|card| card.validate().is_ok()));
    }

    #[tokio::test]
    async fn unregistered_expert_card_cannot_be_invoked_directly() {
        let model = Model::new(None).unwrap();
        let policy = policy(&model, None);
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            evidence: vec![],
        };
        let local_context = LocalContextStore::default();
        let experts = ConversationExperts {
            model: &model,
            policy: &policy,
            context: &context,
            local_context: &local_context,
            task_views: &[],
            cards: vec![],
            builtin_setup: None,
        };
        let result = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_agent::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                person_id: PersonId::new(),
                session_id: uuid::Uuid::new_v4(),
                parent_turn_id: uuid::Uuid::new_v4(),
                agent_id: RELATIONSHIPS_AGENT_ID.into(),
                message: floe_agent::A2AMessage {
                    message_id: uuid::Uuid::new_v4(),
                    context_id: uuid::Uuid::new_v4(),
                    task_id: Some(uuid::Uuid::new_v4()),
                    role: A2AMessageRole::User,
                    parts: vec![A2APart::Text {
                        text: "Find a follow-up.".into(),
                    }],
                },
                max_output_bytes: 16_384,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                cancellation: floe_agent::Cancellation::default(),
            })
            .await;
        assert_eq!(result, Err(AgentFailure::CapabilityDenied));
    }

    #[tokio::test]
    async fn mail_capability_rejects_authority_escalation_before_io() {
        let model = Model::new(Some(AgentRemoteRouteDto {
            base_url: "http://127.0.0.1:1".into(),
            bearer_token: "daily_route_token_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
            calendar_connections: vec![],
        }))
        .unwrap();
        let policy = policy(&model, None);
        let local_context = LocalContextStore::default();
        let capabilities = ConversationCapabilities {
            model: &model,
            policy: &policy,
            local_context: &local_context,
        };
        let result = capabilities
            .invoke(CapabilityInvocation {
                usage: floe_agent::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                call_id: uuid::Uuid::new_v4(),
                person_id: PersonId::new(),
                session_id: uuid::Uuid::new_v4(),
                turn_id: uuid::Uuid::new_v4(),
                capability_id: "mail.communication.read".into(),
                input: r#"{"query":"reply","authority":"send"}"#.into(),
                max_output_bytes: 65_536,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(1),
                cancellation: floe_agent::Cancellation::default(),
            })
            .await;
        assert_eq!(result, Err(AgentFailure::InvalidInput));
    }

    #[tokio::test]
    async fn commitments_delegation_reads_fresh_view_and_returns_typed_artifact() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (view_request, socket) = request(socket).await;
            assert!(view_request.starts_with("POST /v1/views/mail.communication "));
            respond(
                socket,
                serde_json::json!({
                    "schema_version": 1,
                    "view": {
                        "schema_version": 1,
                        "view_id": "mail.communication",
                        "source_handle": "mail:fresh",
                        "observed_at_unix_ms": now - 1,
                        "expires_at_unix_ms": now + 299_999,
                        "coverage_complete": true,
                        "items": [{
                            "evidence_handle": "mail:request",
                            "thread_handle": "mail:thread",
                            "received_unix_ms": now - 2,
                            "from": "alex@example.com",
                            "to": "person@example.com",
                            "subject": "Confirm by Friday",
                            "snippet": "Please confirm the review by Friday.",
                            "labels": ["INBOX"]
                        }]
                    }
                })
                .to_string(),
            )
            .await;

            let (socket, _) = listener.accept().await.unwrap();
            let (calendar_request, socket) = request(socket).await;
            assert_calendar_request_contract(&calendar_request);
            respond_not_found(socket).await;

            let (socket, _) = listener.accept().await.unwrap();
            let (model_request, socket) = request(socket).await;
            assert!(model_request.starts_with("POST /v1/agent "));
            assert!(model_request.contains("Commitments Expert"));
            assert!(model_request.contains("Confirm by Friday"));
            let answer = serde_json::json!({
                "summary": "A reply and Friday commitment are requested.",
                "findings": [{
                    "evidence_handle": "mail:request",
                    "kind": "request_to_user",
                    "statement": "Confirm the review by Friday.",
                    "epistemic_status": "observed",
                    "confidence_millis": 1000
                }]
            })
            .to_string();
            let output = serde_json::json!({
                "output": [{"kind": "answer", "text": answer}],
                "used_tokens": 64,
                "call_ids": []
            })
            .to_string();
            respond(
                socket,
                serde_json::json!({
                    "schema_version": 1,
                    "purpose": "everyday_assistance",
                    "output": output,
                    "trace_id": "0123456789abcdef0123456789abcdef",
                    "routing": {
                        "placement": "server_local",
                        "external_transfer": false,
                        "replay_source": "a".repeat(64)
                    }
                })
                .to_string(),
            )
            .await;
        });
        let route = AgentRemoteRouteDto {
            base_url: format!("http://{address}"),
            bearer_token: "daily_route_token_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
            calendar_connections: test_calendar_connections(),
        };
        let model = Model::new(Some(route.clone())).unwrap();
        let policy = policy(&model, Some(&route));
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            evidence: vec![],
        };
        let local_context = LocalContextStore::default();
        let experts = ConversationExperts {
            model: &model,
            policy: &policy,
            context: &context,
            local_context: &local_context,
            task_views: &[],
            cards: test_expert_cards(),
            builtin_setup: None,
        };
        let task_id = uuid::Uuid::new_v4();
        let task = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_agent::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                person_id: PersonId::new(),
                session_id: uuid::Uuid::new_v4(),
                parent_turn_id: uuid::Uuid::new_v4(),
                agent_id: COMMITMENTS_AGENT_ID.into(),
                message: floe_agent::A2AMessage {
                    message_id: uuid::Uuid::new_v4(),
                    context_id: uuid::Uuid::new_v4(),
                    task_id: Some(task_id),
                    role: A2AMessageRole::User,
                    parts: vec![A2APart::Text {
                        text: "Check my latest mail for commitments.".into(),
                    }],
                },
                max_output_bytes: 16_384,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                cancellation: floe_agent::Cancellation::default(),
            })
            .await
            .unwrap();
        assert_eq!(task.id, task_id);
        assert_eq!(task.state, A2ATaskState::Completed);
        let data = task.data_part(EXPERT_RESULT_MEDIA_TYPE).unwrap();
        let result: CommitmentsExpertResult = serde_json::from_str(data).unwrap();
        assert_eq!(result.findings.len(), 1);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn commitments_artifact_preserves_mail_calendar_task_and_memory_provenance() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let person_id = PersonId::new();
        let task_id = uuid::Uuid::new_v4();
        let memory_id = uuid::Uuid::new_v4();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (mail_request, socket) = request(socket).await;
            assert!(mail_request.starts_with("POST /v1/views/mail.communication "));
            respond(
                socket,
                serde_json::json!({
                    "schema_version": 1,
                    "view": {
                        "schema_version": 1,
                        "view_id": "mail.communication",
                        "source_handle": "mail:selected",
                        "observed_at_unix_ms": now - 1,
                        "expires_at_unix_ms": now + 299_999,
                        "coverage_complete": true,
                        "items": [{
                            "evidence_handle": "mail:request",
                            "thread_handle": "mail:thread",
                            "received_unix_ms": now - 2,
                            "from": "alex@example.com",
                            "to": "person@example.com",
                            "subject": "Delivery review",
                            "snippet": "Please confirm the review.",
                            "labels": ["INBOX"]
                        }]
                    }
                })
                .to_string(),
            )
            .await;

            let (socket, _) = listener.accept().await.unwrap();
            let (calendar_request, socket) = request(socket).await;
            assert_calendar_request_contract(&calendar_request);
            respond(
                socket,
                serde_json::json!({
                    "schema_version": 1,
                    "view": {
                        "schema_version": 1,
                        "view_id": "calendar.timeline",
                        "source_handle": "calendar:selected",
                        "observed_at_unix_ms": now - 1,
                        "expires_at_unix_ms": now + 240_000,
                        "range_start_unix_ms": now - 86_400_000,
                        "range_end_unix_ms": now + 86_400_000,
                        "coverage_complete": true,
                        "items": [{
                            "evidence_handle": "calendar:review",
                            "untrusted_title": "Delivery review",
                            "starts_at_unix_ms": now + 10_000,
                            "ends_at_unix_ms": now + 20_000,
                            "all_day": false
                        }]
                    }
                })
                .to_string(),
            )
            .await;

            let (socket, _) = listener.accept().await.unwrap();
            let (model_request, socket) = request(socket).await;
            assert!(model_request.starts_with("POST /v1/agent "));
            assert!(model_request.contains("mail:request"));
            assert!(model_request.contains("calendar:review"));
            assert!(model_request.contains(&task_id.to_string()));
            assert!(model_request.contains(&memory_id.to_string()));
            let answer = serde_json::json!({
                "summary": "Four bounded sources support the delivery commitment.",
                "findings": [
                    {"evidence_handle":"mail:request","kind":"request_to_user","statement":"A reply was requested.","epistemic_status":"observed","confidence_millis":1000},
                    {"evidence_handle":"calendar:review","kind":"user_commitment","statement":"A review is scheduled.","epistemic_status":"observed","confidence_millis":1000},
                    {"evidence_handle":task_id.to_string(),"kind":"user_commitment","statement":"A task remains open.","epistemic_status":"observed","confidence_millis":1000},
                    {"evidence_handle":memory_id.to_string(),"kind":"user_commitment","statement":"The delivery was confirmed.","epistemic_status":"observed","confidence_millis":1000}
                ]
            });
            let output = serde_json::json!({
                "output": [{"kind": "answer", "text": answer.to_string()}],
                "used_tokens": 64,
                "call_ids": []
            })
            .to_string();
            respond(
                socket,
                serde_json::json!({
                    "schema_version": 1,
                    "purpose": "everyday_assistance",
                    "output": output,
                    "trace_id": "0123456789abcdef0123456789abcdef",
                    "routing": {
                        "placement": "server_local",
                        "external_transfer": false,
                        "replay_source": "a".repeat(64)
                    }
                })
                .to_string(),
            )
            .await;
        });
        let route = AgentRemoteRouteDto {
            base_url: format!("http://{address}"),
            bearer_token: "daily_route_token_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
            calendar_connections: test_calendar_connections(),
        };
        let model = Model::new(Some(route.clone())).unwrap();
        let policy = policy(&model, Some(&route));
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![floe_agent::ContextMemory {
                target_id: memory_id,
                revision: 2,
                kind: floe_agent::PersonalMemoryKind::Commitment,
                statement: "The user confirmed the delivery.".into(),
                epistemic_status: floe_agent::EpistemicStatus::Fact,
                confidence_millis: 1000,
                observed_at_unix_ms: now - 5_000,
                valid_from_unix_ms: None,
                valid_until_unix_ms: Some(now + 180_000),
                source_refs: vec![floe_agent::LearningEvidenceRef {
                    session_id: uuid::Uuid::new_v4(),
                    turn_id: uuid::Uuid::new_v4(),
                }],
            }],
            evidence: vec![],
        };
        let local_context = LocalContextStore::default();
        let task_handle = uuid::Uuid::new_v5(&person_id.0, b"floe.tasks");
        let tasks = [NativeContextView {
            schema_version: AGENT_VERSION,
            handle: task_handle,
            person_id,
            view_id: "floe.tasks".into(),
            data_class: DataClass::Personal,
            source_handle: format!("floe.tasks:{task_handle}"),
            observed_at_unix_ms: now as u64,
            expires_at_unix_ms: (now + 220_000) as u64,
            coverage_complete: true,
            next_cursor: None,
            items: vec![floe_agent::NativeContextItem::Task {
                evidence_handle: task_id,
                untrusted_title: "Prepare delivery".into(),
                deadline_unix_ms: Some((now + 30_000) as u64),
                priority: floe_agent::TaskContextPriority::High,
            }],
        }];
        let experts = ConversationExperts {
            model: &model,
            policy: &policy,
            context: &context,
            local_context: &local_context,
            task_views: &tasks,
            cards: test_expert_cards(),
            builtin_setup: None,
        };
        let task = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_agent::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                person_id,
                session_id: uuid::Uuid::new_v4(),
                parent_turn_id: uuid::Uuid::new_v4(),
                agent_id: COMMITMENTS_AGENT_ID.into(),
                message: floe_agent::A2AMessage {
                    message_id: uuid::Uuid::new_v4(),
                    context_id: uuid::Uuid::new_v4(),
                    task_id: Some(uuid::Uuid::new_v4()),
                    role: A2AMessageRole::User,
                    parts: vec![A2APart::Text {
                        text: "Assess all commitment sources.".into(),
                    }],
                },
                max_output_bytes: 16_384,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                cancellation: floe_agent::Cancellation::default(),
            })
            .await
            .unwrap();
        let result: CommitmentsExpertResult =
            serde_json::from_str(task.data_part(EXPERT_RESULT_MEDIA_TYPE).unwrap()).unwrap();
        assert_eq!(result.findings.len(), 4);
        assert_eq!(result.expires_at_unix_ms, now + 180_000);
        assert!(result.source_handles.contains(&"mail:selected".into()));
        assert!(result.source_handles.contains(&"calendar:selected".into()));
        assert!(
            result
                .source_handles
                .contains(&format!("floe.tasks:{task_handle}"))
        );
        assert!(
            result
                .source_handles
                .contains(&format!("memory:{memory_id}:2"))
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn portfolio_delegations_read_fresh_views_and_return_typed_artifacts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let cases = [
            (
                "/v1/views/work.context",
                "Work Context Expert",
                serde_json::json!({
                    "schema_version": 1,
                    "view_id": "work.context",
                    "source_handle": "github:fresh",
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 299_999,
                    "coverage_complete": true,
                    "scope_handle": "workspace:selected",
                    "items": [{
                        "evidence_handle": "github:issue",
                        "kind": "project",
                        "title": "Release readiness",
                        "status": "open",
                        "blocker": "Missing validation",
                        "observed_at_unix_ms": now - 2
                    }]
                }),
                serde_json::json!({
                    "summary": "The release is blocked on validation.",
                    "insights": [{
                        "evidence_handle": "github:issue",
                        "blocker": "Missing validation",
                        "next_action": "Attach validation evidence.",
                        "confidence_millis": 1000
                    }]
                }),
            ),
            (
                "/v1/views/life.logistics",
                "Life Logistics Expert",
                serde_json::json!({
                    "schema_version": 1,
                    "view_id": "life.logistics",
                    "source_handle": "home:fresh",
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 299_999,
                    "coverage_complete": true,
                    "items": [{
                        "evidence_handle": "home:sensor",
                        "kind": "home_state",
                        "summary": "Window sensor",
                        "status": "open",
                        "needs_attention": true
                    }]
                }),
                serde_json::json!({
                    "summary": "The selected window needs attention.",
                    "preparations": [{
                        "evidence_handle": "home:sensor",
                        "recommendation": "Check the window before leaving.",
                        "urgency": "soon",
                        "requires_approval": false
                    }]
                }),
            ),
        ];
        let server = tokio::spawn(async move {
            for (path, role, view, answer) in cases {
                let (socket, _) = listener.accept().await.unwrap();
                let (view_request, socket) = request(socket).await;
                assert!(view_request.starts_with(&format!("POST {path} ")));
                respond(
                    socket,
                    serde_json::json!({"schema_version": 1, "view": view}).to_string(),
                )
                .await;

                let (socket, _) = listener.accept().await.unwrap();
                let (model_request, socket) = request(socket).await;
                assert!(model_request.starts_with("POST /v1/agent "));
                assert!(model_request.contains(role));
                let output = serde_json::json!({
                    "output": [{"kind": "answer", "text": answer.to_string()}],
                    "used_tokens": 64,
                    "call_ids": []
                })
                .to_string();
                respond(
                    socket,
                    serde_json::json!({
                        "schema_version": 1,
                        "purpose": "everyday_assistance",
                        "output": output,
                        "trace_id": "0123456789abcdef0123456789abcdef",
                        "routing": {
                            "placement": "server_local",
                            "external_transfer": false,
                            "replay_source": "a".repeat(64)
                        }
                    })
                    .to_string(),
                )
                .await;
            }
        });
        let route = AgentRemoteRouteDto {
            base_url: format!("http://{address}"),
            bearer_token: "daily_route_token_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
            calendar_connections: vec![],
        };
        let model = Model::new(Some(route.clone())).unwrap();
        let policy = policy(&model, Some(&route));
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            evidence: vec![],
        };
        let local_context = LocalContextStore::default();
        let experts = ConversationExperts {
            model: &model,
            policy: &policy,
            context: &context,
            local_context: &local_context,
            task_views: &[],
            cards: test_expert_cards(),
            builtin_setup: None,
        };
        let mut results = vec![];
        for agent_id in [WORK_CONTEXT_AGENT_ID, LIFE_LOGISTICS_AGENT_ID] {
            let task = experts
                .handle_message(A2ASendMessageRequest {
                    usage: floe_agent::UsageLedger::default(),
                    schema_version: AGENT_VERSION,
                    person_id: PersonId::new(),
                    session_id: uuid::Uuid::new_v4(),
                    parent_turn_id: uuid::Uuid::new_v4(),
                    agent_id: agent_id.into(),
                    message: floe_agent::A2AMessage {
                        message_id: uuid::Uuid::new_v4(),
                        context_id: uuid::Uuid::new_v4(),
                        task_id: Some(uuid::Uuid::new_v4()),
                        role: A2AMessageRole::User,
                        parts: vec![A2APart::Text {
                            text: "Review the selected context.".into(),
                        }],
                    },
                    max_output_bytes: 16_384,
                    deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                    cancellation: floe_agent::Cancellation::default(),
                })
                .await
                .unwrap();
            results.push(task.data_part(EXPERT_RESULT_MEDIA_TYPE).unwrap().to_owned());
        }
        let work: WorkContextExpertResult = serde_json::from_str(&results[0]).unwrap();
        let logistics: LifeLogisticsExpertResult = serde_json::from_str(&results[1]).unwrap();
        assert_eq!(work.scope_handle, "workspace:selected");
        assert_eq!(work.insights[0].evidence_handle, "github:issue");
        assert_eq!(logistics.source_handle, "home:fresh");
        assert_eq!(logistics.preparations[0].evidence_handle, "home:sensor");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn personal_delegations_use_bounded_views_and_capability_free_experts() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let cases = [
            (
                RELATIONSHIPS_AGENT_ID,
                "/v1/views/people.identity",
                "Relationships Expert",
                serde_json::json!({
                    "schema_version": 1,
                    "view_id": "people.identity",
                    "source_handle": "contacts:local",
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 299_999,
                    "coverage_complete": true,
                    "identities": [{
                        "identity_handle": "person:alex",
                        "display_name": "Alex",
                        "aliases": ["alex@example.com"],
                        "confidence_millis": 1000,
                        "evidence_handles": ["contact:alex"]
                    }]
                }),
                serde_json::json!({
                    "summary": "No confirmed interaction supports a follow-up.",
                    "follow_ups": []
                }),
                "contacts:local",
            ),
            (
                FOCUS_AGENT_ID,
                "/v1/views/attention.coarse",
                "Focus & Attention Expert",
                serde_json::json!({
                    "schema_version": 1,
                    "view_id": "attention.coarse",
                    "source_handle": "attention:mac-local",
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 119_999,
                    "state": "focused",
                    "confidence_millis": 800,
                    "evidence_handles": ["attention:aggregate"]
                }),
                serde_json::json!({
                    "summary": "Protect the current focus period before the review.",
                    "recommendation": "protect_focus",
                    "rationale": "Attention, the upcoming review, and active release work support focus protection.",
                    "evidence_handles": ["attention:aggregate", "calendar:review", "work:release"]
                }),
                "attention:mac-local",
            ),
            (
                WELLBEING_AGENT_ID,
                "/v1/views/wellbeing.derived",
                "Wellbeing Expert",
                serde_json::json!({
                    "schema_version": 1,
                    "view_id": "wellbeing.derived",
                    "source_handle": "health:derived-local",
                    "observed_at_unix_ms": now - 1,
                    "expires_at_unix_ms": now + 299_999,
                    "capacity": "reduced",
                    "recovery": "needs_recovery",
                    "confidence_millis": 750,
                    "evidence_handles": ["health:aggregate"]
                }),
                serde_json::json!({
                    "summary": "Reduce optional load and preserve recovery time.",
                    "schedule_impact": "protect_recovery",
                    "rationale": "Derived capacity is reduced and recovery is needed.",
                    "evidence_handles": ["health:aggregate"]
                }),
                "health:derived-local",
            ),
        ];
        let server_cases = cases.clone();
        let server = tokio::spawn(async move {
            for (agent_id, path, role, view, answer, _) in server_cases {
                let (socket, _) = listener.accept().await.unwrap();
                let (view_request, socket) = request(socket).await;
                assert!(view_request.starts_with(&format!("POST {path} ")));
                assert!(view_request.contains(r#"{"schema_version":1}"#));
                respond(
                    socket,
                    serde_json::json!({"schema_version": 1, "view": view}).to_string(),
                )
                .await;

                let optional_paths: &[&str] = match agent_id {
                    RELATIONSHIPS_AGENT_ID => &["/v1/views/relationships.confirmed_interactions"],
                    FOCUS_AGENT_ID => &["/v1/views/calendar.timeline", "/v1/views/work.context"],
                    WELLBEING_AGENT_ID => &["/v1/views/calendar.timeline"],
                    _ => unreachable!(),
                };
                for path in optional_paths {
                    let (socket, _) = listener.accept().await.unwrap();
                    let (optional_request, socket) = request(socket).await;
                    assert!(optional_request.starts_with(&format!("POST {path} ")));
                    if *path == "/v1/views/calendar.timeline" {
                        assert_calendar_request_contract(&optional_request);
                    }
                    match (agent_id, *path) {
                        (FOCUS_AGENT_ID, "/v1/views/calendar.timeline") => {
                            respond(
                                socket,
                                serde_json::json!({
                                    "schema_version": 1,
                                    "view": {
                                        "schema_version": 1,
                                        "view_id": "calendar.timeline",
                                        "source_handle": "calendar:selected",
                                        "observed_at_unix_ms": now - 1,
                                        "expires_at_unix_ms": now + 240_000,
                                        "range_start_unix_ms": now - 86_400_000,
                                        "range_end_unix_ms": now + 86_400_000,
                                        "coverage_complete": true,
                                        "items": [{
                                            "evidence_handle": "calendar:review",
                                            "untrusted_title": "Release review",
                                            "starts_at_unix_ms": now + 10_000,
                                            "ends_at_unix_ms": now + 20_000,
                                            "all_day": false
                                        }]
                                    }
                                })
                                .to_string(),
                            )
                            .await;
                        }
                        (FOCUS_AGENT_ID, "/v1/views/work.context") => {
                            respond(
                                socket,
                                serde_json::json!({
                                    "schema_version": 1,
                                    "view": {
                                        "schema_version": 1,
                                        "view_id": "work.context",
                                        "source_handle": "work:selected",
                                        "observed_at_unix_ms": now - 1,
                                        "expires_at_unix_ms": now + 220_000,
                                        "coverage_complete": true,
                                        "scope_handle": "workspace:selected",
                                        "items": [{
                                            "evidence_handle": "work:release",
                                            "kind": "project",
                                            "title": "Release readiness",
                                            "status": "active",
                                            "blocker": null,
                                            "observed_at_unix_ms": now - 2
                                        }]
                                    }
                                })
                                .to_string(),
                            )
                            .await;
                        }
                        _ => respond_not_found(socket).await,
                    }
                }

                let (socket, _) = listener.accept().await.unwrap();
                let (model_request, socket) = request(socket).await;
                assert!(model_request.starts_with("POST /v1/agent "));
                assert!(model_request.contains(role));
                assert!(model_request.contains(r#""tools":[]"#));
                assert!(!model_request.contains("notification.send"));
                let output = serde_json::json!({
                    "output": [{"kind": "answer", "text": answer.to_string()}],
                    "used_tokens": 64,
                    "call_ids": []
                })
                .to_string();
                respond(
                    socket,
                    serde_json::json!({
                        "schema_version": 1,
                        "purpose": "everyday_assistance",
                        "output": output,
                        "trace_id": "0123456789abcdef0123456789abcdef",
                        "routing": {
                            "placement": "server_local",
                            "external_transfer": false,
                            "replay_source": "a".repeat(64)
                        }
                    })
                    .to_string(),
                )
                .await;
            }
        });
        let route = AgentRemoteRouteDto {
            base_url: format!("http://{address}"),
            bearer_token: "daily_route_token_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
            calendar_connections: test_calendar_connections(),
        };
        let model = Model::new(Some(route.clone())).unwrap();
        let policy = policy(&model, Some(&route));
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            evidence: vec![],
        };
        let local_context = LocalContextStore::default();
        let experts = ConversationExperts {
            model: &model,
            policy: &policy,
            context: &context,
            local_context: &local_context,
            task_views: &[],
            cards: test_expert_cards(),
            builtin_setup: None,
        };
        for (agent_id, _, _, _, _, source_handle) in cases {
            let task = experts
                .handle_message(A2ASendMessageRequest {
                    usage: floe_agent::UsageLedger::default(),
                    schema_version: AGENT_VERSION,
                    person_id: PersonId::new(),
                    session_id: uuid::Uuid::new_v4(),
                    parent_turn_id: uuid::Uuid::new_v4(),
                    agent_id: agent_id.into(),
                    message: floe_agent::A2AMessage {
                        message_id: uuid::Uuid::new_v4(),
                        context_id: uuid::Uuid::new_v4(),
                        task_id: Some(uuid::Uuid::new_v4()),
                        role: A2AMessageRole::User,
                        parts: vec![A2APart::Text {
                            text: "Assess only the supplied personal context.".into(),
                        }],
                    },
                    max_output_bytes: 16_384,
                    deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                    cancellation: floe_agent::Cancellation::default(),
                })
                .await
                .unwrap();
            let data: serde_json::Value =
                serde_json::from_str(task.data_part(EXPERT_RESULT_MEDIA_TYPE).unwrap()).unwrap();
            assert_eq!(data["source_handle"], source_handle);
            if agent_id == RELATIONSHIPS_AGENT_ID {
                assert_eq!(
                    data["source_handles"],
                    serde_json::json!(["contacts:local"])
                );
                assert_eq!(data["follow_ups"], serde_json::json!([]));
            }
            if agent_id == FOCUS_AGENT_ID {
                assert_eq!(
                    data["source_handles"],
                    serde_json::json!([
                        "attention:mac-local",
                        "calendar:selected",
                        "work:selected"
                    ])
                );
                assert_eq!(
                    data["evidence_handles"],
                    serde_json::json!(["attention:aggregate", "calendar:review", "work:release"])
                );
            }
        }
        server.await.unwrap();
    }

    #[tokio::test]
    async fn unavailable_personal_provider_is_typed_and_never_runs_the_expert() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (view_request, mut socket) = request(socket).await;
            assert!(view_request.starts_with("POST /v1/views/attention.coarse "));
            socket
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
        });
        let route = AgentRemoteRouteDto {
            base_url: format!("http://{address}"),
            bearer_token: "daily_route_token_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
            calendar_connections: test_calendar_connections(),
        };
        let model = Model::new(Some(route.clone())).unwrap();
        let policy = policy(&model, Some(&route));
        let context = AgentContext {
            projection_version: 1,
            persona: None,
            memories: vec![],
            evidence: vec![],
        };
        let local_context = LocalContextStore::default();
        let experts = ConversationExperts {
            model: &model,
            policy: &policy,
            context: &context,
            local_context: &local_context,
            task_views: &[],
            cards: test_expert_cards(),
            builtin_setup: None,
        };
        let result = experts
            .handle_message(A2ASendMessageRequest {
                usage: floe_agent::UsageLedger::default(),
                schema_version: AGENT_VERSION,
                person_id: PersonId::new(),
                session_id: uuid::Uuid::new_v4(),
                parent_turn_id: uuid::Uuid::new_v4(),
                agent_id: FOCUS_AGENT_ID.into(),
                message: floe_agent::A2AMessage {
                    message_id: uuid::Uuid::new_v4(),
                    context_id: uuid::Uuid::new_v4(),
                    task_id: Some(uuid::Uuid::new_v4()),
                    role: A2AMessageRole::User,
                    parts: vec![A2APart::Text {
                        text: "Assess my current attention.".into(),
                    }],
                },
                max_output_bytes: 16_384,
                deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(5),
                cancellation: floe_agent::Cancellation::default(),
            })
            .await;
        assert_eq!(result, Err(AgentFailure::CapabilityUnavailable));
        server.await.unwrap();
    }
}
