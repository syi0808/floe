use floe_context_contract::{ConnectionId, ConnectorId, GrantAuthority, GrantId, SourceAuthority};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AgentVaultFailureDto, AgentVaultStateDto};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionObserveStatusDto {
    Active,
    Paused,
    NeedsReview,
    NeedsSystemAccess,
    ReconnectRequired,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionObserveGrantStateDto {
    Active,
    Paused,
    Revoked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionObserveMemberDto {
    pub view_id: String,
    pub state: ConnectionObserveGrantStateDto,
    pub review_required: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionObserveOverviewDto {
    pub connector_id: String,
    pub connection_id: String,
    pub status: ConnectionObserveStatusDto,
    pub enabled: bool,
    pub source_resources: Vec<String>,
    pub members: Vec<ConnectionObserveMemberDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionObserveReviewedMemberDto {
    pub view_id: String,
    pub policy_digest: String,
    pub resource: String,
    pub expected_grant_id: Option<GrantId>,
    pub expected_grant_authority: Option<GrantAuthority>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionObserveExpectationDto {
    pub connector_id: String,
    pub connection_id: String,
    pub source_authority: SourceAuthority,
    pub connection_revision: Option<u64>,
    pub native_subject: Option<String>,
    pub producer_fingerprint: Option<String>,
    pub members: Vec<ConnectionObserveReviewedMemberDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionObserveMutationDto {
    pub connector_id: String,
    pub connection_id: String,
    pub enabled: bool,
    #[serde(default)]
    pub disconnecting: bool,
    pub expected: Option<ConnectionObserveExpectationDto>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionObserveResultDto {
    pub operation_id: Uuid,
    pub done: bool,
    pub state: Option<AgentVaultStateDto>,
    pub overview: Option<ConnectionObserveOverviewDto>,
    pub reviewed: Option<ConnectionObserveExpectationDto>,
    pub failure: Option<AgentVaultFailureDto>,
}

pub(crate) fn validate_identity(
    connector_id: &str,
    connection_id: &str,
) -> Result<(), &'static str> {
    ConnectorId::try_new(connector_id).map_err(|_| "connection_observe.connector_id")?;
    ConnectionId::try_new(connection_id).map_err(|_| "connection_observe.connection_id")?;
    Ok(())
}

impl ConnectionObserveMutationDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        validate_identity(&self.connector_id, &self.connection_id)?;
        if self.enabled == self.expected.is_none() || (self.enabled && self.disconnecting) {
            return Err("connection_observe.expected");
        }
        if let Some(expected) = &self.expected {
            expected.validate()?;
            if expected.connector_id != self.connector_id
                || expected.connection_id != self.connection_id
            {
                return Err("connection_observe.expected.identity");
            }
        }
        Ok(())
    }
}

impl ConnectionObserveExpectationDto {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        validate_identity(&self.connector_id, &self.connection_id)?;
        let connection = ConnectionId::try_new(&self.connection_id)
            .map_err(|_| "connection_observe.connection_id")?;
        if !self.source_authority.is_valid()
            || self
                .connection_revision
                .is_some_and(|revision| revision == 0)
            || self.native_subject.is_some() == self.producer_fingerprint.is_some()
            || self.members.is_empty()
            || self.members.len() > 8
        {
            return Err("connection_observe.expected");
        }
        for value in [
            self.native_subject.as_deref(),
            self.producer_fingerprint.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if !valid_text(value, 256) {
                return Err("connection_observe.expected.subject");
            }
        }
        let mut previous_view = None;
        for member in &self.members {
            if !valid_text(&member.view_id, 128)
                || previous_view.is_some_and(|previous| previous >= member.view_id.as_str())
                || member.policy_digest.len() != 64
                || !member
                    .policy_digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                || floe_context_contract::connection_view_resource(&member.view_id, &connection)
                    .map_or(true, |resource| resource.as_str() != member.resource)
            {
                return Err("connection_observe.expected.member");
            }
            match (member.expected_grant_id, member.expected_grant_authority) {
                (None, None) => {}
                (Some(id), Some(authority)) if id.is_valid() && authority.is_valid() => {}
                _ => return Err("connection_observe.expected.grant"),
            }
            previous_view = Some(member.view_id.as_str());
        }
        Ok(())
    }
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_requires_exact_review_only_for_enable() {
        let mut mutation = ConnectionObserveMutationDto {
            connector_id: "calendar.apple".into(),
            connection_id: Uuid::new_v4().to_string(),
            enabled: true,
            disconnecting: false,
            expected: None,
        };
        assert!(mutation.validate().is_err());
        mutation.enabled = false;
        assert!(mutation.validate().is_ok());
        mutation.disconnecting = true;
        assert!(mutation.validate().is_ok());
        mutation.enabled = true;
        assert!(mutation.validate().is_err());
    }

    #[test]
    fn observe_wire_rejects_leaf_and_routing_authority_fields() {
        let mutation = serde_json::json!({
            "connector_id": "contacts.apple",
            "connection_id": Uuid::new_v4().to_string(),
            "enabled": false,
            "expected": null,
            "selected_handles": ["contact-a"]
        });
        assert!(serde_json::from_value::<ConnectionObserveMutationDto>(mutation).is_err());
        let expectation = serde_json::json!({
            "connector_id": "remote.calendar",
            "connection_id": Uuid::new_v4().to_string(),
            "source_authority": {"revision": 1},
            "connection_revision": 1,
            "native_subject": null,
            "producer_fingerprint": "abc",
            "members": [],
            "provider_identity": "caller-supplied"
        });
        assert!(serde_json::from_value::<ConnectionObserveExpectationDto>(expectation).is_err());
    }
}
