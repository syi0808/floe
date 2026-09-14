use uuid::Uuid;

use crate::{AppHost, CallerContext, HostError, HostServices, LocalIdentityProvider};

impl<Services: HostServices> AppHost<Services> {
    pub fn bootstrap(
        services: Services,
        identity: &dyn LocalIdentityProvider,
    ) -> Result<Self, HostError> {
        let caller = CallerContext::verified(identity.verified_local_identity()?, Uuid::new_v4())?;
        Ok(Self::with_caller(services, caller))
    }
}
