use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use floe_agent_contract::{
    AgentFailure, BoxFuture, ExecutionJournal, JournalAck, JournalEvent, ModelRequest,
    ModelResponse, PreparedModelCall,
};
use floe_execution::ExecutionScope;
use floe_kernel::{OwnerActor, PersonId};
use floe_provider_adapters::gateway::{CompositeModelProvider, GatewayCredentialStore};
use floe_vault::{EncryptedAgentVault, VaultKey, VaultKeyProvider};
use uuid::Uuid;

pub(super) struct SmokeResolver;
impl floe_access::DependencyResolver for SmokeResolver {
    fn authorize<'a>(
        &'a self,
        _: &'a floe_context_contract::ContextDependency,
        _: &'a floe_access::DependencyAuthorization,
    ) -> BoxFuture<'a, Result<(), AgentFailure>> {
        Box::pin(async { Err(AgentFailure::PolicyDenied) })
    }
}

pub(super) type DiagnosticModel =
    floe_inference::InferenceService<CompositeModelProvider, SmokeResolver, GatewayCredentialStore>;

pub(super) fn model(store: GatewayCredentialStore) -> DiagnosticModel {
    floe_inference::InferenceService::new(
        CompositeModelProvider::new(store.clone()),
        SmokeResolver,
        store,
    )
}

/// Each synthetic exercise creates a real isolated encrypted Person. Gateway
/// absence comes from that Vault's durable owner state; a foreign system
/// credential remains a failure. The disposable Vault key stays in memory.
pub(super) struct SyntheticProfile {
    pub vault: Arc<EncryptedAgentVault<SmokeKeys>>,
    pub actor: OwnerActor,
    _root: tempfile::TempDir,
}

impl SyntheticProfile {
    pub(super) async fn create() -> Result<Self, AgentFailure> {
        let root = tempfile::Builder::new().prefix("floe-synthetic-smoke-").tempdir()
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        let person = PersonId::new();
        let vault = Arc::new(EncryptedAgentVault::create(root.path(), person, SmokeKeys::default()).await?);
        let actor = OwnerActor { person_id: vault.person_id(),
            device_id: format!("synthetic-smoke-{}", Uuid::new_v4()), runtime_epoch: 1 };
        actor.validate()?;
        Ok(Self { vault, actor, _root: root })
    }

    pub(super) fn model(&self) -> DiagnosticModel {
        model(GatewayCredentialStore::new(self.vault.clone()))
    }
}

#[derive(Default)]
pub(super) struct SmokeKeys(Mutex<HashMap<(PersonId, Uuid), [u8; 32]>>);

impl VaultKeyProvider for SmokeKeys {
    fn load(&self, person: PersonId, vault: Uuid) -> Result<VaultKey, AgentFailure> {
        self.0.lock().map_err(|_| AgentFailure::VaultUnavailable)?
            .get(&(person, vault)).copied().map(VaultKey::from_bytes).ok_or(AgentFailure::VaultUnavailable)
    }

    fn insert(&self, person: PersonId, vault: Uuid, key: &VaultKey) -> Result<(), AgentFailure> {
        let mut keys = self.0.lock().map_err(|_| AgentFailure::VaultUnavailable)?;
        if keys.contains_key(&(person, vault)) { return Err(AgentFailure::VaultUnavailable); }
        keys.insert((person, vault), *key.as_bytes());
        Ok(())
    }
}

