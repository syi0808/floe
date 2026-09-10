use floe_agent::{
    AGENT_VERSION, AgentBudget, AgentCommand, AgentContext, AgentEvent, AgentFailure, AgentRuntime,
    CapabilityDescriptor, CapabilityHost, CapabilityInvocation, DataClass, InferencePolicyDecision,
    ModelPlacement, ModelRequest, ModelResponse, ModelRunner, SessionStore, TransferConsent,
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
    let runtime = AgentRuntime {
        store: vault,
        model: &model,
        capabilities: &capabilities,
        policy: &policy,
        budget: AgentBudget::default(),
    };
    if request.continuation {
        runtime
            .continue_turn(
                person_id,
                session_id,
                request.expected_revision,
                context,
                cancellation,
                emit,
            )
            .await
    } else {
        runtime
            .run_turn(
                AgentCommand {
                    schema_version: AGENT_VERSION,
                    person_id,
                    session_id,
                    expected_revision: request.expected_revision,
                    text: text.into(),
                },
                context,
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
        vec![CapabilityDescriptor {
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
        }]
    }

    async fn invoke(&self, invocation: CapabilityInvocation) -> Result<String, AgentFailure> {
        if invocation.capability_id != "mail.communication.read" {
            return Err(AgentFailure::CapabilityDenied);
        }
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
        let input: Input =
            serde_json::from_str(&invocation.input).map_err(|_| AgentFailure::InvalidInput)?;
        let Model::Server(model) = self.model else {
            return Err(AgentFailure::CapabilityUnavailable);
        };
        let view = model
            .read_communication_view(
                &input.query,
                input.cursor,
                input.limit,
                invocation.deadline,
                &invocation.cancellation,
            )
            .await?;
        serde_json::to_string(&view).map_err(|_| AgentFailure::InvalidInput)
    }
}

fn default_communication_limit() -> usize {
    25
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn server_route_exposes_only_bounded_mail_observe_capability() {
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
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].id, "mail.communication.read");
        assert!(descriptors[0].read_only);
        assert_eq!(descriptors[0].output_data_class, DataClass::Personal);
        let schema = descriptors[0].input_schema.as_ref().unwrap();
        assert_eq!(schema["additionalProperties"], false);
        assert!(schema.to_string().len() < 1024);
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
}
