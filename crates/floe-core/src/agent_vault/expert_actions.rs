use std::future::Future;

use floe_agent::{AgentMessage, AgentRegistry, ExpertResult};
use turso::transaction::TransactionBehavior;

use super::*;
use crate::ExpertProposalReference;

impl<Keys: VaultKeyProvider> EncryptedAgentVault<Keys> {
    pub(crate) async fn with_expert_proposal<ResultValue, Publish>(
        &self,
        reference: &ExpertProposalReference,
        publish: impl FnOnce(ExpertResult) -> Publish,
    ) -> Result<ResultValue, AgentFailure>
    where
        Publish: Future<Output = Result<ResultValue, AgentFailure>>,
    {
        if reference.person_id != self.person_id {
            return Err(AgentFailure::NotFound);
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await
            .map_err(storage)?;
        let result = async {
            let session = self.session_on(&transaction, reference.session_id).await?;
            let snapshot = self
                .registry_on(&transaction)
                .await?
                .ok_or(AgentFailure::NotFound)?;
            let outputs: Vec<_> = session
                .messages
                .iter()
                .filter_map(|message| match message {
                    AgentMessage::Capability { call_id, result: Ok(output), .. }
                        if *call_id == reference.invocation_id => Some(output),
                    _ => None,
                })
                .collect();
            let [output] = outputs.as_slice() else {
                return Err(AgentFailure::NotFound);
            };
            if output.len() > 16_384 {
                return Err(AgentFailure::BudgetExceeded);
            }
            let evidence: ExpertResult =
                serde_json::from_str(output).map_err(|_| AgentFailure::InvalidInput)?;
            if evidence.invocation_id != reference.invocation_id
                || evidence.person_id != reference.person_id
                || evidence.action_proposals.len() != 1
                || !session.data_classes.contains(&evidence.data_class)
            {
                return Err(AgentFailure::InvalidInput);
            }
            let mut receipts = transaction
                .query(
                    "SELECT session_id, assignment_id, registry_revision FROM agent_expert_receipts WHERE invocation_id = ?",
                    [reference.invocation_id.to_string()],
                )
                .await
                .map_err(storage)?;
            let receipt = receipts.next().await.map_err(storage)?.ok_or(AgentFailure::NotFound)?;
            let revision = receipt.get::<i64>(2).map_err(storage)?;
            if receipt.get::<String>(0).map_err(storage)? != reference.session_id.to_string()
                || receipt.get::<String>(1).map_err(storage)? != evidence.assignment_id.to_string()
                || revision <= 0
                || u64::try_from(revision).map_err(|_| AgentFailure::InvalidInput)? > snapshot.revision
            {
                return Err(AgentFailure::Conflict);
            }
            drop(receipts);
            let registry = AgentRegistry::restore(snapshot, self.vault_id)?;
            if evidence.source_handle.starts_with("calendar.timeline:")
                && registry.calendar_view(evidence.person_id, evidence.view_handle)?.data_class() != evidence.data_class
            {
                return Err(AgentFailure::PolicyDenied);
            }
            registry.validate_recorded_result(&evidence)?;
            self.check_access()?;
            let value = publish(evidence).await?;
            self.check_access()?;
            Ok(value)
        }
        .await;
        self.finish_registry_transaction(transaction, result).await
    }
}
