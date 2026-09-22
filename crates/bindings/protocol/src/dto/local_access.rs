use super::{
    AgentVaultFailureDto, AgentVaultStateDto, CalendarExpertOverviewDto, CalendarProviderDto,
    CalendarScopeDto, CalendarSubjectPreviewDto, PersonalAccessOverviewDto,
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
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CalendarGrantChangeDto {
    SetEnabled {
        enabled: bool,
    },
    SetScope {
        provider: CalendarProviderDto,
        calendar_ids: Vec<String>,
        connection_scope: CalendarScopeDto,
        connection_revision: u64,
        source_authority: Option<SourceAuthority>,
        reviewed_native_subject_fingerprint: Option<String>,
    },
    Remove {},
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarGrantConfigurationDto {
    pub instance_id: Uuid,
    pub expected_revision: u64,
    pub setup_id: Uuid,
    pub change: CalendarGrantChangeDto,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalAccessResultDto {
    pub operation_id: Uuid,
    pub done: bool,
    pub state: Option<AgentVaultStateDto>,
    pub calendar_experts: Option<CalendarExpertOverviewDto>,
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

impl CalendarGrantConfigurationDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.instance_id.is_nil()
            || self.setup_id.is_nil()
            || self.expected_revision > i64::MAX as u64
        {
            return Err("command.change");
        }
        if let CalendarGrantChangeDto::SetScope {
            calendar_ids,
            connection_revision,
            reviewed_native_subject_fingerprint,
            ..
        } = &self.change
        {
            if !identifiers(calendar_ids, 4)
                || *connection_revision == 0
                || *connection_revision > i64::MAX as u64
                || reviewed_native_subject_fingerprint
                    .as_ref()
                    .is_some_and(|value| {
                        value.len() != 64
                            || !value
                                .bytes()
                                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
                    })
            {
                return Err("command.change.scope");
            }
        }
        Ok(())
    }
}
