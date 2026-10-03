//! Commands for the one host-owned encrypted Vault lifecycle.
use uuid::Uuid;
use crate::{AgentFailure, AppComposition, CallerContext};
use crate::vault_lifecycle::VaultLifecycleIntent;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VaultState { #[default] Missing, Locked, Ready, Unavailable }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VaultLifecycleCommand { Create, Unlock, Lock }
#[derive(Clone, Debug)]
pub struct VaultLifecycleResult {
    pub operation_id: Uuid, pub stage: String, pub done: bool,
    pub state: Option<VaultState>, pub failure: Option<AgentFailure>,
}
pub trait VaultLifecycleCommands {
    fn vault_command(&self, caller:&CallerContext, operation_id:Uuid, command:VaultLifecycleCommand)
        -> Result<VaultLifecycleResult,AgentFailure>;
}
pub trait VaultLifecycleQueries {
    fn vault_status(&self, caller:&CallerContext, request_id:Uuid)->Result<VaultLifecycleResult,AgentFailure>;
    fn read_vault_result(&self, caller:&CallerContext, operation_id:Uuid, release:bool)->Result<VaultLifecycleResult,AgentFailure>;
}
impl VaultLifecycleCommands for AppComposition {
    fn vault_command(&self, caller:&CallerContext, operation_id:Uuid, command:VaultLifecycleCommand)
        -> Result<VaultLifecycleResult,AgentFailure> {
        let intent = match command { VaultLifecycleCommand::Create=>VaultLifecycleIntent::Create,
            VaultLifecycleCommand::Unlock=>VaultLifecycleIntent::Unlock, VaultLifecycleCommand::Lock=>VaultLifecycleIntent::Lock };
        self.agent_vault.request(caller,operation_id,Some(intent),false)
    }
}
impl VaultLifecycleQueries for AppComposition {
    fn vault_status(&self, caller:&CallerContext, request_id:Uuid)->Result<VaultLifecycleResult,AgentFailure> {
        if request_id.is_nil(){return Err(AgentFailure::InvalidInput);}
        Ok(VaultLifecycleResult {operation_id:request_id,stage:"status".into(),done:true,
            state:Some(self.agent_vault.status(caller)?),failure:None})
    }
    fn read_vault_result(&self, caller:&CallerContext, operation_id:Uuid, release:bool)->Result<VaultLifecycleResult,AgentFailure> {
        self.agent_vault.request(caller,operation_id,None,release)
    }
}
