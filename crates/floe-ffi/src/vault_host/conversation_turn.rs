use floe_agent::{
    AGENT_VERSION, AgentBudget, AgentCommand, AgentContext, AgentEvent, AgentFailure, AgentRuntime,
    CapabilityDescriptor, CapabilityHost, CapabilityInvocation, DataClass, InferencePolicyDecision,
    ModelPlacement, ModelRequest, ModelResponse, ModelRunner, SessionStore, TransferConsent,
};
use floe_core::{EncryptedAgentVault, VaultKeyProvider};
use floe_domain::PersonId;
use floe_protocol::{AgentConversationTurnRequestDto, AgentRemoteRouteDto};

use crate::{
    local_model::{FoundationModelRunner, LocalModelAvailability},
    remote_model::ServerModelRunner,
};

use super::session_uuid;

pub(super) async fn run<Keys: VaultKeyProvider>(
    vault: &EncryptedAgentVault<Keys>,
    person_id: PersonId,
    request: &AgentConversationTurnRequestDto,
    cancellation: floe_agent::Cancellation,
    emit: impl FnMut(AgentEvent) + Send,
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
    let model = Model::new(request.remote_route.clone())?;
    let policy = policy(&model, request.remote_route.as_ref());
    AgentRuntime {
        store: vault,
        model: &model,
        capabilities: &NoCapabilities,
        policy: &policy,
        budget: AgentBudget::default(),
    }
    .run_turn(
        AgentCommand {
            schema_version: AGENT_VERSION,
            person_id,
            session_id,
            expected_revision: request.expected_revision,
            text: text.into(),
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
        let local = FoundationModelRunner::encrypted();
        if matches!(local.availability(), Ok(LocalModelAvailability::Available)) {
            Ok(Self::Foundation(local))
        } else if let Some(route) = route {
            ServerModelRunner::new(route).map(Self::Server)
        } else {
            Ok(Self::Foundation(local))
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
