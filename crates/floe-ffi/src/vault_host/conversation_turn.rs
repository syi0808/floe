use floe_agent::{
    A2A_PROTOCOL_VERSION, A2AArtifact, A2AMessageRole, A2APart, A2ASendMessageRequest, A2ATask,
    A2ATaskState, AGENT_VERSION, AgentBudget, AgentCard, AgentCommand, AgentContext, AgentEvent,
    AgentFailure, AgentRuntime, CapabilityDescriptor, CapabilityHost, CapabilityInvocation,
    CommitmentsExpertResult, CommunicationExpertResult, DataClass, EXPERT_RESULT_MEDIA_TYPE,
    InProcessA2ATransport, InProcessAgent, InferencePolicyDecision, LifeLogisticsExpertResult,
    MailExpertInvocation, ModelPlacement, ModelRequest, ModelResponse, ModelRunner,
    PortfolioExpertInvocation, SessionStore, TransferConsent, WorkContextExpertResult,
    run_commitments_expert, run_communication_expert, run_life_logistics_expert,
    run_work_context_expert,
};
use floe_core::{EncryptedAgentVault, FloeCore, VaultKeyProvider};
use floe_domain::PersonId;
use floe_protocol::{AgentConversationTurnRequestDto, AgentRemoteRouteDto};

use crate::{local_model::FoundationModelRunner, remote_model::ServerModelRunner};

use super::session_uuid;

pub(super) async fn run<Keys: VaultKeyProvider>(
    core: &FloeCore,
    vault: &EncryptedAgentVault<Keys>,
    person_id: PersonId,
    request: &AgentConversationTurnRequestDto,
    cancellation: floe_agent::Cancellation,
    mut emit: impl FnMut(AgentEvent) + Send,
) -> Result<floe_agent::AgentSession, AgentFailure> {
    let text = request.text.trim();
    if text.is_empty() || text.len() > 8_192 {
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
    if let Some(session) = super::schedule_conversation::try_run_with_schedule_expert(
        core,
        vault,
        person_id,
        request,
        context.clone(),
        cancellation.clone(),
        &mut emit,
    )
    .await?
    {
        return Ok(session);
    }
    let model = Model::new(request.remote_route.clone())?;
    let policy = policy(&model, request.remote_route.as_ref());
    let capabilities = ConversationCapabilities { model: &model };
    let experts = ConversationExperts {
        model: &model,
        policy: &policy,
        context: &context,
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
                    text: text.into(),
                },
                context.clone(),
                &agents,
                cancellation,
                emit,
            )
            .await
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
}

