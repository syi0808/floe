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

#[cfg(test)]
mod tests {
    use super::*;

    fn reference() -> SourceSelectionReference {
        SourceSelectionReference {
            connector_id: ConnectorId::try_new("floe.connector.calendar").unwrap(),
            connection_id: ConnectionId::try_new("calendar-account").unwrap(),
            execution_owner_id: ExecutionOwnerId::try_new("device:example").unwrap(),
            capability_id: "calendar.timeline".into(),
            resource: ResourceHandle::try_new("calendar:personal").unwrap(),
            contract_version: 1,
        }
    }

    #[test]
    fn round_trip_and_bounds() {
        let reference = reference();
        reference.validate().unwrap();
        let decoded: SourceSelectionReference =
            serde_json::from_slice(&serde_json::to_vec(&reference).unwrap()).unwrap();
        assert_eq!(decoded, reference);
        let mut invalid = reference;
        invalid.contract_version = 0;
        assert!(invalid.validate().is_err());
        invalid.contract_version = 1;
        invalid.capability_id = "x".repeat(129);
        assert!(invalid.validate().is_err());
        assert!(ResourceHandle::try_new("*").is_err());
    }
}
