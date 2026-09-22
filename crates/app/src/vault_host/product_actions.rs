use super::*;
use crate::{CalendarActionCommand, CalendarActionsResult};

pub(super) async fn execute<Keys: VaultKeyProvider>(
    core: &FloeCore,
    vault: Option<&EncryptedAgentVault<Keys>>,
    person: PersonId,
    operation: &CalendarActionOperation,
    cancellation: &Cancellation,
) -> Result<CalendarActionsResult, AgentFailure> {
    if cancellation.is_cancelled() {
        return Err(AgentFailure::Cancelled);
    }
    if matches!(
        operation,
        CalendarActionOperation::GetAuthority | CalendarActionOperation::SetAuthority { .. }
    ) {
        return execute_agent_calendar_action(
            core,
            vault.ok_or(AgentFailure::VaultUnavailable)?,
            person,
            operation,
            cancellation,
        )
        .await;
    }
    if let CalendarActionOperation::List = operation {
        let mut result = core
            .calendar_action_command(person, CalendarActionCommand::List, chrono::Utc::now())
            .await
            .map_err(core_failure)?;
        result
            .actions
            .retain(|action| action.agent_origin.is_none());
        if let Some(vault) = vault {
            let owned = vault.agent_calendar_actions().await?;
            for action in owned {
                if action.person_id != person || action.agent_origin.is_none() || action.direct {
                    return Err(AgentFailure::PolicyDenied);
                }
                if result
                    .actions
                    .iter()
                    .any(|existing| existing.id == action.id)
                {
                    return Err(AgentFailure::Conflict);
                }
                result.actions.push(action);
            }
        }
        return Ok(result);
    }
    let action_id = match operation {
        CalendarActionOperation::Get { action_id }
        | CalendarActionOperation::Decide { action_id, .. }
        | CalendarActionOperation::Execute { action_id }
        | CalendarActionOperation::Recover { action_id } => Some(*action_id),
        _ => None,
    };
    if let Some(action_id) = action_id {
        if vault.is_none() {
            return Err(AgentFailure::VaultUnavailable);
        }
        if let Some(vault) = vault {
            match vault.agent_calendar_action(action_id).await {
                Ok(_) => {
                    return execute_agent_calendar_action(
                        core,
                        vault,
                        person,
                        operation,
                        cancellation,
                    )
                    .await;
                }
                Err(AgentFailure::NotFound) => {}
                Err(failure) => return Err(failure),
            }
        }
        let stored = core
            .calendar_action_command(
                person,
                CalendarActionCommand::Get { action_id },
                chrono::Utc::now(),
            )
            .await
            .map_err(core_failure)?;
        if stored.actions.len() != 1 || stored.actions[0].agent_origin.is_some() {
            return Err(AgentFailure::Conflict);
        }
        if matches!(operation, CalendarActionOperation::Get { .. }) {
            return Ok(stored);
        }
    }
    let command = match operation {
        CalendarActionOperation::Capabilities => CalendarActionCommand::Capabilities,
        CalendarActionOperation::Get { action_id } => CalendarActionCommand::Get {
            action_id: *action_id,
        },
        CalendarActionOperation::Decide { action_id, approve } => CalendarActionCommand::Decide {
            action_id: *action_id,
            approve: *approve,
        },
        CalendarActionOperation::Execute { action_id } => CalendarActionCommand::Execute {
            action_id: *action_id,
        },
        CalendarActionOperation::Recover { action_id } => CalendarActionCommand::Recover {
            action_id: *action_id,
        },
        CalendarActionOperation::Propose(proposal) => CalendarActionCommand::Propose {
            calendar_id: proposal.calendar_id.clone(),
            title: proposal.title.clone(),
            schedule: schedule(proposal)?,
        },
        CalendarActionOperation::Direct(proposal) => {
            let target = match (&proposal.event_id, proposal.event_revision) {
                (Some(identifier), Some(revision)) => Some((
                    floe_kernel::EventId(
                        Uuid::parse_str(identifier).map_err(|_| AgentFailure::InvalidInput)?,
                    ),
                    floe_kernel::Revision(revision),
                )),
                (None, None) => None,
                _ => return Err(AgentFailure::InvalidInput),
            };
            CalendarActionCommand::Direct {
                calendar_id: proposal.calendar_id.clone(),
                title: proposal.title.clone(),
                schedule: schedule(proposal)?,
                target,
                delete: proposal.delete,
            }
        }
        CalendarActionOperation::GetAuthority
        | CalendarActionOperation::SetAuthority { .. }
        | CalendarActionOperation::List => return Err(AgentFailure::InvalidInput),
    };
    core.calendar_action_command(person, command, chrono::Utc::now())
        .await
        .map_err(core_failure)
}

fn schedule(
    proposal: &crate::CalendarActionProposal,
) -> Result<crate::TimedSchedule, AgentFailure> {
    crate::TimedSchedule::new(
        chrono::DateTime::parse_from_rfc3339(&proposal.starts_at)
            .map_err(|_| AgentFailure::InvalidInput)?
            .with_timezone(&chrono::Utc),
        chrono::DateTime::parse_from_rfc3339(&proposal.ends_at)
            .map_err(|_| AgentFailure::InvalidInput)?
            .with_timezone(&chrono::Utc),
        &proposal.timezone,
    )
    .map_err(|_| AgentFailure::InvalidInput)
}

fn core_failure(error: crate::CoreError) -> AgentFailure {
    match error.code {
        crate::ErrorCode::Validation | crate::ErrorCode::NoFocusSlot => AgentFailure::InvalidInput,
        crate::ErrorCode::NotFound => AgentFailure::NotFound,
        crate::ErrorCode::Conflict => AgentFailure::Conflict,
        crate::ErrorCode::Storage => AgentFailure::StorageUnavailable,
    }
}
