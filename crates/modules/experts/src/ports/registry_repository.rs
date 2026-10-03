use floe_agent_contract::{AgentFailure, BoxFuture, CommandId, ExecutionScope, OwnerActor};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryCommitReceipt {
    pub command_id: CommandId,
    pub person_id: floe_kernel::PersonId,
    pub device_id: String,
    pub request_digest: [u8; 32],
    pub snapshot: crate::RegistrySnapshot,
}

pub struct RegistryCommit {
    pub actor: OwnerActor,
    pub command_id: CommandId,
    pub request_digest: [u8; 32],
    pub expected_revision: u64,
    pub next: crate::RegistrySnapshot,
}

pub trait RegistryRepository: Send + Sync {
    fn read<'a>(
        &'a self,
        actor: &'a OwnerActor,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<crate::RegistrySnapshot, AgentFailure>>;
    fn find_command<'a>(
        &'a self,
        actor: &'a OwnerActor,
        command_id: CommandId,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<Option<RegistryCommitReceipt>, AgentFailure>>;
    fn commit<'a>(
        &'a self,
        commit: RegistryCommit,
        scope: &'a ExecutionScope,
    ) -> BoxFuture<'a, Result<RegistryCommitReceipt, AgentFailure>>;
}
