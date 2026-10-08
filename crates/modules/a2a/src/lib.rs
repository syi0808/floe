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
    A2aProtocolPolicy, A2aTaskObservation, MAX_A2A_EXTENSIONS,
};
pub use mapping::{
    A2aInboundAdmission, A2aInboundMapping, AuthenticatedPeerAgent, map_inbound_message,
};
pub use ports::{A2aPeerExchangePort, HostedTaskAdmission, HostedTaskPort};
