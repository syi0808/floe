use floe_agent_contract::AgentFailure;
use uuid::Uuid;

use crate::{ExpertInstallOperation, ExpertInstallResult};

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

pub trait ExpertInstallStore: Sync {
    fn instance_id(&self) -> Uuid;
    fn operation_id(&self) -> Uuid;
    fn manifest_digest(&self) -> Result<String, AgentFailure>;
    fn overview<'a>(&'a self) -> BoxFuture<'a, Result<Option<ExpertInstallResult>, AgentFailure>>;
    fn registry_revision<'a>(&'a self) -> BoxFuture<'a, Result<u64, AgentFailure>>;
    fn install<'a>(
        &'a self,
        operation: ExpertInstallOperation,
    ) -> BoxFuture<'a, Result<ExpertInstallResult, AgentFailure>>;
}

pub async fn ensure_expert_bundle(store: &impl ExpertInstallStore) -> Result<(), AgentFailure> {
    let existing = store.overview().await?;
    let ensured = match existing {
        Some(existing) => Ok(existing),
        None => {
            store
                .install(ExpertInstallOperation {
                    instance_id: store.instance_id(),
                    expected_revision: store.registry_revision().await?,
                    operation_id: store.operation_id(),
                })
                .await
        }
    };
    let result = match ensured {
        Ok(result) => result,
        Err(AgentFailure::Conflict) => store.overview().await?.ok_or(AgentFailure::Conflict)?,
        Err(failure) => return Err(failure),
    };
    if result.receipt.manifest_digest != store.manifest_digest()? {
        return Err(AgentFailure::Conflict);
    }
    Ok(())
}
