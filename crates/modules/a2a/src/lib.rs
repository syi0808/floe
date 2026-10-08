//! Transport-independent A2A exchange contracts and peer identity mapping.
//!
//! This crate does not implement an HTTP binding, authenticate a transport,
//! select Manager/Expert policy, or own host Task state. A binding must provide
//! an already verified peer identity and call the host Task owner.

mod exchange;
mod mapping;
mod ports;

pub use exchange::{
    A2A_EXCHANGE_CONTRACT_VERSION, A2aArtifact, A2aArtifactPart, A2aEnvelope, A2aFailure,
    A2aMessage, A2aPeerAgentId, A2aPeerContextId, A2aPeerId, A2aPeerMessageId, A2aPeerTaskId,
    A2aProtocolPolicy, A2aTaskObservation, MAX_A2A_ARTIFACT_BYTES, MAX_A2A_ARTIFACT_PARTS,
    MAX_A2A_ARTIFACTS, MAX_A2A_ENVELOPE_BYTES, MAX_A2A_EXTENSIONS, MAX_A2A_ID_BYTES,
    MAX_A2A_MESSAGE_BYTES, MAX_A2A_TOTAL_ARTIFACT_BYTES,
};
pub use mapping::{
    A2aInboundAdmission, A2aInboundMapping, AuthenticatedPeerAgent, map_inbound_message,
};
pub use ports::{A2aPeerExchangePort, HostedTaskAdmission, HostedTaskPort};
