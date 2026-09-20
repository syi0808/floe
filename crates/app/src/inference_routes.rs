//! Injection site for the canonical root model owner.
//!
//! The judgment — whether a saved connection exists, whether it may be bound to
//! this caller, and what the paired server offers — belongs to Inference and the
//! provider adapters. This type only supplies the verified caller identity and
//! the concrete ports. Nothing here resolves a route before admission.

#[derive(Clone, Copy, Default)]
pub struct HostInferenceRoutes;

/// Current exact-recipient authority for the canonical root model path.
///
/// Every recipient check reloads the current saved connection bound to the
/// verified caller; no recipient list is copied at construction. Production
/// (`None`) re-reads the host keychain slot. A test-only injected connection
/// (`Some`) is re-read from a fixed store with the same per-check shape.
pub fn root_recipient_authority(
    person_id: &str,
    device_id: &str,
    saved: Option<floe_inference::SavedServerConnection>,
) -> floe_provider_adapters::control::SavedConnectionRecipientAuthority<
    floe_provider_adapters::control::CurrentSavedConnectionStore,
> {
    use floe_provider_adapters::control::{
        CurrentSavedConnectionStore, FixedSavedConnectionStore, SavedConnectionRecipientAuthority,
        SavedServerConnectionStore,
    };
    let store = match saved {
        Some(saved) => {
            CurrentSavedConnectionStore::Fixed(FixedSavedConnectionStore::fixed(Some(saved)))
        }
        None => CurrentSavedConnectionStore::Keychain(SavedServerConnectionStore),
    };
    SavedConnectionRecipientAuthority::new(store, person_id.to_owned(), device_id.to_owned())
}

impl HostInferenceRoutes {
    /// Canonical root model provider from verified caller identity.
    ///
    /// The server leg comes only from the saved connection the product
    /// supplied for this turn, or else the host keychain slot, admitted
    /// against this person/device after admission. No pre-resolved route
    /// exists to consult for model selection.
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
}
