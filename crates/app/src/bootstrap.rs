use std::path::Path;

use uuid::Uuid;

use crate::{AppHost, CallerContext, HostError, HostServices, LocalIdentityProvider};

impl<Services: HostServices> AppHost<Services> {
    pub fn bootstrap(
        services: Services,
        identity: &dyn LocalIdentityProvider,
    ) -> Result<Self, HostError> {
        Self::bootstrap_claim(services, identity.verified_local_identity()?)
    }

    pub fn bootstrap_claim(
        services: Services,
        identity: crate::LocalIdentityClaim,
    ) -> Result<Self, HostError> {
        let caller = CallerContext::verified(identity, runtime_epoch())?;
        Ok(Self::with_caller(services, caller))
    }

    pub fn bootstrap_local_or_legacy(
        services: Services,
        database_path: &Path,
    ) -> Result<Self, HostError> {
        let Some(identity) = local_identity_for_database(database_path)? else {
            return Ok(Self::legacy(services));
        };
        Self::bootstrap_claim(services, identity)
    }
}

fn runtime_epoch() -> u64 {
    let value = Uuid::new_v4().as_u128();
    (((value >> 64) as u64 ^ value as u64) & i64::MAX as u64).max(1)
}

pub(crate) fn local_identity_for_database(
    database_path: &Path,
) -> Result<Option<crate::LocalIdentityClaim>, HostError> {
    floe_provider_adapters::local_identity_for_database(database_path)
        .map(|identity| {
            identity.map(|identity| crate::LocalIdentityClaim {
                person_id: identity.person_id,
                device_id: identity.device_id,
            })
        })
        .map_err(|failure| match failure {
            floe_provider_adapters::LocalIdentityError::Invalid => HostError::InvalidIdentity,
            floe_provider_adapters::LocalIdentityError::Unavailable => {
                HostError::IdentityUnavailable
            }
        })
}
