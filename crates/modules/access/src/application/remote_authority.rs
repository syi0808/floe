//! Which producer a Person's device enrolls with, and when.
//!
//! Enrolling this device with a paired server is an authority decision, not a
//! transport one: the producer answering has to be the one the Person pinned,
//! the pairing has to be theirs, and the pin is recorded before anything is
//! signed. The transport only fetches; the store only keeps keys and pins.

use floe_kernel::{AgentFailure, PersonId};

use crate::application::remote_view::{RemoteProducerIdentity, producer_is_pinned};
use crate::ports::remote_authorization::{
    RemoteAuthorityStore, RemoteAuthorityTransport, RemoteEnrollmentStatus, RemoteOwnerPublicKey,
};
use crate::ports::remote_grants::{RemoteCallWindow, RemotePairingIdentity};

/// What the Person is shown about a server before they enroll with it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteAuthorityInspection {
    pub producer: RemoteProducerIdentity,
    /// The key this Person would enroll under, when their vault is open.
    pub owner: Option<RemoteOwnerPublicKey>,
}

/// What an enrollment settled as.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteAuthorityEnrollment {
    pub producer: RemoteProducerIdentity,
    pub enrollment: RemoteEnrollmentStatus,
    pub owner: RemoteOwnerPublicKey,
}

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

/// Ask a server who it is, so the Person can decide whether to pin it.
///
/// Nothing is pinned or stored here; a locked vault simply has no owner key to
/// show alongside it.
pub async fn inspect_remote_authority(
    transport: &dyn RemoteAuthorityTransport,
    store: Option<&dyn RemoteAuthorityStore>,
    window: &RemoteCallWindow,
) -> Result<RemoteAuthorityInspection, AgentFailure> {
    let producer = transport.producer_identity(window).await?;
    let owner = match store {
        Some(store) => Some(store.owner_public_key().await?),
        None => None,
    };
    Ok(RemoteAuthorityInspection { producer, owner })
}

/// Pin the producer the Person reviewed, then enroll this device with it.
///
/// The producer is observed again rather than taken on the caller's word: a
/// server that changed identity while the Person was deciding is not the one
/// they reviewed. The pin is recorded before the enrollment so that nothing is
/// signed for a producer this Person has not accepted.
pub async fn review_and_enroll_remote_authority(
    transport: &dyn RemoteAuthorityTransport,
    store: &dyn RemoteAuthorityStore,
    person_id: PersonId,
    pairing: RemotePairingIdentity<'_>,
    reviewed: RemoteProducerIdentity,
    window: &RemoteCallWindow,
) -> Result<RemoteAuthorityEnrollment, AgentFailure> {
    admit_enrollment_pairing(person_id, pairing)?;
    let observed = transport.producer_identity(window).await?;
    producer_is_pinned(&reviewed, &observed)?;
    store.pin_producer(reviewed.clone()).await?;
    let enrollment = transport
        .enroll(pairing.client_id, pairing.device_id, &reviewed, window)
        .await?;
    Ok(RemoteAuthorityEnrollment {
        producer: reviewed,
        enrollment,
        owner: store.owner_public_key().await?,
    })
}

/// What the producer now reports about an enrollment already begun.
pub async fn remote_enrollment_status(
    transport: &dyn RemoteAuthorityTransport,
    enrollment_id: &str,
    window: &RemoteCallWindow,
) -> Result<RemoteEnrollmentStatus, AgentFailure> {
    transport.enrollment_status(enrollment_id, window).await
}
