//! Domain-independent bounded model values shared by reasoning and local transforms.
mod schema;
mod wire;

pub use schema::{ModelContractError, ModelOutputFormat, ModelSchema, strict_json, validate_json};
pub use wire::*;

mod live;
pub use live::{LiveModelCapabilities, LiveModelCapability, LiveModelPlan, LiveTransportLimits};

mod binding;
pub use binding::{ModelBindingDigest, ModelSelectionCommitment, ProcessingBoundary};

mod live_events;
pub use live_events::{
    LiveAppendKind, LiveAppendPayload, LiveEndEvidence, LiveEventKey, LiveEventKind,
    LiveProviderEvent, LiveSpeaker, LiveUsageObservation, MAX_LIVE_ID_BYTES, MAX_LIVE_TEXT_BYTES,
};

pub use live::{LiveContextPayload, MAX_LIVE_CONTEXT_BYTES};
