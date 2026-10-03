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

mod source_cleanup;
pub use source_cleanup::GatewaySourceCleanup;

mod integrations;
pub use integrations::GatewayIntegrationAdapter;
