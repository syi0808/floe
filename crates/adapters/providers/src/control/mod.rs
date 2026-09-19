//! Control-plane transports: pairing, enrollment and authority checks.

pub mod authorization;
pub mod recipient_authority;
pub mod server_connection;

pub use authorization::{
    CalendarChallengeParts, HttpRemoteControl, PairingConfirmationResponse, PairingIssuerResponse,
    PairingStartResponse, PairingStatusResponse, RemoteAuthorityEndpoint,
    RemoteAuthorizationClient, RemoteViewAuthorizationRequest, RemoteViewSourcePreviewResponse,
    calendar_query_sha256, parse_calendar_challenge,
};

pub use recipient_authority::{
    CurrentSavedConnectionStore, FixedSavedConnectionStore, SavedConnectionRecipientAuthority,
};
pub use server_connection::{SavedServerConnectionStore, load_saved_connection};
