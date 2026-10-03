use std::{
    collections::HashMap,
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use floe_agent_contract::{
    AgentFailure, BoxFuture, ExecutionJournal, JournalAck, JournalEvent, ModelRequest,
    ModelResponse, PreparedModelCall,
};
use floe_execution::ExecutionScope;
use floe_provider_adapters::gateway::{CompositeModelProvider, GatewayCredentialStore};

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

/// A synthetic fresh identity has no enrolled producer. A committed credential
/// still fails identity/trust validation; it is never converted to absence.
struct SyntheticTrust;
impl floe_access::GatewayTrustReader for SyntheticTrust {
    fn credential_expectation<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<floe_access::GatewayCredentialExpectation, AgentFailure>> {
        Box::pin(async { Ok(floe_access::GatewayCredentialExpectation::Unpaired) })
    }
    fn pinned_producer<'a>(
        &'a self,
    ) -> BoxFuture<'a, Result<floe_access::RemoteProducerIdentity, AgentFailure>> {
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

pub(super) fn synthetic_model() -> DiagnosticModel {
    model(GatewayCredentialStore::new(Arc::new(SyntheticTrust)))
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
