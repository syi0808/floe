mod agent_codec;
pub(crate) mod credentials;
pub(crate) mod http;
mod inference;
mod inference_wire;
pub(crate) mod json;
mod model_provider;
mod pairing;
pub use credentials::{GatewayCredentialError, GatewayCredentialStore};
pub use inference::{GatewayModelProvider, PreparedGatewayTransport};
pub use model_provider::{CompositeModelProvider, PreparedCompositeTransport};

mod proof;
mod source_preview;
pub use source_preview::GatewaySourcePreviewVerifier;

pub use pairing::GatewayPairingAdapter;

pub use proof::GatewayProofVerifier;

pub(crate) mod views;

pub mod calendar_mirror;
pub use calendar_mirror::GatewayCalendarMirrorClient;

mod source_cleanup;
pub use source_cleanup::GatewaySourceCleanup;

pub mod product_lease;
pub use product_lease::{ProductGatewayLease, ProductGatewayLeaseRegistry};

mod integrations;
pub use integrations::GatewayIntegrationAdapter;
