use serde::{Deserialize, Serialize};

use crate::{ConnectionId, ConnectorId, ExecutionOwnerId, GrantValidationError, ResourceHandle};

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSelectionReference {
    pub connector_id: ConnectorId,
    pub connection_id: ConnectionId,
    pub execution_owner_id: ExecutionOwnerId,
    pub capability_id: String,
    pub resource: ResourceHandle,
    pub contract_version: u32,
}

impl SourceSelectionReference {
    pub fn validate(&self) -> Result<(), GrantValidationError> {
        ConnectorId::try_new(self.connector_id.as_str().to_owned())?;
        ConnectionId::try_new(self.connection_id.as_str().to_owned())?;
        ExecutionOwnerId::try_new(self.execution_owner_id.as_str().to_owned())?;
        ResourceHandle::try_new(self.resource.as_str().to_owned())?;
        if self.contract_version == 0
            || self.capability_id.is_empty()
            || self.capability_id.len() > 128
            || !self
                .capability_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            return Err(GrantValidationError::InvalidIdentifier("capability"));
        }
        Ok(())
    }
}
