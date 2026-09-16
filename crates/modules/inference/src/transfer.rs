//! Whether a run's input may leave this device.
//!
//! Running remotely is not itself consent. A route says where computation
//! happens; who may receive the input is a separate answer, and the Person has
//! to have granted it to that recipient.

use floe_agent_contract::{ModelPlacement, TransferConsent};

/// The recipient a run's route resolves to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteRecipient {
    /// The route leaves the Person's own paired server for a third party.
    pub external: bool,
    /// The Person allowed that third party to receive this input.
    pub allowed: bool,
}

/// The consent a run's model call stands under.
pub fn external_transfer_consent(
    placement: ModelPlacement,
    recipient: Option<RouteRecipient>,
) -> TransferConsent {
    if placement == ModelPlacement::Remote
        && recipient.is_some_and(|recipient| recipient.external && recipient.allowed)
    {
        TransferConsent::Granted
    } else {
        TransferConsent::NotGranted
    }
}
