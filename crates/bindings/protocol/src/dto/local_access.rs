use super::{
    AgentVaultFailureDto, AgentVaultStateDto, CalendarProviderDto, CalendarScopeDto,
    CalendarSubjectPreviewDto, PersonalAccessOverviewDto,
};
use floe_context_contract::{ConsumerPolicyAuthority, GrantAuthority, GrantId, SourceAuthority};
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
pub enum CalendarAccessChangeDto {
    Review {
        connection_id: String,
        calendar_ids: Vec<String>,
        expected_source_authority: SourceAuthority,
        expected_native_subject_fingerprint: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_grant_id: Option<GrantId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_grant_authority: Option<GrantAuthority>,
    },
    Pause {
        grant_id: GrantId,
        expected_grant_authority: GrantAuthority,
    },
    Remove {
        grant_id: GrantId,
        expected_grant_authority: GrantAuthority,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalendarAccessOverviewDto {
    pub schema_version: u32,
    pub person_id: String,
    pub provider: CalendarProviderDto,
    pub connection_id: String,
    pub selected_resources: Vec<String>,
    pub granted_resources: Vec<String>,
    pub source_authority: SourceAuthority,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_id: Option<GrantId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_authority: Option<GrantAuthority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumer_policy: Option<ConsumerPolicyAuthority>,
    pub state: String,
    pub review_required: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalAccessResultDto {
    pub operation_id: Uuid,
    pub done: bool,
    pub state: Option<AgentVaultStateDto>,
    pub calendar_subject_preview: Option<CalendarSubjectPreviewDto>,
    pub calendar_access: Option<CalendarAccessOverviewDto>,
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

impl CalendarAccessChangeDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Review {
                connection_id,
                calendar_ids,
                expected_source_authority,
                expected_native_subject_fingerprint,
                expected_grant_id,
                expected_grant_authority,
            } => {
                if !identifier(connection_id)
                    || calendar_ids.is_empty()
                    || !identifiers(calendar_ids, 4)
                    || !expected_source_authority.is_valid()
                    || expected_native_subject_fingerprint.is_empty()
                    || expected_native_subject_fingerprint.len() > 256
                {
                    return Err("command.change");
                }
                match (expected_grant_id, expected_grant_authority) {
                    (None, None) => Ok(()),
                    (Some(id), Some(authority)) if id.is_valid() && authority.is_valid() => Ok(()),
                    _ => Err("command.change.expected_grant"),
                }
            }
            Self::Pause {
                grant_id,
                expected_grant_authority,
            }
            | Self::Remove {
                grant_id,
                expected_grant_authority,
            } => {
                if !grant_id.is_valid() || !expected_grant_authority.is_valid() {
                    return Err("command.change.expected_grant");
                }
                Ok(())
            }
        }
    }
}
