//! Fresh encrypted layout admission. Optional owner state is represented by rows.
use super::{EncryptedAgentVault, VaultKeyProvider, storage};
use floe_kernel::AgentFailure;
use turso::transaction::TransactionBehavior;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(super) async fn create_schema(&self) -> Result<(), AgentFailure> {
        let expectation =
            serde_json::to_string(&floe_access::GatewayCredentialExpectation::Unpaired)
                .map_err(storage)?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(|error| crate::schema::SchemaFailure::from_database(error).into_agent())?;
        let result = async {
            crate::schema::create(&transaction, crate::schema::Layout::Encrypted)
                .await
                .map_err(|failure| failure.during_creation().into_agent())?;
            transaction
                .execute(
                    "INSERT INTO vault_identity(id,version,person_id,vault_id) VALUES(1,?,?,?)",
                    (
                        crate::schema::ENCRYPTED_LAYOUT_VERSION,
                        self.person_id.to_string(),
                        self.vault_id.to_string(),
                    ),
                )
                .await
                .map_err(storage)?;
            for statement in [
                "INSERT INTO agent_conversation_executor(id,generation) VALUES(1,0)",
                "INSERT INTO agent_task_executor(id,generation) VALUES(1,0)",
                "INSERT INTO remote_authority_clock(id,last_now_unix_ms) VALUES(1,0)",
                "INSERT INTO gateway_pairing_generation(id,generation) VALUES(1,0)",
            ] {
                transaction.execute(statement, ()).await.map_err(storage)?;
            }
            transaction
                .execute(
                    "INSERT INTO gateway_credential_expectation(id,payload) VALUES(1,?)",
                    (expectation,),
                )
                .await
                .map_err(storage)?;
            self.seed_actions_authority(&transaction).await?;
            crate::schema::inspect(&transaction, crate::schema::Layout::Encrypted)
                .await
                .map_err(|failure| failure.during_creation().into_agent())?;
            self.check_access()
        }
        .await;
        self.finish_access_grant_transaction(transaction, result)
            .await
    }
}
