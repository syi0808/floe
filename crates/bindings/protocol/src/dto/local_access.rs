use super::{
    AgentVaultFailureDto, AgentVaultStateDto, CalendarProviderDto, CalendarScopeDto,
    CalendarSubjectPreviewDto, PersonalAccessOverviewDto,
};
use floe_context_contract::SourceAuthority;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarSubjectIntentDto {
    pub provider: CalendarProviderDto,
    pub connection_id: String,
    pub calendar_ids: Vec<String>,
    pub connection_scope: CalendarScopeDto,
    pub connection_revision: u64,
    pub source_authority: SourceAuthority,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalAccessResultDto {
    pub operation_id: Uuid,
    pub done: bool,
    pub state: Option<AgentVaultStateDto>,
    pub calendar_subject_preview: Option<CalendarSubjectPreviewDto>,
    pub personal_access: Option<PersonalAccessOverviewDto>,
    pub failure: Option<AgentVaultFailureDto>,
}

pub(crate) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

pub(crate) fn identifiers(values: &[String], maximum: usize) -> bool {
    values.len() <= maximum && values.iter().all(|value| identifier(value))
}

impl CalendarSubjectIntentDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !identifier(&self.connection_id)
            || !identifiers(&self.calendar_ids, 4)
            || self.connection_revision == 0
            || self.connection_revision > i64::MAX as u64
        {
            Err("query.request")
        } else {
            Ok(())
        }
    }
}
