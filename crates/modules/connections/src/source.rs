use floe_context_contract::{
    ConnectionId, ConnectorId, ExecutionOwnerId, ResourceHandle, SourceAuthority,
};
use floe_kernel::PersonId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_RESOURCE_LABEL_BYTES: usize = 256;
const MAX_RESOURCES: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionResource {
    handle: ResourceHandle,
    label: String,
}

impl ConnectionResource {
    pub fn new(handle: ResourceHandle, label: String) -> Result<Self, SourceConnectionError> {
        let resource = Self { handle, label };
        resource.validate()?;
        Ok(resource)
    }

    pub fn handle(&self) -> &ResourceHandle {
        &self.handle
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    fn validate(&self) -> Result<(), SourceConnectionError> {
        if self.handle.as_str().is_empty()
            || self.handle.as_str() == "*"
            || self.handle.as_str().len() > floe_context_contract::MAX_RESOURCE_HANDLE_BYTES
            || self.handle.as_str().chars().any(char::is_control)
            || self.label.is_empty()
            || self.label.len() > MAX_RESOURCE_LABEL_BYTES
            || self.label.chars().any(char::is_control)
        {
            return Err(SourceConnectionError::InvalidResource);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceMode {
    Selected,
    AllAvailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceState {
    Pending,
    Ready,
    Disconnected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConnection {
    person_id: PersonId,
    connector_id: ConnectorId,
    connection_id: ConnectionId,
    execution_owner_id: ExecutionOwnerId,
    state: SourceState,
    revision: u64,
    source_authority: SourceAuthority,
    resource_mode: ResourceMode,
    resources: Vec<ConnectionResource>,
    native_subject_fingerprint: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SourceConnectionError {
    #[error("invalid source identity")]
    InvalidIdentity,
    #[error("invalid source revision or authority")]
    InvalidAuthority,
    #[error("invalid source resource")]
    InvalidResource,
    #[error("invalid native subject fingerprint")]
    InvalidSubject,
    #[error("source revision conflict")]
    Conflict,
    #[error("source is disconnected")]
    Disconnected,
    #[error("source revision exhausted")]
    RevisionExhausted,
    #[error("invalid source transition")]
    InvalidTransition,
}

impl SourceConnection {
    pub fn establish(
        person_id: PersonId,
        connector_id: ConnectorId,
        connection_id: ConnectionId,
        execution_owner_id: ExecutionOwnerId,
        resource_mode: ResourceMode,
        resources: Vec<ConnectionResource>,
    ) -> Result<Self, SourceConnectionError> {
        let mut source = Self {
            person_id,
            connector_id,
            connection_id,
            execution_owner_id,
            state: SourceState::Pending,
            revision: 1,
            source_authority: SourceAuthority::new(),
            resource_mode,
            resources: normalize_resources(resources)?,
            native_subject_fingerprint: None,
        };
        if !source.requires_native_subject() {
            source.state = SourceState::Ready;
        }
        source.validate()?;
        Ok(source)
    }

    pub fn validate(&self) -> Result<(), SourceConnectionError> {
        if !self.person_id.is_valid()
            || ConnectorId::try_new(self.connector_id.as_str()).is_err()
            || ConnectionId::try_new(self.connection_id.as_str()).is_err()
            || ExecutionOwnerId::try_new(self.execution_owner_id.as_str()).is_err()
        {
            return Err(SourceConnectionError::InvalidIdentity);
        }
        if self.revision == 0
            || self.revision > i64::MAX as u64
            || !self.source_authority.is_valid()
        {
            return Err(SourceConnectionError::InvalidAuthority);
        }
        if normalize_resources(self.resources.clone())? != self.resources {
            return Err(SourceConnectionError::InvalidResource);
        }
        if let Some(fingerprint) = &self.native_subject_fingerprint {
            if !valid_fingerprint(fingerprint) || !self.requires_native_subject() {
                return Err(SourceConnectionError::InvalidSubject);
            }
        }
        if self.state == SourceState::Ready
            && self.requires_native_subject()
            && self.native_subject_fingerprint.is_none()
        {
            return Err(SourceConnectionError::InvalidSubject);
        }
        Ok(())
    }

    pub fn validate_successor(&self, next: &Self) -> Result<(), SourceConnectionError> {
        self.validate()?;
        next.validate()?;
        if self.person_id != next.person_id
            || self.connector_id != next.connector_id
            || self.connection_id != next.connection_id
            || self.execution_owner_id != next.execution_owner_id
            || self.state == SourceState::Disconnected
            || next.revision
                != self
                    .revision
                    .checked_add(1)
                    .ok_or(SourceConnectionError::RevisionExhausted)?
        {
            return Err(SourceConnectionError::InvalidTransition);
        }
        let expected_state = if next.state == SourceState::Disconnected {
            SourceState::Disconnected
        } else if next.requires_native_subject() && next.native_subject_fingerprint.is_none() {
            SourceState::Pending
        } else {
            SourceState::Ready
        };
        if next.state != expected_state {
            return Err(SourceConnectionError::InvalidTransition);
        }
        if next.state == SourceState::Disconnected {
            if self.resource_mode != next.resource_mode
                || self.resources != next.resources
                || self.native_subject_fingerprint != next.native_subject_fingerprint
            {
                return Err(SourceConnectionError::InvalidTransition);
            }
        } else if self.native_subject_fingerprint != next.native_subject_fingerprint {
            if self.resource_mode != next.resource_mode || self.resources != next.resources {
                return Err(SourceConnectionError::InvalidTransition);
            }
        } else if self.state != next.state {
            return Err(SourceConnectionError::InvalidTransition);
        }
        let scope_changed = self.resource_mode != next.resource_mode
            || self
                .resources
                .iter()
                .map(ConnectionResource::handle)
                .ne(next.resources.iter().map(ConnectionResource::handle))
            || self.native_subject_fingerprint != next.native_subject_fingerprint
            || self.state != next.state;
        let expected_authority = if scope_changed {
            self.source_authority
                .advance()
                .ok_or(SourceConnectionError::RevisionExhausted)?
        } else {
            self.source_authority
        };
        if next.source_authority != expected_authority
            || (self.resource_mode == next.resource_mode
                && self.resources == next.resources
                && self.native_subject_fingerprint == next.native_subject_fingerprint
                && self.state == next.state)
        {
            return Err(SourceConnectionError::InvalidTransition);
        }
        Ok(())
    }

    pub fn person_id(&self) -> PersonId {
        self.person_id
    }
    pub fn connector_id(&self) -> &ConnectorId {
        &self.connector_id
    }
    pub fn connection_id(&self) -> &ConnectionId {
        &self.connection_id
    }
    pub fn execution_owner_id(&self) -> &ExecutionOwnerId {
        &self.execution_owner_id
    }
    pub fn state(&self) -> SourceState {
        self.state
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn source_authority(&self) -> SourceAuthority {
        self.source_authority
    }
    pub fn resource_mode(&self) -> ResourceMode {
        self.resource_mode
    }
    pub fn resources(&self) -> &[ConnectionResource] {
        &self.resources
    }
    pub fn native_subject_fingerprint(&self) -> Option<&str> {
        self.native_subject_fingerprint.as_deref()
    }
    pub fn is_serving(&self) -> bool {
        self.state == SourceState::Ready
    }

    pub fn configure(
        &mut self,
        expected_revision: u64,
        resource_mode: ResourceMode,
        resources: Vec<ConnectionResource>,
    ) -> Result<bool, SourceConnectionError> {
        self.check_mutation(expected_revision)?;
        let resources = normalize_resources(resources)?;
        if self.resource_mode == resource_mode && self.resources == resources {
            return Ok(false);
        }
        let scope_changed = self.resource_mode != resource_mode
            || self
                .resources
                .iter()
                .map(ConnectionResource::handle)
                .ne(resources.iter().map(ConnectionResource::handle));
        self.advance(scope_changed)?;
        self.resource_mode = resource_mode;
        self.resources = resources;
        Ok(true)
    }

    pub fn reconcile_inventory(
        &mut self,
        expected_revision: u64,
        resources: Vec<ConnectionResource>,
    ) -> Result<bool, SourceConnectionError> {
        if self.resource_mode != ResourceMode::AllAvailable {
            return Err(SourceConnectionError::InvalidResource);
        }
        self.configure(expected_revision, ResourceMode::AllAvailable, resources)
    }

    pub fn update_native_subject(
        &mut self,
        expected_revision: u64,
        fingerprint: String,
    ) -> Result<bool, SourceConnectionError> {
        self.check_mutation(expected_revision)?;
        if !self.requires_native_subject() || !valid_fingerprint(&fingerprint) {
            return Err(SourceConnectionError::InvalidSubject);
        }
        if self.native_subject_fingerprint.as_deref() == Some(&fingerprint) {
            return Ok(false);
        }
        self.advance(true)?;
        self.native_subject_fingerprint = Some(fingerprint);
        self.state = SourceState::Ready;
        Ok(true)
    }

    pub fn disconnect(&mut self, expected_revision: u64) -> Result<bool, SourceConnectionError> {
        if self.revision != expected_revision {
            return Err(SourceConnectionError::Conflict);
        }
        if self.state == SourceState::Disconnected {
            return Ok(false);
        }
        self.advance(true)?;
        self.state = SourceState::Disconnected;
        Ok(true)
    }

    fn check_mutation(&self, expected_revision: u64) -> Result<(), SourceConnectionError> {
        if self.revision != expected_revision {
            return Err(SourceConnectionError::Conflict);
        }
        if self.state == SourceState::Disconnected {
            return Err(SourceConnectionError::Disconnected);
        }
        Ok(())
    }

    fn advance(&mut self, source_changed: bool) -> Result<(), SourceConnectionError> {
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(SourceConnectionError::RevisionExhausted)?;
        if revision > i64::MAX as u64 {
            return Err(SourceConnectionError::RevisionExhausted);
        }
        let authority = if source_changed {
            self.source_authority
                .advance()
                .ok_or(SourceConnectionError::RevisionExhausted)?
        } else {
            self.source_authority
        };
        self.revision = revision;
        self.source_authority = authority;
        Ok(())
    }

    fn requires_native_subject(&self) -> bool {
        matches!(
            self.connector_id.as_str(),
            "calendar.event_kit" | "calendar.android"
        )
    }
}

fn normalize_resources(
    mut resources: Vec<ConnectionResource>,
) -> Result<Vec<ConnectionResource>, SourceConnectionError> {
    if resources.len() > MAX_RESOURCES {
        return Err(SourceConnectionError::InvalidResource);
    }
    for resource in &resources {
        resource.validate()?;
    }
    resources.sort_by(|left, right| left.handle.cmp(&right.handle));
    if resources
        .windows(2)
        .any(|pair| pair[0].handle == pair[1].handle)
    {
        return Err(SourceConnectionError::InvalidResource);
    }
    Ok(resources)
}

fn valid_fingerprint(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