impl CapabilityHost for ConversationCapabilities<'_> {
    fn descriptors(&self, _: PersonId) -> Vec<CapabilityDescriptor> {
        if !matches!(self.model, Model::Server(_)) {
            return vec![];
        }
        vec![
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
        ]
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
        let Model::Server(model) = self.model else {
            return Err(AgentFailure::CapabilityUnavailable);
        };
        let output = match invocation.capability_id.as_str() {
            "mail.communication.read" => {
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
            "work.context.read" | "life.logistics.read" => {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Empty {}
                serde_json::from_str::<Empty>(&invocation.input)
                    .map_err(|_| AgentFailure::InvalidInput)?;
                if invocation.capability_id == "work.context.read" {
                    serde_json::to_value(
                        model
                            .read_work_context_view(invocation.deadline, &invocation.cancellation)
                            .await?,
                    )
                } else {
                    serde_json::to_value(
                        model
                            .read_logistics_view(invocation.deadline, &invocation.cancellation)
                            .await?,
                    )
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

const COMMITMENTS_AGENT_ID: &str = "floe.commitments";
const COMMUNICATION_AGENT_ID: &str = "floe.communication";
const WORK_CONTEXT_AGENT_ID: &str = "floe.work-context";
const LIFE_LOGISTICS_AGENT_ID: &str = "floe.life-logistics";

struct ConversationExperts<'model> {
    model: &'model Model,
    policy: &'model InferencePolicyDecision,
    context: &'model AgentContext,
}

impl InProcessAgent for ConversationExperts<'_> {
    fn agent_cards(&self, _: PersonId) -> Vec<AgentCard> {
        if !matches!(self.model, Model::Server(_)) {
            return vec![];
        }
        vec![
            AgentCard {
                schema_version: AGENT_VERSION,
                protocol_version: A2A_PROTOCOL_VERSION.into(),
                id: COMMITMENTS_AGENT_ID.into(),
                version: "1.0.0".into(),
                name: "Commitments Expert".into(),
                description: "Finds obligations, deadlines, expected replies and follow-up gaps from bounded evidence.".into(),
                domain_tags: vec!["commitments".into()],
                skills: vec!["Distinguish explicit commitments from inferred follow-up candidates.".into()],
            },
            AgentCard {
                schema_version: AGENT_VERSION,
                protocol_version: A2A_PROTOCOL_VERSION.into(),
                id: COMMUNICATION_AGENT_ID.into(),
                version: "1.0.0".into(),
                name: "Communication Expert".into(),
                description: "Judges reply need, summary, tone and optional email drafts from bounded evidence.".into(),
                domain_tags: vec!["communication".into()],
                skills: vec!["Prepare evidence-linked reply guidance without sending messages.".into()],
            },
            AgentCard {
                schema_version: AGENT_VERSION,
                protocol_version: A2A_PROTOCOL_VERSION.into(),
                id: WORK_CONTEXT_AGENT_ID.into(),
                version: "1.0.0".into(),
                name: "Work Context Expert".into(),
                description: "Finds blockers and next actions from a bounded selected workspace.".into(),
                domain_tags: vec!["work-context".into()],
                skills: vec!["Prepare evidence-linked work guidance without changing project state.".into()],
            },
            AgentCard {
                schema_version: AGENT_VERSION,
                protocol_version: A2A_PROTOCOL_VERSION.into(),
                id: LIFE_LOGISTICS_AGENT_ID.into(),
                version: "1.0.0".into(),
                name: "Life Logistics Expert".into(),
                description: "Finds preparations from bounded home and logistics state.".into(),
                domain_tags: vec!["life-logistics".into()],
                skills: vec!["Prepare evidence-linked logistics guidance without taking actions.".into()],
            },
        ]
    }

    async fn handle_message(
        &self,
        request: A2ASendMessageRequest,
    ) -> Result<A2ATask, AgentFailure> {
        if request.schema_version != AGENT_VERSION
            || request.message.role != A2AMessageRole::User
            || request.message.task_id.is_none()
            || !matches!(
                request.agent_id.as_str(),
                COMMITMENTS_AGENT_ID
                    | COMMUNICATION_AGENT_ID
                    | WORK_CONTEXT_AGENT_ID
                    | LIFE_LOGISTICS_AGENT_ID
            )
        {
            return Err(AgentFailure::CapabilityDenied);
        }
        let Model::Server(model) = self.model else {
            return Err(AgentFailure::CapabilityUnavailable);
        };
        let assignment = request.message.text()?.to_owned();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map_err(|_| AgentFailure::StaleContext)?;
        let current_time_unix_ms =
            i64::try_from(now.as_millis()).map_err(|_| AgentFailure::StaleContext)?;
        let invocation_id = request.message.task_id.ok_or(AgentFailure::InvalidInput)?;
        let mail_invocation = |view| MailExpertInvocation {
            usage: request.usage.clone(),
            person_id: request.person_id,
            invocation_id,
            assignment: assignment.clone(),
            current_time_unix_ms,
            context: self.context.clone(),
            view,
            max_output_bytes: request.max_output_bytes,
            max_model_tokens: 40_960,
            max_model_cost_micros: 50_000,
            deadline: request.deadline,
            cancellation: request.cancellation.clone(),
        };
        let portfolio_invocation = || PortfolioExpertInvocation {
            usage: request.usage.clone(),
            person_id: request.person_id,
            invocation_id,
            assignment: assignment.clone(),
            current_time_unix_ms,
            context: self.context.clone(),
            max_output_bytes: request.max_output_bytes,
            max_model_tokens: 40_960,
            max_model_cost_micros: 50_000,
            deadline: request.deadline,
            cancellation: request.cancellation.clone(),
        };
        let (summary, data, name) = match request.agent_id.as_str() {
            COMMITMENTS_AGENT_ID => {
                let view = model
                    .read_communication_view(
                        "",
                        0,
                        default_communication_limit(),
                        request.deadline,
                        &request.cancellation,
                    )
                    .await?;
                let result: CommitmentsExpertResult =
                    run_commitments_expert(model, self.policy, mail_invocation(view)).await?;
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                    "Commitments expert result",
                )
            }
            COMMUNICATION_AGENT_ID => {
                let view = model
                    .read_communication_view(
                        "",
                        0,
                        default_communication_limit(),
                        request.deadline,
                        &request.cancellation,
                    )
                    .await?;
                let result: CommunicationExpertResult =
                    run_communication_expert(model, self.policy, mail_invocation(view)).await?;
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                    "Communication expert result",
                )
            }
            WORK_CONTEXT_AGENT_ID => {
                let view = model
                    .read_work_context_view(request.deadline, &request.cancellation)
                    .await?;
                let result: WorkContextExpertResult =
                    run_work_context_expert(model, self.policy, portfolio_invocation(), view)
                        .await?;
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                    "Work Context expert result",
                )
            }
            LIFE_LOGISTICS_AGENT_ID => {
                let view = model
                    .read_logistics_view(request.deadline, &request.cancellation)
                    .await?;
                let result: LifeLogisticsExpertResult =
                    run_life_logistics_expert(model, self.policy, portfolio_invocation(), view)
                        .await?;
                (
                    result.summary.clone(),
                    serde_json::to_string(&result).map_err(|_| AgentFailure::InvalidModelOutput)?,
                    "Life Logistics expert result",
                )
            }
            _ => return Err(AgentFailure::CapabilityDenied),
        };
        Ok(A2ATask {
            id: request.message.task_id.ok_or(AgentFailure::InvalidInput)?,
            context_id: request.message.context_id,
            agent_id: request.agent_id,
            state: A2ATaskState::Completed,
            history: vec![request.message],
            artifacts: vec![A2AArtifact {
                artifact_id: uuid::Uuid::new_v4(),
                name: name.into(),
                parts: vec![
                    A2APart::Text { text: summary },
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

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

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

    #[test]
    fn configured_daily_route_takes_priority_over_the_device_model() {
        let model = Model::new(Some(AgentRemoteRouteDto {
            base_url: "http://127.0.0.1:8431".into(),
            bearer_token: "daily_route_token_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
        }))
        .unwrap();
        assert!(matches!(model, Model::Server(_)));
    }

    #[test]
    fn missing_daily_route_uses_the_device_model() {
        assert!(matches!(Model::new(None).unwrap(), Model::Foundation(_)));
    }

    #[test]
    fn server_route_exposes_only_bounded_context_observe_capabilities() {
        let model = Model::new(Some(AgentRemoteRouteDto {
            base_url: "http://127.0.0.1:8431".into(),
            bearer_token: "daily_route_token_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
        }))
        .unwrap();
        let capabilities = ConversationCapabilities { model: &model };
        let descriptors = capabilities.descriptors(PersonId::new());
        assert_eq!(descriptors.len(), 3);
        assert_eq!(
            descriptors
                .iter()
                .map(|descriptor| descriptor.id.as_str())
                .collect::<Vec<_>>(),
            [
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
        let policy = policy(&model, None);
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
        };
        let cards = experts.agent_cards(PersonId::new());
        assert_eq!(cards.len(), 4);
        assert_eq!(cards[0].id, COMMITMENTS_AGENT_ID);
        assert_eq!(cards[1].id, COMMUNICATION_AGENT_ID);
        assert_eq!(cards[2].id, WORK_CONTEXT_AGENT_ID);
        assert_eq!(cards[3].id, LIFE_LOGISTICS_AGENT_ID);
        assert!(cards.iter().all(|card| card.validate().is_ok()));
    }

    #[tokio::test]
    async fn mail_capability_rejects_authority_escalation_before_io() {
        let model = Model::new(Some(AgentRemoteRouteDto {
            base_url: "http://127.0.0.1:1".into(),
            bearer_token: "daily_route_token_that_is_long_enough".into(),
            purpose: "everyday_assistance".into(),
            external: false,
            allow_external: false,
        }))
        .unwrap();
        let capabilities = ConversationCapabilities { model: &model };
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
        };
        let model = Model::new(Some(route.clone())).unwrap();
        let policy = policy(&model, Some(&route));
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
        assert_eq!(result.source_handle, "mail:fresh");
        assert_eq!(result.findings.len(), 1);
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
        };
        let model = Model::new(Some(route.clone())).unwrap();
        let policy = policy(&model, Some(&route));
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
}