/// A durable diagnostic journal containing only synthetic fixture events.
pub(super) struct DiagnosticJournal {
    path: PathBuf,
    state: Mutex<(File, HashMap<String, (u64, String)>)>,
}
impl DiagnosticJournal {
    pub(super) fn new() -> Result<Self, AgentFailure> {
        let root = tempfile::Builder::new()
            .prefix("floe-synthetic-model-journal-")
            .tempdir()
            .map_err(|_| AgentFailure::StorageUnavailable)?
            .keep();
        let path = root.join("journal.jsonl");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| AgentFailure::StorageUnavailable)?;
        Ok(Self {
            path,
            state: Mutex::new((file, HashMap::new())),
        })
    }
    pub(super) fn path(&self) -> &std::path::Path {
        &self.path
    }
    fn record<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        Box::pin(async move {
            let encoded = serde_json::to_string(&event).map_err(|_| AgentFailure::InvalidInput)?;
            let key = match &event {
                JournalEvent::ModelIntent { attempt_id, .. } => format!("intent:{attempt_id}"),
                JournalEvent::ModelResult { attempt_id, .. } => format!("result:{attempt_id}"),
                _ => encoded.clone(),
            };
            let mut state = self
                .state
                .lock()
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            if let Some((revision, previous)) = state.1.get(&key) {
                return if previous == &encoded {
                    Ok(JournalAck::Accepted {
                        revision: *revision,
                    })
                } else {
                    Err(AgentFailure::Conflict)
                };
            }
            let revision =
                u64::try_from(state.1.len()).map_err(|_| AgentFailure::BudgetExceeded)? + 1;
            writeln!(state.0, "{encoded}").map_err(|_| AgentFailure::StorageUnavailable)?;
            state
                .0
                .sync_all()
                .map_err(|_| AgentFailure::StorageUnavailable)?;
            state.1.insert(key, (revision, encoded));
            Ok(JournalAck::Accepted { revision })
        })
    }
}
impl ExecutionJournal for DiagnosticJournal {
    fn record_intent<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        self.record(event)
    }
    fn record_result<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        self.record(event)
    }
    fn record_output<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        self.record(event)
    }
    fn checkpoint<'a>(
        &'a self,
        event: JournalEvent,
    ) -> BoxFuture<'a, Result<JournalAck, AgentFailure>> {
        self.record(event)
    }
}

pub(super) async fn invoke(
    prepared: &dyn PreparedModelCall,
    request: ModelRequest,
    scope: &ExecutionScope,
    journal: &dyn ExecutionJournal,
) -> Result<ModelResponse, AgentFailure> {
    request.validate()?;
    let attempt_id = request.attempt_id;
    let intent = scope
        .run(journal.record_intent(JournalEvent::ModelIntent {
            reservation_ceiling: request.reservation_ceiling,
            parent_task_id: None,
            attempt_id,
            projection_ref: request.projection.projection_ref,
            plan: prepared.plan().clone(),
        }))
        .await?;
    if !matches!(intent, JournalAck::Accepted { .. }) {
        return Err(AgentFailure::Conflict);
    }
    let response = scope.run(prepared.generate(request, scope)).await;
    let receipt = scope.budget().model_attempt_receipt(attempt_id);
    if receipt.is_none() && (response.is_ok() || scope.budget().model_attempt_admitted(attempt_id))
    {
        return Err(AgentFailure::StorageUnavailable);
    }
    let (usage, accounting) = receipt.map_or_else(
        || {
            (
                floe_agent_contract::ModelUsage::default(),
                floe_agent_contract::ModelAccounting::default(),
            )
        },
        |receipt| {
            (
                floe_agent_contract::ModelUsage {
                    tokens: receipt.charged_tokens,
                    cost_micros: receipt.charged_cost_micros,
                },
                receipt.accounting,
            )
        },
    );
    let acknowledgment = journal
        .record_result(JournalEvent::ModelResult {
            attempt_id,
            usage,
            accounting,
        })
        .await?;
    if !matches!(acknowledgment, JournalAck::Accepted { .. }) {
        return Err(AgentFailure::Conflict);
    }
    if receipt.is_some() {
        scope.budget().acknowledge_model_attempt(attempt_id)?;
    }
    let response = response?;
    if response.attempt_id != attempt_id
        || response.usage != usage
        || response.accounting != accounting
    {
        return Err(AgentFailure::InvalidModelOutput);
    }
    Ok(response)
}
