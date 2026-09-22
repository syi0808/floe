use super::{
    AgentProposalInspectionDto, AgentVaultFailureDto, AgentVaultStateDto,
    CalendarActionOperationDto,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActionOperationResultDto {
    pub operation_id: Uuid,
    pub done: bool,
    pub state: Option<AgentVaultStateDto>,
    pub calendar_actions: Option<serde_json::Value>,
    pub proposal: Option<AgentProposalInspectionDto>,
    pub failure: Option<AgentVaultFailureDto>,
}

pub(crate) fn validate_command(operation: &CalendarActionOperationDto) -> Result<(), &'static str> {
    let valid_id =
        |value: &str| Uuid::parse_str(value).is_ok_and(|identifier| !identifier.is_nil());
    match operation {
        CalendarActionOperationDto::Execute { action_id }
        | CalendarActionOperationDto::Recover { action_id }
        | CalendarActionOperationDto::Decide { action_id, .. } => {
            if !valid_id(action_id) {
                return Err("command.operation.action_id");
            }
        }
        CalendarActionOperationDto::Propose {
            calendar_id,
            title,
            starts_at,
            ends_at,
            timezone,
        }
        | CalendarActionOperationDto::Direct {
            calendar_id,
            title,
            starts_at,
            ends_at,
            timezone,
            ..
        } => {
            if !super::local_access::identifier(calendar_id)
                || title.is_empty()
                || title.len() > 4096
                || timezone.is_empty()
                || timezone.len() > 128
                || chrono::DateTime::parse_from_rfc3339(starts_at).is_err()
                || chrono::DateTime::parse_from_rfc3339(ends_at).is_err()
            {
                return Err("command.operation.proposal");
            }
            if let CalendarActionOperationDto::Direct {
                event_id,
                event_revision,
                ..
            } = operation
            {
                if event_id
                    .as_ref()
                    .is_some_and(|identifier| !valid_id(identifier))
                    || event_id.is_some() != event_revision.is_some()
                    || event_revision.is_some_and(|revision| revision > i64::MAX as u64)
                {
                    return Err("command.operation.target");
                }
            }
        }
        CalendarActionOperationDto::SetAuthority { .. } => {}
        CalendarActionOperationDto::Capabilities {}
        | CalendarActionOperationDto::GetAuthority {}
        | CalendarActionOperationDto::List {}
        | CalendarActionOperationDto::Get { .. } => return Err("command.operation"),
    }
    Ok(())
}
