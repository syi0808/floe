//! Injection site for the canonical root model owner.
//!
//! The judgment — whether a saved connection exists, whether it may be bound to
//! this caller, and what the paired server offers — belongs to Inference and the
//! provider adapters. This type only supplies the verified caller identity and
//! the concrete ports. Nothing here resolves a route before admission.

use crate::turn_request::SavedConnectionSource;

#[derive(Clone, Copy, Default)]
pub struct HostInferenceRoutes;

/// Current exact-recipient authority for the canonical root model path.
///
/// Every recipient check reloads the current saved connection bound to the
/// verified caller; no recipient list is copied at construction. Production
/// (`HostSlot`) re-reads the host keychain slot. A test-only injected
/// connection (`Fixed`) is re-read from a fixed store with the same
/// per-check shape.
///
/// Crate-internal: the credential-source type is not part of the public turn
/// contract.
pub(crate) fn root_recipient_authority(
    person_id: &str,
    device_id: &str,
    saved: SavedConnectionSource,
) -> floe_provider_adapters::control::SavedConnectionRecipientAuthority<
    floe_provider_adapters::control::CurrentSavedConnectionStore,
> {
    use floe_provider_adapters::control::{
        CurrentSavedConnectionStore, SavedConnectionRecipientAuthority, SavedServerConnectionStore,
    };
    #[cfg(test)]
    use floe_provider_adapters::control::FixedSavedConnectionStore;
    let store = match saved {
        #[cfg(test)]
        SavedConnectionSource::Fixed(saved) => {
            CurrentSavedConnectionStore::Fixed(FixedSavedConnectionStore::fixed(saved))
        }
        SavedConnectionSource::HostSlot => {
            CurrentSavedConnectionStore::Keychain(SavedServerConnectionStore)
        }
    };
    SavedConnectionRecipientAuthority::new(store, person_id.to_owned(), device_id.to_owned())
}

impl HostInferenceRoutes {
    /// Canonical root model provider from verified caller identity.
    ///
    /// The server leg comes only from the saved connection admitted for this
    /// turn, or else the host keychain slot, admitted against this
    /// person/device after admission. No pre-resolved route exists to consult
    /// for model selection.
    ///
    /// Crate-internal: the credential-source type is not part of the public
    /// turn contract.
    pub(crate) fn root_model_provider(
        person_id: &str,
        device_id: &str,
        saved: SavedConnectionSource,
    ) -> Result<
        floe_provider_adapters::models::RootModelProvider,
        floe_agent_contract::AgentFailure,
    > {
        let stored = match saved {
            #[cfg(test)]
            SavedConnectionSource::Fixed(stored) => stored,
            // No keychain item is device-only, not an error. A keychain
            // failure that is not "absent" fails closed like route
            // resolution does.
            SavedConnectionSource::HostSlot => {
                floe_provider_adapters::control::load_saved_connection()?
            }
        };
        floe_provider_adapters::models::RootModelProvider::for_saved_connection(
            stored, person_id, device_id,
        )
    }
}
