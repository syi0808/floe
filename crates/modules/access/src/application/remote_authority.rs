use floe_kernel::{AgentFailure, PersonId};
use crate::ports::remote_grants::RemotePairingIdentity;

/// That the pairing an enrollment is made under is this Person's own.
pub fn admit_enrollment_pairing(
    person_id: PersonId,
    pairing: RemotePairingIdentity<'_>,
) -> Result<(), AgentFailure> {
    if pairing.person_id != person_id.to_string()
        || pairing.client_id.trim().is_empty()
        || pairing.device_id.trim().is_empty()
    {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

/// That the pairing a remote read runs under is this Person's own, from the
/// device they are running on.
///
/// A route paired for another device is not this run's, whatever it can reach.
pub fn admit_device_pairing(
    person_id: PersonId,
    device_id: &str,
    pairing: RemotePairingIdentity<'_>,
) -> Result<(), AgentFailure> {
    admit_enrollment_pairing(person_id, pairing)?;
    if pairing.device_id != device_id {
        return Err(AgentFailure::PolicyDenied);
    }
    Ok(())
}

