//! Injection site for remote route selection.
//!
//! The judgment — whether a saved connection exists, whether it may be bound to
//! this caller, and what the paired server offers — belongs to Inference and the
//! provider adapters. This type only supplies the verified caller identity and
//! the concrete ports.

use crate::CallerContext;

#[derive(Clone, Copy, Default)]
pub struct HostInferenceRoutes;

/// Current exact-recipient authority for the canonical root model path,
/// without credentials.
///
/// Reports whether one exact external recipient still holds processing
/// authority/consent right now, from the admitted saved connection bound to
/// the verified caller. Device dispatch never consults it.
pub struct HostRecipientAuthority {
    consented_recipients: Vec<String>,
}

impl HostRecipientAuthority {
    /// Report exactly the consented recipients derived from the admitted
    /// saved connection. An empty list denies every external recipient:
    /// fail closed.
    pub fn admitted(recipients: Vec<String>) -> Self {
        Self {
            consented_recipients: recipients,
        }
    }
}

impl floe_access::ModelDispatchRecipientAuthority for HostRecipientAuthority {
    fn check_recipient(&self, recipient: &str) -> Result<(), floe_agent_contract::AgentFailure> {
        if self
            .consented_recipients
            .iter()
            .any(|allowed| allowed == recipient)
        {
            Ok(())
        } else {
            Err(floe_agent_contract::AgentFailure::PolicyDenied)
        }
    }
}

impl HostInferenceRoutes {
    /// Canonical root model provider from verified caller identity.
    ///
    /// The server leg comes only from the saved connection the product
    /// supplied for this turn, or else the host keychain slot, admitted
    /// against this person/device. The turn's pre-resolved `remote_route`
    /// is never consulted for model selection.
    pub fn root_model_provider(
        person_id: &str,
        device_id: &str,
        saved: Option<floe_inference::SavedServerConnection>,
    ) -> Result<
        floe_provider_adapters::models::RootModelProvider,
        floe_agent_contract::AgentFailure,
    > {
        let stored = match saved {
            Some(stored) => Some(stored),
            // No keychain item is device-only, not an error. A keychain
            // failure that is not "absent" fails closed like route
            // resolution does.
            None => floe_provider_adapters::control::load_saved_connection()?,
        };
        floe_provider_adapters::models::RootModelProvider::for_saved_connection(
            stored, person_id, device_id,
        )
    }

    pub async fn resolve(
        &self,
        caller: &CallerContext,
    ) -> Result<Option<crate::RemoteTurnRoute>, floe_agent_contract::AgentFailure> {
        let resolved = floe_inference::select_remote_route(
            &floe_provider_adapters::control::SavedServerConnectionStore,
            &floe_provider_adapters::models::RemoteModelRouteResolver,
            &caller.person_id().to_string(),
            caller.device_id(),
        )
        .await?;
        Ok(resolved.map(|resolved| crate::RemoteTurnRoute {
            route: resolved.route,
            calendar_connections: resolved.calendar_connections,
        }))
    }
}
