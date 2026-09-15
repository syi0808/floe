use floe_agent_contract::AgentFailure;

use crate::{CommandQuery, ConversationRepository, RunQuery, RunReceipt};

pub async fn get_command<Repository: ConversationRepository>(
    repository: &Repository,
    query: CommandQuery,
) -> Result<Option<RunReceipt>, AgentFailure> {
    query.validate()?;
    authorize(
        &query.principal,
        repository.find_command(query.clone()).await?,
    )
}

pub async fn get_run<Repository: ConversationRepository>(
    repository: &Repository,
    query: RunQuery,
) -> Result<Option<RunReceipt>, AgentFailure> {
    query.validate()?;
    authorize(
        &query.principal,
        repository.load_receipt(query.run_id).await?,
    )
}

fn authorize(
    principal: &str,
    receipt: Option<RunReceipt>,
) -> Result<Option<RunReceipt>, AgentFailure> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    receipt.validate()?;
    if receipt.principal != principal {
        return Err(AgentFailure::CapabilityDenied);
    }
    Ok(Some(receipt))
}
