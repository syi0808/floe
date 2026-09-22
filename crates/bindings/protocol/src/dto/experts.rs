use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    AgentVaultFailureDto, AgentVaultStateDto, CalendarExpertOverviewDto, CalendarProviderDto,
    CalendarScopeDto, RegistryOverviewDto,
};
use floe_context_contract::SourceAuthority;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarExpertInstallDto {
    pub instance_id: Uuid,
    pub expected_revision: u64,
    pub setup_id: Uuid,
    pub provider: CalendarProviderDto,
    pub calendar_ids: Vec<String>,
    pub connection_scope: CalendarScopeDto,
    pub connection_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_authority: Option<SourceAuthority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_native_subject_fingerprint: Option<String>,
}

impl CalendarExpertInstallDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.instance_id.is_nil() || self.setup_id.is_nil() {
            return Err("command.setup.identity");
        }
        if self.expected_revision > i64::MAX as u64
            || self.connection_revision == 0
            || self.connection_revision > i64::MAX as u64
        {
            return Err("command.setup.revision");
        }
        if self.calendar_ids.len() > 4
            || self.calendar_ids.iter().any(|value| {
                value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
            })
        {
            return Err("command.setup.calendar_ids");
        }
        if self
            .reviewed_native_subject_fingerprint
            .as_ref()
            .is_some_and(|value| {
                value.len() != 64
                    || !value
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            })
        {
            return Err("command.setup.reviewed_native_subject_fingerprint");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExpertOperationResultDto {
    pub operation_id: Uuid,
    pub done: bool,
    pub state: Option<AgentVaultStateDto>,
    pub registry: Option<RegistryOverviewDto>,
    pub calendar_experts: Option<CalendarExpertOverviewDto>,
    pub failure: Option<AgentVaultFailureDto>,
}
